//! Deno Provider：GitHub Releases 上的单文件二进制（zip 包裹）。
//!
//! - 版本清单：`api.github.com/repos/denoland/deno/releases?per_page=100`
//! - 产物：`deno-<rust triple>.zip`，解压后只有一个 `deno(.exe)`
//! - 目录：扁平放置，入口在版本目录根上

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "deno";
const RELEASES: &str = "https://api.github.com/repos/denoland/deno/releases?per_page=100";

pub struct DenoProvider {
    platform: Platform,
}

impl DenoProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn triple(&self) -> Result<&'static str, ProviderError> {
        Ok(match (self.platform.os, self.platform.arch) {
            (Os::Linux, Arch::X64) => "x86_64-unknown-linux-gnu",
            (Os::Linux, Arch::Arm64) => "aarch64-unknown-linux-gnu",
            (Os::Macos, Arch::X64) => "x86_64-apple-darwin",
            (Os::Macos, Arch::Arm64) => "aarch64-apple-darwin",
            (Os::Windows, Arch::X64) => "x86_64-pc-windows-msvc",
            // Deno 尚未发布 Windows arm64 产物。
            (Os::Windows, Arch::Arm64) => {
                return Err(ProviderError::Unsupported(
                    "deno does not publish windows-arm64 builds".to_string(),
                ))
            }
        })
    }

    fn asset_name(&self) -> Result<String, ProviderError> {
        Ok(format!("deno-{}.zip", self.triple()?))
    }
}

impl RuntimeProvider for DenoProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/denoland/deno/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["deno"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let releases = ctx.http.json(RELEASES)?;
        let items = releases
            .as_array()
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "GitHub releases payload is not an array".to_string(),
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
        let wanted = self.asset_name()?;
        let detail = ctx.http.json(&format!(
            "https://api.github.com/repos/denoland/deno/releases/tags/{}",
            release.tag
        ))?;
        let assets = detail
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
                    .is_some_and(|name| name == wanted)
            })
            .ok_or_else(|| ProviderError::Unsupported(format!("no asset named {wanted}")))?;
        let url = asset
            .get("browser_download_url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "asset has no browser_download_url".to_string(),
            })?;
        let sha256 = asset
            .get("digest")
            .and_then(|value| value.as_str())
            .and_then(|value| value.strip_prefix("sha256:"))
            .map(|value| value.to_ascii_lowercase());
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
        let binary = format!("deno{}", self.platform.exe_suffix());
        Ok(Layout::new(".", self.entrypoints()).with_executables(&[&binary]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Libc;

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
    fn asset_names_match_deno_releases() {
        assert_eq!(
            DenoProvider::new(platform(Os::Linux, Arch::X64))
                .asset_name()
                .unwrap(),
            "deno-x86_64-unknown-linux-gnu.zip"
        );
        assert_eq!(
            DenoProvider::new(platform(Os::Windows, Arch::X64))
                .asset_name()
                .unwrap(),
            "deno-x86_64-pc-windows-msvc.zip"
        );
    }
}
