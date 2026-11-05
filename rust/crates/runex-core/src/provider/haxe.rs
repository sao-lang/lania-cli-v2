//! Haxe Provider：Haxe Foundation 官方编译器发行包。
//!
//! - 版本清单：`api.github.com/repos/HaxeFoundation/haxe/releases`（只取纯数字 tag）
//! - 产物：`haxe-<ver>-linux64.tar.gz` / `-osx.tar.gz` / `-win64.zip`
//! - 目录：包内是 `haxe_<build>/`，摊平后 `haxe` 与 `std/` 位于根
//! - 环境：`HAXE_STD_PATH` 必须指向版本目录里的 `std/`

use super::github;
use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "haxe";
const REPO: &str = "HaxeFoundation/haxe";

pub struct HaxeProvider {
    platform: Platform,
}

impl HaxeProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn asset_names(&self, version: &str) -> Vec<String> {
        let suffix = match (self.platform.os, self.platform.arch) {
            (Os::Linux, Arch::X64) => "linux64.tar.gz",
            (Os::Linux, Arch::Arm64) => "linux-arm64.tar.gz",
            (Os::Macos, _) => "osx.tar.gz",
            (Os::Windows, _) => "win64.zip",
        };
        vec![format!("haxe-{version}-{suffix}")]
    }
}

impl RuntimeProvider for HaxeProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/HaxeFoundation/haxe/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["haxe", "haxelib"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        github::releases(REPO, ctx, github::plain_version)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let assets = github::assets(REPO, &release.tag, ctx)?;
        for name in self.asset_names(&release.version) {
            if github::find(&assets, &name).is_some() {
                return github::plan(&assets, &name, None, ctx);
            }
        }
        Err(ProviderError::Unsupported(format!(
            "no Haxe {} archive for {}/{}",
            release.version,
            self.platform.os.id(),
            self.platform.arch.id()
        )))
    }

    fn install_layout(
        &self,
        _extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        Ok(Layout::new(".", self.entrypoints())
            .with_env("HAXE_STD_PATH", "{root}/std")
            .with_executables(&["haxe", "haxelib"]))
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
    fn asset_names_cover_the_platform_matrix() {
        assert_eq!(
            HaxeProvider::new(platform(Os::Linux, Arch::X64)).asset_names("4.3.7"),
            vec!["haxe-4.3.7-linux64.tar.gz"]
        );
        assert_eq!(
            HaxeProvider::new(platform(Os::Windows, Arch::Arm64)).asset_names("4.3.7"),
            vec!["haxe-4.3.7-win64.zip"]
        );
    }
}
