//! Python Provider：python-build-standalone（astral-sh）的独立可重定位发行包。
//!
//! 为什么不用 python.org：官方只提供安装器/源码，没有可解压的便携产物；
//! python-build-standalone 是 uv / rye / mise 等工具的共同上游，产物稳定。
//!
//! - 版本清单：`api.github.com/repos/astral-sh/python-build-standalone/releases?per_page=100`
//! - 产物：`cpython-<ver>+<date>-<triple>-install_only.tar.gz`
//! - 目录：单层 `python/`，实际入口在 `bin/`（Windows 在根上）
//! - 入口：python / pip

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "python";
const REPO: &str = "astral-sh/python-build-standalone";
/// 这个仓库的 release payload 极大：`per_page=100` 会被 GitHub 直接 504。
/// 用较小的页 + 翻页拿到足够多的版本；`MAX_PAGES` 取 8 是为了覆盖近几个
/// Python 次版本（3.12/3.13 的补丁 release 都排在很后面）。
const PAGE_SIZE: usize = 20;
const MAX_PAGES: usize = 8;

pub struct PythonProvider {
    platform: Platform,
}

impl PythonProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    /// python-build-standalone 的目标三元组。
    fn target_triple(&self) -> Result<&'static str, ProviderError> {
        let triple = match (self.platform.os, self.platform.arch) {
            (Os::Linux, Arch::X64) => "x86_64-unknown-linux-gnu",
            (Os::Linux, Arch::Arm64) => "aarch64-unknown-linux-gnu",
            (Os::Macos, Arch::X64) => "x86_64-apple-darwin",
            (Os::Macos, Arch::Arm64) => "aarch64-apple-darwin",
            // Windows 的独立发行包只有一个 x86_64 变体（arm64 通过 x64 模拟运行）。
            (Os::Windows, _) => "x86_64-pc-windows-msvc",
        };
        Ok(triple)
    }

    /// 从 release asset 名字里抽出规范化版本号，如
    /// `cpython-3.12.4+20240726-x86_64-unknown-linux-gnu-install_only.tar.gz` → `3.12.4`。
    fn version_from_asset(name: &str) -> Option<String> {
        let rest = name.strip_prefix("cpython-")?;
        let version = rest.split('+').next()?;
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        // 三段必须都是纯数字，借此排除 `3.13.0b1` 之类的预发布。
        parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
            .then(|| version.to_string())
    }
}

impl RuntimeProvider for PythonProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "python-build-standalone"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        if self.platform.os == Os::Windows {
            &["python", "pip"]
        } else {
            &["python3", "python", "pip"]
        }
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let triple = self.target_triple()?;
        let suffix = format!("-{triple}-install_only.tar.gz");
        let mut out: Vec<Release> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 逐页拉取：第一页失败直接报错，后续页失败则用已有结果（版本清单够用即可）。
        for page in 1..=MAX_PAGES {
            let url = format!(
                "https://api.github.com/repos/{REPO}/releases?per_page={PAGE_SIZE}&page={page}"
            );
            let payload = match ctx.http.json(&url) {
                Ok(payload) => payload,
                Err(_) if !out.is_empty() => break,
                Err(error) => return Err(error.into()),
            };
            let Some(items) = payload.as_array() else {
                return Err(ProviderError::Unexpected {
                    detail: "GitHub releases payload is not an array".to_string(),
                });
            };
            if items.is_empty() {
                break;
            }
            for release in items {
                let Some(assets) = release.get("assets").and_then(|value| value.as_array()) else {
                    continue;
                };
                let tag = release
                    .get("tag_name")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
                let prerelease = release
                    .get("prerelease")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                // 每个 release 同时包含多个 Python 次版本（3.10 / 3.11 / 3.12 …），
                // 因此必须遍历全部 asset，而不是只取第一个匹配项。
                for asset in assets {
                    let Some(name) = asset.get("name").and_then(|value| value.as_str()) else {
                        continue;
                    };
                    if !name.ends_with(&suffix) {
                        continue;
                    }
                    let Some(version) = Self::version_from_asset(name) else {
                        continue;
                    };
                    // 新 → 旧遍历：同一 `X.Y.Z` 只保留第一次出现（即最新构建）。
                    if !seen.insert(version.clone()) {
                        continue;
                    }
                    out.push(Release {
                        prerelease,
                        version,
                        tag: tag.clone(),
                    });
                }
            }
        }
        if out.is_empty() {
            return Err(ProviderError::Unexpected {
                detail: format!("no release of {REPO} ships a {triple} install_only archive"),
            });
        }
        // GitHub 的 release 顺序是「新 → 旧」，但同一版本会有多个 patch release，
        // 这里按版本号真正降序排序，保证 `pick_best` 拿到最新的补丁版本。
        out.sort_by(|a, b| {
            let left = crate::VersionTag::parse(&a.version).map(|v| v.parts);
            let right = crate::VersionTag::parse(&b.version).map(|v| v.parts);
            right.cmp(&left)
        });
        Ok(out)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        // 重新拉一次 release 详情，拿到该版本 asset 的精确地址与 sha256 digest。
        let release_json = ctx.http.json(&format!(
            "https://api.github.com/repos/{REPO}/releases/tags/{}",
            release.tag
        ))?;
        let triple = self.target_triple()?;
        // 同一 release 里既有 `cpython-3.12.15+<date>-...`、也有 `3.12.15` 的
        // stripped/freethreaded 变体，必须按「版本 + 平台 + install_only」精确匹配。
        let prefix = format!("cpython-{}+", release.version);
        let suffix = format!("-{triple}-install_only.tar.gz");
        let assets = release_json
            .get("assets")
            .and_then(|value| value.as_array())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: format!("release {} has no assets", release.tag),
            })?;
        let asset = assets
            .iter()
            .find(|asset| {
                asset
                    .get("name")
                    .and_then(|value| value.as_str())
                    .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(&suffix))
            })
            .ok_or_else(|| {
                ProviderError::Unsupported(format!(
                    "no {triple} install_only asset for python {}",
                    release.version
                ))
            })?;
        let file_name = asset
            .get("name")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "asset has no name".to_string(),
            })?
            .to_string();

        let url = asset
            .get("browser_download_url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "asset has no browser_download_url".to_string(),
            })?;
        // GitHub 从 2024 年起为 asset 提供 `digest: sha256:<hex>`。
        let sha256 = asset
            .get("digest")
            .and_then(|value| value.as_str())
            .and_then(|value| value.strip_prefix("sha256:"))
            .map(|value| value.to_ascii_lowercase());
        Ok(ArtifactPlan {
            url: url.to_string(),
            sha256,
            signature: None,
            file_name,
        })
    }

    fn install_layout(
        &self,
        extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        let root =
            crate::archive::single_root(extracted).map_err(|error| ProviderError::Layout {
                detail: error.to_string(),
            })?;
        // `install_only` 产物解压后是 `python/{bin,lib,include}`。
        let bin_dir = if root.join("bin").is_dir() {
            std::path::PathBuf::from("bin")
        } else {
            std::path::PathBuf::from(".")
        };
        Ok(Layout::new(bin_dir, self.entrypoints()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Libc, RuntimeProvider};

    fn platform(os: Os, arch: Arch) -> Platform {
        Platform {
            os,
            arch,
            libc: if os == Os::Linux {
                Libc::Gnu
            } else {
                Libc::None
            },
        }
    }

    #[test]
    fn extracts_version_from_asset_name() {
        let name = "cpython-3.12.4+20240726-x86_64-unknown-linux-gnu-install_only.tar.gz";
        assert_eq!(
            PythonProvider::version_from_asset(name).as_deref(),
            Some("3.12.4")
        );
        assert!(PythonProvider::version_from_asset(
            "cpython-3.13.0b1+20240726-x86_64-unknown-linux-gnu-install_only.tar.gz"
        )
        .is_none());
    }

    #[test]
    fn windows_has_no_python3_entrypoint() {
        let provider = PythonProvider::new(platform(Os::Windows, Arch::X64));
        assert_eq!(provider.entrypoints(), &["python", "pip"]);
        let unix = PythonProvider::new(platform(Os::Linux, Arch::X64));
        assert!(unix.entrypoints().contains(&"python3"));
    }

    #[test]
    fn target_triples_cover_supported_platforms() {
        let cases = [
            (platform(Os::Linux, Arch::X64), "x86_64-unknown-linux-gnu"),
            (
                platform(Os::Linux, Arch::Arm64),
                "aarch64-unknown-linux-gnu",
            ),
            (platform(Os::Macos, Arch::Arm64), "aarch64-apple-darwin"),
            (platform(Os::Windows, Arch::X64), "x86_64-pc-windows-msvc"),
        ];
        for (platform, expected) in cases {
            assert_eq!(
                PythonProvider::new(platform).target_triple().unwrap(),
                expected
            );
        }
    }
}
