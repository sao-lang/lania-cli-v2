//! Zig Provider：ziglang.org 的 tar.xz 产物。
//!
//! - 版本清单：`https://ziglang.org/download/index.json`
//! - 产物：`download/<ver>/zig-<os>-<arch>-<ver>.tar.xz`
//! - 目录：单层 `zig-<os>-<arch>-<ver>/`，内含 `zig` 可执行文件
//! - 校验：`index.json` 里每个条目自带 `shasum`

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "zig";
const INDEX: &str = "https://ziglang.org/download/index.json";

pub struct ZigProvider {
    platform: Platform,
}

impl ZigProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn os_slug(&self) -> &'static str {
        match self.platform.os {
            Os::Linux => "linux",
            Os::Macos => "macos",
            Os::Windows => "windows",
        }
    }

    fn arch_slug(&self) -> &'static str {
        match self.platform.arch {
            Arch::X64 => "x86_64",
            Arch::Arm64 => "aarch64",
        }
    }

    fn file_name(&self, version: &str) -> String {
        format!(
            "zig-{}-{}-{version}.tar.xz",
            self.os_slug(),
            self.arch_slug()
        )
    }

    /// `index.json` 里的平台键：`<arch>-<os>`（`x86_64-linux` / `aarch64-macos`）。
    fn platform_key(&self) -> String {
        format!("{}-{}", self.arch_slug(), self.os_slug())
    }
}

impl RuntimeProvider for ZigProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "ziglang.org/download"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["zig"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let index = ctx.http.json(INDEX)?;
        let object = index.as_object().ok_or_else(|| ProviderError::Unexpected {
            detail: "zig index.json is not an object".to_string(),
        })?;
        let platform_key = self.platform_key();
        let mut releases = Vec::new();
        for (key, value) in object {
            // 顶层键是版本号，另有 `master` 表示 nightly。
            // 只保留「本平台确实有产物」的版本，避免列出装不了的版本。
            if value.get(&platform_key).is_none() {
                continue;
            }
            releases.push(Release {
                version: key.clone(),
                prerelease: key == "master",
                tag: key.clone(),
            });
        }
        // index.json 的对象顺序不保证；按版本号降序，`master` 放最前（它是最新的）。
        releases.sort_by(|a, b| {
            let key = |release: &Release| {
                if release.version == "master" {
                    (u64::MAX, Vec::new())
                } else {
                    let parts = crate::VersionTag::parse(&release.version)
                        .map(|tag| tag.parts)
                        .unwrap_or_default();
                    (0u64, parts)
                }
            };
            key(b).cmp(&key(a))
        });
        Ok(releases)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let index = ctx.http.json(INDEX)?;
        let entry = index.get(&release.version).ok_or_else(|| {
            ProviderError::Unsupported(format!("version {} missing", release.version))
        })?;
        // index.json 的平台键是 `<arch>-<os>`，产物文件名是 `zig-<os>-<arch>-<ver>`。
        let platform_key = self.platform_key();
        let artifact = entry
            .get(&platform_key)
            .ok_or_else(|| ProviderError::Unsupported(format!("no {platform_key} artifact")))?;
        let url = artifact
            .get("tarball")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: format!("{} has no tarball for {platform_key}", release.version),
            })?;
        let sha256 = artifact
            .get("shasum")
            .and_then(|value| value.as_str())
            .map(|value| value.to_ascii_lowercase());
        // 上游给出的 tarball 地址就是权威命名；优先用它，避免自己拼错。
        let file_name = url
            .rsplit('/')
            .find(|segment| !segment.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.file_name(&release.version));
        Ok(ArtifactPlan {
            url: url.to_string(),
            sha256,
            signature: None,
            file_name,
        })
    }

    fn install_layout(
        &self,
        _extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        // 单层目录里直接就是 `zig` 可执行文件与 `lib/`，装到版本目录根上即可。
        Ok(Layout::new(".", self.entrypoints()))
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
    fn file_names_match_zig_downloads() {
        assert_eq!(
            ZigProvider::new(platform(Os::Linux, Arch::X64)).file_name("0.13.0"),
            "zig-linux-x86_64-0.13.0.tar.xz"
        );
        assert_eq!(
            ZigProvider::new(platform(Os::Macos, Arch::Arm64)).file_name("0.13.0"),
            "zig-macos-aarch64-0.13.0.tar.xz"
        );
        assert_eq!(
            ZigProvider::new(platform(Os::Windows, Arch::X64)).file_name("0.13.0"),
            "zig-windows-x86_64-0.13.0.tar.xz"
        );
    }

    #[test]
    fn platform_keys_match_index_json() {
        // index.json 用 `<arch>-<os>`，与产物文件名的顺序相反。
        assert_eq!(
            ZigProvider::new(platform(Os::Linux, Arch::X64)).platform_key(),
            "x86_64-linux"
        );
        assert_eq!(
            ZigProvider::new(platform(Os::Macos, Arch::Arm64)).platform_key(),
            "aarch64-macos"
        );
        assert_eq!(
            ZigProvider::new(platform(Os::Windows, Arch::X64)).platform_key(),
            "x86_64-windows"
        );
    }
}
