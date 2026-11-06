//! Gleam Provider：官方 GitHub Releases 预编译二进制。
//!
//! - 版本清单：`api.github.com/repos/gleam-lang/gleam/releases`
//! - 产物：`gleam-<tag>-<target>.tar.gz` / `.zip`，包内单文件 `gleam` / `gleam.exe`
//! - 校验：每个资产都有同名 `.sha256` sidecar
//! - 目录：顶层散文件，bin_dir 为 `.`

use super::github;
use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "gleam";
const REPO: &str = "gleam-lang/gleam";

pub struct GleamProvider {
    platform: Platform,
}

impl GleamProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn arch(&self) -> &'static str {
        match self.platform.arch {
            Arch::X64 => "x86_64",
            Arch::Arm64 => "aarch64",
        }
    }

    fn asset_name(&self, tag: &str) -> String {
        let target = match self.platform.os {
            Os::Linux => format!("{}-unknown-linux-musl.tar.gz", self.arch()),
            Os::Macos => format!("{}-apple-darwin.tar.gz", self.arch()),
            Os::Windows => format!("{}-pc-windows-msvc.zip", self.arch()),
        };
        format!("gleam-{tag}-{target}")
    }
}

impl RuntimeProvider for GleamProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/gleam-lang/gleam/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["gleam"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        github::releases(REPO, ctx, github::plain_version)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let file_name = self.asset_name(&release.tag);
        let checksum_name = format!("{file_name}.sha256");
        let assets = github::assets(REPO, &release.tag, ctx)?;
        github::plan(&assets, &file_name, Some(&checksum_name), ctx)
    }

    fn install_layout(
        &self,
        _extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        Ok(Layout::new(".", self.entrypoints()).with_executables(&["gleam"]))
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
    fn asset_names_follow_rust_target_triples() {
        let cases = [
            (
                platform(Os::Linux, Arch::X64),
                "gleam-v1.19.2-x86_64-unknown-linux-musl.tar.gz",
            ),
            (
                platform(Os::Macos, Arch::Arm64),
                "gleam-v1.19.2-aarch64-apple-darwin.tar.gz",
            ),
            (
                platform(Os::Windows, Arch::X64),
                "gleam-v1.19.2-x86_64-pc-windows-msvc.zip",
            ),
        ];
        for (platform, expected) in cases {
            assert_eq!(GleamProvider::new(platform).asset_name("v1.19.2"), expected);
        }
    }
}
