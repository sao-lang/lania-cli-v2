//! Julia Provider：julialang 官方二进制归档。
//!
//! - 版本清单：`julialang-s3.julialang.org/bin/versions.json`（含每个文件的 sha256）
//! - 产物：`julia-<ver>-<os>-<arch>.tar.gz` / `julia-<ver>-win64.zip`
//! - 目录：单层 `julia-<ver>/`，入口在 `bin/`
//! - 注意：macOS 的 `.dmg`、Windows 的 `.exe` 是安装器，这里只取归档产物。

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Libc, Os, Platform};

pub const ID: &str = "julia";
const VERSIONS: &str = "https://julialang-s3.julialang.org/bin/versions.json";

pub struct JuliaProvider {
    platform: Platform,
}

impl JuliaProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn os(&self) -> &'static str {
        match self.platform.os {
            Os::Windows => "winnt",
            Os::Macos => "mac",
            Os::Linux => "linux",
        }
    }

    fn arch(&self) -> &'static str {
        match self.platform.arch {
            Arch::X64 => "x86_64",
            Arch::Arm64 => "aarch64",
        }
    }

    /// Windows 优先 zip，其它平台只用 tar.gz（dmg / exe 是安装器，不能当归档解）。
    fn extensions(&self) -> &'static [&'static str] {
        if self.platform.os == Os::Windows {
            &["zip", "tar.gz"]
        } else {
            &["tar.gz"]
        }
    }

    fn payload(&self, ctx: &ResolveContext<'_>) -> Result<serde_json::Value, ProviderError> {
        Ok(ctx.http.json(VERSIONS)?)
    }
}

/// `1.13.3` → `[1, 13, 3]`，用于把上游按字典序排列的键还原成版本序。
fn version_key(version: &str) -> Option<Vec<u32>> {
    version
        .split('.')
        .map(|part| part.parse::<u32>().ok())
        .collect()
}

impl RuntimeProvider for JuliaProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "julialang-s3.julialang.org/bin"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["julia"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let payload = self.payload(ctx)?;
        let versions = payload
            .as_object()
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "Julia versions.json is not an object".to_string(),
            })?;
        let mut releases: Vec<(Vec<u32>, Release)> = versions
            .iter()
            .filter(|(_, entry)| {
                entry
                    .get("stable")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
            })
            .filter_map(|(version, _)| {
                Some((
                    version_key(version)?,
                    Release {
                        version: version.clone(),
                        prerelease: false,
                        tag: version.clone(),
                    },
                ))
            })
            .collect();
        releases.sort_by(|left, right| right.0.cmp(&left.0));
        Ok(releases.into_iter().map(|(_, release)| release).collect())
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let payload = self.payload(ctx)?;
        let entry = payload.get(release.version.as_str()).ok_or_else(|| {
            ProviderError::Unsupported(format!("Julia {} is not in versions.json", release.version))
        })?;
        let files = entry
            .get("files")
            .and_then(|value| value.as_array())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: format!("Julia {} has no files array", release.version),
            })?;
        let want_musl = self.platform.os == Os::Linux && self.platform.libc == Libc::Musl;
        for extension in self.extensions() {
            let file = files.iter().find(|file| {
                let os_ok = file.get("os").and_then(|v| v.as_str()) == Some(self.os());
                let arch_ok = file.get("arch").and_then(|v| v.as_str()) == Some(self.arch());
                let ext_ok = file.get("extension").and_then(|v| v.as_str()) == Some(*extension);
                let url = file.get("url").and_then(|v| v.as_str()).unwrap_or_default();
                // musl 产物与普通 linux 产物的 os/arch 完全相同，只能按文件名区分。
                let libc_ok = url.contains("-musl-") == want_musl;
                let hosted = url.starts_with("https://");
                os_ok && arch_ok && ext_ok && libc_ok && hosted
            });
            if let Some(file) = file {
                let url = file
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let file_name = url.rsplit('/').next().unwrap_or("julia.tar.gz").to_string();
                return Ok(ArtifactPlan {
                    url,
                    sha256: file
                        .get("sha256")
                        .and_then(|v| v.as_str())
                        .map(|value| value.to_ascii_lowercase()),
                    signature: None,
                    file_name,
                });
            }
        }
        Err(ProviderError::Unsupported(format!(
            "no Julia {} archive for {}/{}/{}",
            release.version,
            self.os(),
            self.arch(),
            if want_musl { "musl" } else { "default" }
        )))
    }

    fn install_layout(
        &self,
        _extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        Ok(Layout::new("bin", self.entrypoints()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_keys_sort_numerically() {
        assert_eq!(version_key("1.13.3"), Some(vec![1, 13, 3]));
        assert!(version_key("1.13.3-rc1").is_none());
        assert!(vec![1u32, 13, 3] > vec![1u32, 9, 4]);
    }

    #[test]
    fn installer_formats_are_never_selected() {
        let windows = JuliaProvider::new(Platform {
            os: Os::Windows,
            arch: Arch::X64,
            libc: Libc::None,
        });
        assert_eq!(windows.extensions(), &["zip", "tar.gz"]);
        let linux = JuliaProvider::new(Platform {
            os: Os::Linux,
            arch: Arch::Arm64,
            libc: Libc::Gnu,
        });
        assert_eq!(linux.extensions(), &["tar.gz"]);
        assert_eq!(linux.arch(), "aarch64");
    }
}
