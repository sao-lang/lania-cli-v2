//! GitHub Releases 的共享解析逻辑。
//!
//! 第三批里有多种语言直接以 GitHub Releases 作为官方分发渠道，
//! 这里收敛「列版本 / 取资产 / 读 sidecar 校验和」三件事，
//! 让各 Provider 只负责自己的命名规则与平台矩阵。
//!
//! `releases` 只用于清单（不含资产）；`assets` 按 tag 取单个 release，
//! 避免每次解析产物都拉一整页 100 条 release（GitHub 匿名配额只有 60 次/小时）。

use super::model::{ArtifactPlan, Release, ResolveContext};
use super::ProviderError;

const API: &str = "https://api.github.com";

/// 一个可下载资产。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

/// 拉取 release 清单；`normalize` 把上游 tag 转成安装目录用的版本号，
/// 返回 `None` 表示该 tag 不是可安装版本（预发布、CI 产物等）。
pub fn releases(
    repo: &str,
    ctx: &ResolveContext<'_>,
    normalize: impl Fn(&str) -> Option<String>,
) -> Result<Vec<Release>, ProviderError> {
    let payload = ctx
        .http
        .json(&format!("{API}/repos/{repo}/releases?per_page=100"))?;
    let items = payload
        .as_array()
        .ok_or_else(|| ProviderError::Unexpected {
            detail: format!("{repo} releases payload is not an array"),
        })?;
    Ok(items
        .iter()
        .filter_map(|item| {
            let tag = item.get("tag_name")?.as_str()?;
            let version = normalize(tag)?;
            Some(Release {
                version,
                prerelease: item
                    .get("prerelease")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
                tag: tag.to_string(),
            })
        })
        .collect())
}

/// 按 tag 取单个 release 的资产列表。
pub fn assets(
    repo: &str,
    tag: &str,
    ctx: &ResolveContext<'_>,
) -> Result<Vec<Asset>, ProviderError> {
    let payload = ctx
        .http
        .json(&format!("{API}/repos/{repo}/releases/tags/{tag}"))?;
    let items = payload
        .get("assets")
        .and_then(|value| value.as_array())
        .ok_or_else(|| ProviderError::Unexpected {
            detail: format!("{repo} release {tag} has no assets"),
        })?;
    Ok(items
        .iter()
        .filter_map(|item| {
            Some(Asset {
                name: item.get("name")?.as_str()?.to_string(),
                url: item.get("browser_download_url")?.as_str()?.to_string(),
            })
        })
        .collect())
}

pub fn find<'a>(assets: &'a [Asset], name: &str) -> Option<&'a Asset> {
    assets.iter().find(|asset| asset.name == name)
}

/// 按条件挑资产（资产名带构建号时用后缀匹配比硬拼全名稳）。
pub fn find_where<'a>(assets: &'a [Asset], predicate: impl Fn(&str) -> bool) -> Option<&'a Asset> {
    assets.iter().find(|asset| predicate(&asset.name))
}

/// 组装下载计划；`checksum_name` 指向同 release 里的 `.sha256` / `.sha256sum` 资产。
pub fn plan(
    assets: &[Asset],
    file_name: &str,
    checksum_name: Option<&str>,
    ctx: &ResolveContext<'_>,
) -> Result<ArtifactPlan, ProviderError> {
    let asset = find(assets, file_name)
        .ok_or_else(|| ProviderError::Unsupported(format!("no release asset named {file_name}")))?;
    let sha256 = checksum_name
        .and_then(|name| find(assets, name))
        .and_then(|checksum| ctx.http.text(&checksum.url).ok())
        .and_then(|text| {
            crate::checksum::find_in_manifest(&text, file_name).or_else(|| {
                // 有些上游的 sidecar 只有一个裸摘要，没有文件名。
                text.split_whitespace()
                    .next()
                    .filter(|token| {
                        token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit())
                    })
                    .map(ToOwned::to_owned)
            })
        });
    Ok(ArtifactPlan {
        url: asset.url.clone(),
        sha256,
        signature: None,
        file_name: file_name.to_string(),
    })
}

/// 只接受 `1.2.3` 这种纯数字点分版本（会过滤 rc / beta / preview / CI tag）。
pub fn plain_version(tag: &str) -> Option<String> {
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    let parts: Vec<&str> = tag.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 || !parts.iter().all(|part| is_number(part)) {
        return None;
    }
    Some(tag.to_string())
}

fn is_number(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_version_filters_preview_tags() {
        assert_eq!(plain_version("v1.19.2").as_deref(), Some("1.19.2"));
        assert_eq!(plain_version("4.3.7").as_deref(), Some("4.3.7"));
        assert_eq!(plain_version("v2.113.1-rc1"), None);
        assert_eq!(plain_version("main-latest"), None);
        assert_eq!(plain_version("0.5.2").as_deref(), Some("0.5.2"));
    }
}
