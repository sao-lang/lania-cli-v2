//! Go Provider：go.dev/dl 的官方发行包。
//!
//! - 版本清单：`https://go.dev/dl/?mode=json&include=all`
//! - 产物：`dl/go<ver>.<os>-<arch>.tar.gz`（Windows 用 `.zip`）
//! - 目录：单层 `go/`，入口在 `bin/`
//! - 环境：`GOROOT` 指向版本目录

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "go";
const DL: &str = "https://go.dev/dl/";
const RELEASES: &str = "https://go.dev/dl/?mode=json&include=all";

pub struct GoProvider {
    platform: Platform,
}

impl GoProvider {
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

    /// Go 在 arm64 上只有 `arm64`，在 x64 上是 `amd64`。
    fn arch_slug(&self) -> &'static str {
        match self.platform.arch {
            Arch::X64 => "amd64",
            Arch::Arm64 => "arm64",
        }
    }

    fn archive_kind(&self) -> &'static str {
        if self.platform.os == Os::Windows {
            "zip"
        } else {
            "tar.gz"
        }
    }

    fn file_name(&self, version: &str) -> String {
        format!(
            "go{version}.{}-{}.{}",
            self.os_slug(),
            self.arch_slug(),
            self.archive_kind()
        )
    }
}

impl RuntimeProvider for GoProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "go.dev/dl"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["go", "gofmt"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let payload = ctx.http.json(RELEASES)?;
        let entries = payload
            .as_array()
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "go.dev/dl json is not an array".to_string(),
            })?;
        // go.dev 的顺序是「新 → 旧」，但条目里同时包含 `1.23rc1` 这类预发布，直接沿用顺序。
        Ok(entries
            .iter()
            .filter_map(|entry| {
                let version = entry.get("version")?.as_str()?;
                let normalized = version.trim_start_matches("go").to_string();
                Some(Release {
                    prerelease: entry
                        .get("stable")
                        .and_then(|value| value.as_bool())
                        .is_some_and(|stable| !stable),
                    version: normalized,
                    tag: version.to_string(),
                })
            })
            .collect())
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        _ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let file_name = self.file_name(&release.version);
        Ok(ArtifactPlan {
            // go.dev 的下载地址与文件名一致，且不单独发布 checksum 清单。
            url: format!("{DL}{file_name}"),
            sha256: None,
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
        let bin_dir = if root.join("bin").is_dir() {
            std::path::PathBuf::from("bin")
        } else {
            std::path::PathBuf::from(".")
        };
        Ok(Layout::new(bin_dir, self.entrypoints()).with_env("GOROOT", "{root}"))
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
    fn file_names_follow_go_download_convention() {
        let cases = [
            (
                platform(Os::Linux, Arch::X64),
                "go1.22.5.linux-amd64.tar.gz",
            ),
            (
                platform(Os::Linux, Arch::Arm64),
                "go1.22.5.linux-arm64.tar.gz",
            ),
            (
                platform(Os::Macos, Arch::Arm64),
                "go1.22.5.darwin-arm64.tar.gz",
            ),
            (
                platform(Os::Windows, Arch::X64),
                "go1.22.5.windows-amd64.zip",
            ),
        ];
        for (platform, expected) in cases {
            assert_eq!(GoProvider::new(platform).file_name("1.22.5"), expected);
        }
    }

    #[test]
    fn goroot_points_at_the_version_directory() {
        let provider = GoProvider::new(platform(Os::Linux, Arch::X64));
        let layout = Layout::new("bin", provider.entrypoints()).with_env("GOROOT", "{root}");
        let resolved = layout.resolved_env(std::path::Path::new("/runex/go/1.22.5"));
        assert_eq!(resolved[0].0, "GOROOT");
        assert_eq!(resolved[0].1, "/runex/go/1.22.5");
    }
}
