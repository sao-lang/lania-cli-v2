//! D Provider：DMD 官方发行包（downloads.dlang.org）。
//!
//! - 版本清单：`/releases/2.x/` 的 HTML 版本索引（无需 GitHub 配额）
//! - 产物：`dmd.<ver>.linux.tar.xz` / `.osx.tar.xz`
//! - 目录：包内是 `dmd2/`，摊平后二进制在 `linux/bin64/`、`osx/bin/`
//! - Windows 只有 `.windows.7z`：当前归档层不处理 7z，因此明确拒绝并给出原因。

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Os, Platform};

pub const ID: &str = "d";
const INDEX: &str = "https://downloads.dlang.org/releases/2.x/";

pub struct DProvider {
    platform: Platform,
}

impl DProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn file_name(&self, version: &str) -> Result<String, ProviderError> {
        Ok(match self.platform.os {
            Os::Linux => format!("dmd.{version}.linux.tar.xz"),
            Os::Macos => format!("dmd.{version}.osx.tar.xz"),
            Os::Windows => {
                return Err(ProviderError::Unsupported(
                    "DMD ships Windows builds as .7z installers; lania only extracts tar/zip"
                        .to_string(),
                ))
            }
        })
    }
}

/// 从 `<li><a href="/releases/2.x/2.113.1/">2.113.1</a></li>` 里取版本号。
/// 不引入正则：索引页是固定模板，按 `href="` 前缀扫描即可。
fn parse_versions(html: &str) -> Vec<Release> {
    const PREFIX: &str = "href=\"/releases/2.x/";
    let mut versions = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(PREFIX) {
        rest = &rest[start + PREFIX.len()..];
        let Some(end) = rest.find('"') else { break };
        let candidate = &rest[..end];
        let version = candidate.trim_end_matches('/');
        if let Some(key) = numeric_key(version) {
            versions.push((
                key,
                Release {
                    version: version.to_string(),
                    prerelease: false,
                    tag: version.to_string(),
                },
            ));
        }
    }
    versions.sort_by(|left, right| right.0.cmp(&left.0));
    versions.dedup_by(|left, right| left.1.version == right.1.version);
    versions.into_iter().map(|(_, release)| release).collect()
}

fn numeric_key(value: &str) -> Option<Vec<u32>> {
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    parts.iter().map(|part| part.parse::<u32>().ok()).collect()
}

impl RuntimeProvider for DProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "downloads.dlang.org/releases/2.x"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["dmd", "dub"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let html = ctx.http.text(INDEX)?;
        Ok(parse_versions(&html))
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        _ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let file_name = self.file_name(&release.version)?;
        let url = format!("{INDEX}{}/{file_name}", release.version);
        Ok(ArtifactPlan {
            signature: Some(format!("{url}.sig")),
            url,
            sha256: None,
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
        // materialize 会把唯一的 `dmd2/` 摊平，因此这里返回的是相对版本目录的路径。
        let bin_dir = [
            "linux/bin64",
            "linux/bin32",
            "osx/bin",
            "windows/bin",
            "bin",
        ]
        .iter()
        .find(|candidate| root.join(candidate).is_dir())
        .copied()
        .unwrap_or(".");
        Ok(Layout::new(bin_dir, self.entrypoints()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arch, Libc};

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
    fn parses_release_index_in_newest_first_order() {
        let html = r#"<li><a href="/releases/2.x/2.111.0/">2.111.0</a></li>
<li><a href="/releases/2.x/2.113.1/">2.113.1</a></li>
<li><a href="/releases/2.x/2.113.0/">2.113.0</a></li>
<li><a href="/releases/2.x/">..</a></li>"#;
        let releases = parse_versions(html);
        assert_eq!(
            releases
                .iter()
                .map(|r| r.version.as_str())
                .collect::<Vec<_>>(),
            vec!["2.113.1", "2.113.0", "2.111.0"]
        );
    }

    #[test]
    fn artifact_names_match_dmd_releases() {
        assert_eq!(
            DProvider::new(platform(Os::Linux, Arch::X64))
                .file_name("2.113.1")
                .unwrap(),
            "dmd.2.113.1.linux.tar.xz"
        );
        assert_eq!(
            DProvider::new(platform(Os::Macos, Arch::Arm64))
                .file_name("2.113.1")
                .unwrap(),
            "dmd.2.113.1.osx.tar.xz"
        );
        assert!(DProvider::new(platform(Os::Windows, Arch::X64))
            .file_name("2.113.1")
            .is_err());
    }
}
