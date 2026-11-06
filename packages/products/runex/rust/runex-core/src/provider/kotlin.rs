//! Kotlin Provider：JetBrains 官方编译器发行包。
//!
//! - 版本清单：`api.github.com/repos/JetBrains/kotlin/releases`
//! - 产物：`kotlin-compiler-<version>.zip`
//! - 目录：zip 内是 `kotlinc/`，入口在 `bin/`
//! - 环境：`KOTLIN_HOME`；运行需要系统/自管 JDK
//! - 注意：这里只装 Kotlin 编译器，不附带 JDK。

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};

pub const ID: &str = "kotlin";
const RELEASES: &str = "https://api.github.com/repos/JetBrains/kotlin/releases?per_page=100";
const RELEASE: &str = "https://api.github.com/repos/JetBrains/kotlin/releases/tags";

pub struct KotlinProvider;

impl KotlinProvider {
    pub fn new() -> Self {
        Self
    }

    fn asset_name(version: &str) -> String {
        format!("kotlin-compiler-{version}.zip")
    }
}

impl RuntimeProvider for KotlinProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/JetBrains/kotlin/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["kotlin", "kotlinc"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let payload = ctx.http.json(RELEASES)?;
        let items = payload
            .as_array()
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "Kotlin releases payload is not an array".to_string(),
            })?;
        Ok(items
            .iter()
            .filter_map(|release| {
                let tag = release.get("tag_name")?.as_str()?;
                let version = tag.trim_start_matches('v').to_string();
                Some(Release {
                    prerelease: release
                        .get("prerelease")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                    version,
                    tag: tag.to_string(),
                })
            })
            .collect())
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let wanted = Self::asset_name(&release.version);
        let detail = ctx.http.json(&format!("{RELEASE}/{}", release.tag))?;
        let assets = detail
            .get("assets")
            .and_then(|value| value.as_array())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: format!("Kotlin release {} has no assets", release.tag),
            })?;
        let find = |name: &str| {
            assets.iter().find(|asset| {
                asset
                    .get("name")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| value == name)
            })
        };
        let asset = find(&wanted)
            .ok_or_else(|| ProviderError::Unsupported(format!("no asset named {wanted}")))?;
        let url = asset
            .get("browser_download_url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "Kotlin asset has no browser_download_url".to_string(),
            })?;
        let checksum_name = format!("{wanted}.sha256");
        let sha256 = find(&checksum_name)
            .and_then(|asset| asset.get("browser_download_url"))
            .and_then(|value| value.as_str())
            .and_then(|url| ctx.http.text(url).ok())
            .and_then(|text| text.split_whitespace().next().map(ToOwned::to_owned));
        Ok(ArtifactPlan {
            url: url.to_string(),
            sha256,
            signature: None,
            file_name: wanted,
        })
    }

    fn install_layout(
        &self,
        _extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        Ok(Layout::new("bin", self.entrypoints()).with_env("KOTLIN_HOME", "{root}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_matches_kotlin_release() {
        assert_eq!(
            KotlinProvider::asset_name("2.4.21"),
            "kotlin-compiler-2.4.21.zip"
        );
    }
}
