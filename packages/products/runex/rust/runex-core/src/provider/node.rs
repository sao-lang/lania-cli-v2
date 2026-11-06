//! Node.js Provider：nodejs.org/dist 的 tar.gz / zip 产物。
//!
//! - 版本清单：`https://nodejs.org/dist/index.json`
//! - 产物：`dist/v<ver>/node-v<ver>-<os>-<arch>{.tar.gz|.zip}`
//! - 目录：单层 `node-v<ver>-<os>-<arch>/`，内含 `bin/`（Windows 直接在根上）
//! - 入口：node / npm / npx / corepack

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "node";
const DIST: &str = "https://nodejs.org/dist/";
const MIRROR_ENV: &str = "RUNEX_NODE_MIRROR";

/// Node 分发根地址；`RUNEX_NODE_MIRROR` 可覆盖（例如国内镜像）。
fn dist() -> String {
    match std::env::var(MIRROR_ENV) {
        Ok(value) if !value.trim().is_empty() => {
            format!("{}/", value.trim().trim_end_matches('/'))
        }
        _ => DIST.to_string(),
    }
}

pub struct NodeProvider {
    platform: Platform,
}

impl NodeProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn artifact_name(&self, version: &str) -> Result<String, ProviderError> {
        let os = match self.platform.os {
            Os::Linux => "linux",
            Os::Macos => "darwin",
            Os::Windows => "win",
        };
        let arch = match self.platform.arch {
            Arch::X64 => "x64",
            Arch::Arm64 => "arm64",
        };
        // Node 的 Windows 产物只有 zip，其余平台是 tar.gz。
        let extension = if self.platform.os == Os::Windows {
            "zip"
        } else {
            "tar.gz"
        };
        Ok(format!("node-v{version}-{os}-{arch}.{extension}"))
    }
}

impl RuntimeProvider for NodeProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "nodejs.org/dist"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["node", "npm", "npx", "corepack"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let index = ctx
            .http
            .json(&format!("{}index.json", dist()))
            .map_err(|error| ProviderError::Upstream(error.to_string()))?;
        let items = index.as_array().ok_or_else(|| ProviderError::Unexpected {
            detail: "dist/index.json is not an array".to_string(),
        })?;
        Ok(items
            .iter()
            .filter_map(|item| {
                let version = item.get("version")?.as_str()?;
                // `lts` 为 false 表示当前的最新线；`security` 等字段与本方案无关。
                let normalized = version.trim_start_matches('v').to_string();
                Some(Release {
                    version: normalized,
                    prerelease: item
                        .get("version")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| value.contains('-')),
                    tag: version.to_string(),
                })
            })
            .collect())
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let file_name = self.artifact_name(&release.version)?;
        let base = format!("{}{}/", dist(), release.tag);
        let url = format!("{base}{file_name}");
        // Node 为每个版本提供 SHASUMS256.txt（GNU 风格清单）。
        let sha256 = match ctx.http.text(&format!("{base}SHASUMS256.txt")) {
            Ok(manifest) => Some(
                crate::checksum::find_in_manifest(&manifest, &file_name).ok_or_else(|| {
                    ProviderError::Unexpected {
                        detail: format!("SHASUMS256.txt has no entry for {file_name}"),
                    }
                })?,
            ),
            Err(error) => return Err(ProviderError::Upstream(error.to_string())),
        };
        Ok(ArtifactPlan {
            url,
            sha256,
            signature: Some(format!("{base}SHASUMS256.txt.asc")),
            file_name,
        })
    }

    fn install_layout(
        &self,
        extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        // 解压后是单层目录 `node-v<ver>-<os>-<arch>/`。
        let root =
            crate::archive::single_root(extracted).map_err(|error| ProviderError::Layout {
                detail: error.to_string(),
            })?;
        let bin_dir = if root.join("bin").is_dir() {
            std::path::PathBuf::from("bin")
        } else {
            std::path::PathBuf::from(".")
        };
        let _ = self.platform;
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
    fn artifact_names_follow_nodejs_convention() {
        let cases = [
            (
                platform(Os::Linux, Arch::X64),
                "node-v20.11.1-linux-x64.tar.gz",
            ),
            (
                platform(Os::Macos, Arch::Arm64),
                "node-v20.11.1-darwin-arm64.tar.gz",
            ),
            (
                platform(Os::Windows, Arch::X64),
                "node-v20.11.1-win-x64.zip",
            ),
        ];
        for (platform, expected) in cases {
            assert_eq!(
                NodeProvider::new(platform)
                    .artifact_name("20.11.1")
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn layout_prefers_bin_directory() {
        let root = std::env::temp_dir().join(format!("lania-node-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let inner = root.join("node-v20.11.1-linux-x64");
        std::fs::create_dir_all(inner.join("bin")).unwrap();
        let provider = NodeProvider::new(platform(Os::Linux, Arch::X64));
        let layout = provider
            .install_layout(&root, std::path::Path::new("/target"))
            .unwrap();
        assert_eq!(layout.bin_dir, std::path::PathBuf::from("bin"));
        assert_eq!(layout.entrypoints, vec!["node", "npm", "npx", "corepack"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
