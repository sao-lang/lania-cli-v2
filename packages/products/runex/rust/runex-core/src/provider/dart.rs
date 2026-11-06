//! Dart Provider：Google 官方 Dart SDK 归档。
//!
//! - 版本清单：`storage.googleapis.com/storage/v1/b/dart-archive` 稳定通道
//! - 产物：`dartsdk-<os>-<arch>-release.zip`
//! - 目录：zip 内是 `dart-sdk/`，入口在 `bin/`
//! - 环境：`DART_SDK`

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "dart";
const BUCKET: &str = "https://storage.googleapis.com/storage/v1/b/dart-archive/o";
const DOWNLOADS: &str = "https://storage.googleapis.com/dart-archive/channels/stable/release";

pub struct DartProvider {
    platform: Platform,
}

impl DartProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn sdk_platform(&self) -> Result<&'static str, ProviderError> {
        Ok(match (self.platform.os, self.platform.arch) {
            (Os::Windows, Arch::X64) => "windows-x64",
            (Os::Windows, Arch::Arm64) => "windows-arm64",
            (Os::Linux, Arch::X64) => "linux-x64",
            (Os::Linux, Arch::Arm64) => "linux-arm64",
            (Os::Macos, Arch::X64) => "macos-x64",
            (Os::Macos, Arch::Arm64) => "macos-arm64",
        })
    }

    fn asset_name(&self) -> Result<String, ProviderError> {
        Ok(format!("dartsdk-{}-release.zip", self.sdk_platform()?))
    }
}

impl RuntimeProvider for DartProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "storage.googleapis.com/dart-archive"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["dart"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let mut releases = Vec::new();
        let mut page_token: Option<String> = None;
        for _ in 0..10 {
            let mut url =
                format!("{BUCKET}?prefix=channels/stable/release/&delimiter=/&maxResults=1000");
            if let Some(token) = &page_token {
                url.push_str(&format!("&pageToken={token}"));
            }
            let payload = ctx.http.json(&url)?;
            if let Some(prefixes) = payload.get("prefixes").and_then(|value| value.as_array()) {
                for prefix in prefixes.iter().filter_map(|value| value.as_str()) {
                    let Some(version) = prefix
                        .strip_prefix("channels/stable/release/")
                        .and_then(|value| value.strip_suffix('/'))
                    else {
                        continue;
                    };
                    if version.contains('.')
                        && version.chars().next().is_some_and(|c| c.is_ascii_digit())
                    {
                        releases.push(Release {
                            version: version.to_string(),
                            prerelease: false,
                            tag: version.to_string(),
                        });
                    }
                }
            }
            page_token = payload
                .get("nextPageToken")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            if page_token.is_none() {
                break;
            }
        }
        releases.reverse();
        Ok(releases)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let file_name = self.asset_name()?;
        let base = format!("{DOWNLOADS}/{}/sdk", release.tag);
        let url = format!("{base}/{file_name}");
        let sha256 = ctx
            .http
            .text(&format!("{url}.sha256sum"))
            .ok()
            .and_then(|manifest| {
                crate::checksum::find_in_manifest(&manifest, &file_name)
                    .or_else(|| manifest.split_whitespace().next().map(ToOwned::to_owned))
            });
        Ok(ArtifactPlan {
            url,
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
        Ok(Layout::new("bin", self.entrypoints()).with_env("DART_SDK", "{root}"))
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
    fn sdk_names_follow_dart_archive_convention() {
        let windows = DartProvider::new(platform(Os::Windows, Arch::X64));
        assert_eq!(
            windows.asset_name().unwrap(),
            "dartsdk-windows-x64-release.zip"
        );
        let linux = DartProvider::new(platform(Os::Linux, Arch::Arm64));
        assert_eq!(
            linux.asset_name().unwrap(),
            "dartsdk-linux-arm64-release.zip"
        );
    }
}
