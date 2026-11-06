//! Rust Provider：static.rust-lang.org 的独立 toolchain 产物。
//!
//! 注意这里**不使用 rustup**：lania 直接下载官方发行包并自行拼接 toolchain，
//! 因此 `CARGO_HOME` 指向版本目录内的 `cargo`，全局包（`cargo install`）随版本变化。
//!
//! - 版本清单：`https://static.rust-lang.org/dist/channel-rust-stable.toml`（另有 beta/nightly）
//! - 产物：`dist/<date>/cargo-<ver>-<triple>.tar.gz` 等组件包
//! - 目录：把各组件的 `*.tar.gz` 解到临时目录，再摊平成 `bin/`、`lib/`、`share/`

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "rust";
const DIST: &str = "https://static.rust-lang.org/dist";

pub struct RustProvider {
    platform: Platform,
}

impl RustProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn target_triple(&self) -> &'static str {
        match (self.platform.os, self.platform.arch) {
            (Os::Linux, Arch::X64) => "x86_64-unknown-linux-gnu",
            (Os::Linux, Arch::Arm64) => "aarch64-unknown-linux-gnu",
            (Os::Macos, Arch::X64) => "x86_64-apple-darwin",
            (Os::Macos, Arch::Arm64) => "aarch64-apple-darwin",
            // rust-lang 为 Windows 只发布 msvc 变体。
            (Os::Windows, _) => "x86_64-pc-windows-msvc",
        }
    }

    /// 该平台需要的组件包（`rust` 是编译器本体，`cargo` 是包管理器）。
    fn components(&self) -> [&'static str; 2] {
        ["rust", "cargo"]
    }

    fn component_urls(&self, release: &Release) -> Result<Vec<(String, String)>, ProviderError> {
        let triple = self.target_triple();
        let date = release.tag.as_str();
        let mut out = Vec::new();
        for component in self.components() {
            let file_name = format!("{component}-{}-{triple}.tar.gz", release.version);
            let url = format!("{DIST}/{date}/{file_name}");
            out.push((url, file_name));
        }
        Ok(out)
    }
}

impl RuntimeProvider for RustProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "static.rust-lang.org"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["rustc", "cargo", "rustdoc"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        // 稳定版清单是 TOML。真实格式里**没有**根层 `version`：
        // 版本号在 `[pkg.rust]` / `[pkg.cargo]` 段内，日期在根层 `date`。
        // 因此只做最小的分段解析，不引入 toml 依赖。
        let manifest = ctx.http.text(&format!("{DIST}/channel-rust-stable.toml"))?;
        let mut date = None;
        let mut section = String::new();
        let mut rust_version = None;
        let mut cargo_version = None;
        for raw in manifest.lines() {
            let line = raw.trim();
            if line.starts_with('[') {
                section = line.trim_matches(&['[', ']'][..]).to_string();
                continue;
            }
            let Some(value) = line.strip_prefix("version = ") else {
                continue;
            };
            let value = value.trim_matches('"').to_string();
            match section.as_str() {
                "pkg.rust" if rust_version.is_none() => rust_version = Some(value),
                "pkg.cargo" if cargo_version.is_none() => cargo_version = Some(value),
                _ => {}
            }
        }
        // 日期单独扫一遍（`date` 在 `[pkg.*]` 之前）。
        if date.is_none() {
            date = manifest.lines().find_map(|line| {
                let line = line.trim();
                let value = line.strip_prefix("date = ")?.trim_matches('"');
                (value.len() == 10 && value.as_bytes()[4] == b'-').then(|| value.to_string())
            });
        }

        // `pkg.rust.version` 形如 `1.99.1 (xxxx 2026-08-27)`，只保留语义版本号；
        // `pkg.cargo.version` 是 cargo 自己的版本（`0.100.0`），两者不同步，
        // 工具链版本必须以 `pkg.rust` 为准。
        let version = rust_version
            .or(cargo_version)
            .and_then(|value| {
                value
                    .split_whitespace()
                    .next()
                    .map(|token| token.to_string())
            })
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "channel-rust-stable.toml has no [pkg.rust] version".to_string(),
            })?;
        let date = date.ok_or_else(|| ProviderError::Unexpected {
            detail: "channel-rust-stable.toml has no date".to_string(),
        })?;
        if crate::VersionTag::parse(&version)
            .map(|tag| tag.parts.len() < 2)
            .unwrap_or(true)
        {
            return Err(ProviderError::Unexpected {
                detail: format!("unrecognized rust version `{version}`"),
            });
        }
        Ok(vec![Release {
            version,
            prerelease: false,
            // Rust 的下载路径用发布日期而不是版本号。
            tag: date,
        }])
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        _ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        // Rust 的「产物」是一组组件包，这里返回编译器包作为代表，
        // 真正的下载列表由 [`RuntimeProvider::extra_artifacts`] 提供。
        let (url, file_name) = self
            .component_urls(release)?
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::Unsupported("no components".to_string()))?;
        Ok(ArtifactPlan {
            url,
            sha256: None,
            signature: None,
            file_name,
        })
    }

    fn extra_artifacts(
        &self,
        release: &Release,
        _ctx: &ResolveContext<'_>,
    ) -> Result<Vec<ArtifactPlan>, ProviderError> {
        Ok(self
            .component_urls(release)?
            .into_iter()
            .skip(1)
            .map(|(url, file_name)| ArtifactPlan {
                url,
                sha256: None,
                signature: None,
                file_name,
            })
            .collect())
    }

    fn install_layout(
        &self,
        extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        let mut layout = Layout::new("bin", self.entrypoints());
        for dir in crate::archive::top_level_dirs(extracted) {
            let name = dir
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default();
            layout.notes.push(name);
        }
        Ok(layout)
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
    fn component_urls_use_release_date() {
        let provider = RustProvider::new(platform(Os::Linux, Arch::X64));
        let release = Release {
            version: "1.86.0".into(),
            prerelease: false,
            tag: "2025-04-03".into(),
        };
        let urls = provider.component_urls(&release).unwrap();
        assert_eq!(
            urls[0].0,
            "https://static.rust-lang.org/dist/2025-04-03/rust-1.86.0-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(urls[1].1, "cargo-1.86.0-x86_64-unknown-linux-gnu.tar.gz");
    }

    #[test]
    fn windows_uses_msvc_triple() {
        let provider = RustProvider::new(platform(Os::Windows, Arch::X64));
        assert_eq!(provider.target_triple(), "x86_64-pc-windows-msvc");
    }
}
