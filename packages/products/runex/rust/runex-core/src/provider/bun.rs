//! Bun Provider：GitHub Releases 上按平台打包的 zip。
//!
//! - 版本清单：`api.github.com/repos/oven-sh/bun/releases?per_page=100`
//! - 产物：`bun-<os>-<arch>.zip`，解压后是 `bun-<os>-<arch>/bun(.exe)`
//! - 目录：把单文件提升到版本目录根上

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "bun";
const RELEASES: &str = "https://api.github.com/repos/oven-sh/bun/releases?per_page=100";

pub struct BunProvider {
    platform: Platform,
}

impl BunProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn os_slug(&self) -> &'static str {
        match self.platform.os {
            Os::Linux => "linux",
            Os::Macos => "darwin",
            Os::Windows => "windows",
        }
    }

    fn arch_slug(&self) -> &'static str {
        match self.platform.arch {
            Arch::X64 => "x64",
            Arch::Arm64 => "aarch64",
        }
    }

    fn asset_name(&self) -> String {
        format!("bun-{}-{}.zip", self.os_slug(), self.arch_slug())
    }
}

impl RuntimeProvider for BunProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/oven-sh/bun/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["bun", "bunx"]
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
                let prerelease = release
                    .get("prerelease")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
                    // 上游用 `bun-v1.2.3-canary.1` 这类 tag 标记每日构建。
                    || tag.contains("canary")
                    || tag.contains("beta")
                    || tag.contains("alpha");
                Some(Release {
                    version: tag
                        .trim_start_matches("bun-v")
                        .trim_start_matches('v')
                        .to_string(),
                    prerelease,
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
        let wanted = self.asset_name();
        let detail = ctx.http.json(&format!(
            "https://api.github.com/repos/oven-sh/bun/releases/tags/{}",
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
        // zip 里是 `bun-<os>-<arch>/bun`；安装器会把单文件提升到版本根目录。
        let binary = format!("bun{}", self.platform.exe_suffix());
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
    fn asset_names_match_bun_releases() {
        assert_eq!(
            BunProvider::new(platform(Os::Linux, Arch::X64)).asset_name(),
            "bun-linux-x64.zip"
        );
        assert_eq!(
            BunProvider::new(platform(Os::Macos, Arch::Arm64)).asset_name(),
            "bun-darwin-aarch64.zip"
        );
        assert_eq!(
            BunProvider::new(platform(Os::Windows, Arch::X64)).asset_name(),
            "bun-windows-x64.zip"
        );
    }
}
