//! V Provider：vlang 官方预编译包。
//!
//! - 版本清单：`api.github.com/repos/vlang/v/releases`
//! - 产物：`v_<platform>.zip`，包内是 `v/` 源码树，二进制在 `v/`
//! - 目录：摊平后 `v` / `v.exe` 位于版本目录根，bin_dir 为 `.`
//! - 上游不发布校验和，安装时由归档完整性检查兜底。

use super::github;
use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "v";
const REPO: &str = "vlang/v";

pub struct VProvider {
    platform: Platform,
}

impl VProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn asset_name(&self) -> Result<&'static str, ProviderError> {
        Ok(match (self.platform.os, self.platform.arch) {
            (Os::Linux, Arch::X64) => "v_linux.zip",
            (Os::Linux, Arch::Arm64) => "v_linux_arm64.zip",
            (Os::Macos, Arch::X64) => "v_macos_x86_64.zip",
            (Os::Macos, Arch::Arm64) => "v_macos_arm64.zip",
            (Os::Windows, Arch::X64) => "v_windows.zip",
            (Os::Windows, Arch::Arm64) => {
                return Err(ProviderError::Unsupported(
                    "V has no Windows arm64 build".to_string(),
                ))
            }
        })
    }
}

impl RuntimeProvider for VProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/vlang/v/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["v"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        github::releases(REPO, ctx, github::plain_version)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let file_name = self.asset_name()?.to_string();
        let assets = github::assets(REPO, &release.tag, ctx)?;
        github::plan(&assets, &file_name, None, ctx)
    }

    fn install_layout(
        &self,
        _extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        // zip 产物在 POSIX 上会丢掉可执行位，安装后统一补 0755。
        Ok(Layout::new(".", self.entrypoints()).with_executables(&["v", "v.exe"]))
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
    fn asset_names_follow_v_release_convention() {
        let cases = [
            (platform(Os::Linux, Arch::X64), "v_linux.zip"),
            (platform(Os::Linux, Arch::Arm64), "v_linux_arm64.zip"),
            (platform(Os::Macos, Arch::Arm64), "v_macos_arm64.zip"),
            (platform(Os::Windows, Arch::X64), "v_windows.zip"),
        ];
        for (platform, expected) in cases {
            assert_eq!(VProvider::new(platform).asset_name().unwrap(), expected);
        }
    }
}
