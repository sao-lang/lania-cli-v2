//! Scala 3 Provider：官方发行包。
//!
//! - 版本清单：`api.github.com/repos/scala/scala3/releases`
//! - 产物：`scala3-<version>.zip`
//! - 目录：zip 内是 `scala3-<version>/`，入口在 `bin/`
//! - 环境：`SCALA_HOME`；运行需要系统/自管 JDK

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};

pub const ID: &str = "scala";
const RELEASES: &str = "https://api.github.com/repos/scala/scala3/releases?per_page=100";
const RELEASE: &str = "https://api.github.com/repos/scala/scala3/releases/tags";

pub struct ScalaProvider;

impl ScalaProvider {
    pub fn new() -> Self {
        Self
    }

    fn asset_name(version: &str) -> String {
        format!("scala3-{version}.zip")
    }
}

impl RuntimeProvider for ScalaProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/scala/scala3/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["scala", "scalac"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let payload = ctx.http.json(RELEASES)?;
        let items = payload
            .as_array()
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "Scala releases payload is not an array".to_string(),
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
                detail: format!("Scala release {} has no assets", release.tag),
            })?;
        let asset = assets
            .iter()
            .find(|asset| {
                asset
                    .get("name")
                    .and_then(|value| value.as_str())
                    .is_some_and(|name| name == wanted)
            })
            .ok_or_else(|| ProviderError::Unsupported(format!("no asset named {wanted}")))?;
        let url = asset
            .get("browser_download_url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "Scala asset has no browser_download_url".to_string(),
            })?;
        let sha256 = assets
            .iter()
            .find(|asset| {
                asset
                    .get("name")
                    .and_then(|value| value.as_str())
                    .is_some_and(|name| name == "sha256sum.txt")
            })
            .and_then(|asset| asset.get("browser_download_url"))
            .and_then(|value| value.as_str())
            .and_then(|url| ctx.http.text(url).ok())
            .and_then(|text| crate::checksum::find_in_manifest(&text, &wanted));
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
        Ok(Layout::new("bin", self.entrypoints()).with_env("SCALA_HOME", "{root}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_matches_scala_release() {
        assert_eq!(ScalaProvider::asset_name("3.3.9"), "scala3-3.3.9.zip");
    }
}
