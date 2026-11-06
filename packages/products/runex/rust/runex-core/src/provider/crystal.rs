//! Crystal Provider：官方 GitHub Releases 预编译包。
//!
//! - 版本清单：`api.github.com/repos/crystal-lang/crystal/releases`
//! - 产物：`crystal-<ver>-<n>-linux-<arch>[-bundled].tar.gz` / `darwin-universal.tar.gz`
//! - 目录：单层 `crystal-<ver>-<n>/`，入口在 `bin/`
//! - 优先 `bundled` 包（自带依赖库），避免要求系统装 LLVM/SSL 开发库。
//! - Windows 产物被上游显式标记为 `unsupported`，因此不提供自管安装。

use super::github;
use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "crystal";
const REPO: &str = "crystal-lang/crystal";

pub struct CrystalProvider {
    platform: Platform,
}

impl CrystalProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    /// 返回按优先级排列的「资产名特征」；第一个命中的就是本平台产物。
    fn asset_markers(&self, version: &str) -> Result<Vec<String>, ProviderError> {
        let prefix = format!("crystal-{version}-");
        Ok(match (self.platform.os, self.platform.arch) {
            (Os::Linux, Arch::X64) => vec![
                format!("{prefix}.*linux-x86_64-bundled.tar.gz"),
                format!("{prefix}.*linux-x86_64.tar.gz"),
            ],
            (Os::Linux, Arch::Arm64) => vec![
                format!("{prefix}.*linux-aarch64-bundled.tar.gz"),
                format!("{prefix}.*linux-aarch64.tar.gz"),
            ],
            (Os::Macos, _) => vec![format!("{prefix}.*darwin-universal.tar.gz")],
            (Os::Windows, _) => {
                return Err(ProviderError::Unsupported(
                    "Crystal Windows builds are marked unsupported upstream".to_string(),
                ))
            }
        })
    }
}

/// 把 `crystal-1.22.2-.*linux-x86_64-bundled.tar.gz` 这类带 `.*` 的模式
/// 当成「前缀 + 后缀」匹配，避免依赖正则或硬编码构建号 `-1`。
fn matches_marker(name: &str, marker: &str) -> bool {
    let Some((prefix, suffix)) = marker.split_once(".*") else {
        return name == marker;
    };
    name.starts_with(prefix) && name.ends_with(suffix)
}

impl RuntimeProvider for CrystalProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/crystal-lang/crystal/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["crystal", "shards"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        github::releases(REPO, ctx, github::plain_version)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let markers = self.asset_markers(&release.version)?;
        let assets = github::assets(REPO, &release.tag, ctx)?;
        for marker in &markers {
            if let Some(asset) = github::find_where(&assets, |name| {
                matches_marker(name, marker) && !name.contains("unsupported")
            }) {
                return Ok(ArtifactPlan {
                    url: asset.url.clone(),
                    sha256: None,
                    signature: None,
                    file_name: asset.name.clone(),
                });
            }
        }
        Err(ProviderError::Unsupported(format!(
            "no Crystal {} archive for this platform",
            release.version
        )))
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
            "bin"
        } else {
            "."
        };
        Ok(Layout::new(bin_dir, self.entrypoints()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_handles_build_numbers_and_bundled_variant() {
        let marker = "crystal-1.22.2-.*linux-x86_64-bundled.tar.gz";
        assert!(matches_marker(
            "crystal-1.22.2-1-linux-x86_64-bundled.tar.gz",
            marker
        ));
        assert!(!matches_marker(
            "crystal-1.22.2-1-linux-aarch64-bundled.tar.gz",
            marker
        ));
        assert!(!matches_marker(
            "crystal-1.22.2-1-linux-x86_64.tar.gz",
            marker
        ));
    }
}
