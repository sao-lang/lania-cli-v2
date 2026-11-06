//! Java Provider：Eclipse Adoptium (Temurin) JDK。
//!
//! - 版本清单：`api.adoptium.net/v3/info/available_releases`
//! - 产物：`api.adoptium.net/v3/assets/latest/<major>/hotspot` 的 `package`
//! - 目录：单层 JDK 目录，入口在 `bin/`
//! - 环境：`JAVA_HOME`

use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};
use crate::{Arch, Os, Platform};

pub const ID: &str = "java";
const API: &str = "https://api.adoptium.net/v3";

pub struct JavaProvider {
    platform: Platform,
}

struct Latest {
    version: String,
    url: String,
    sha256: Option<String>,
    signature: Option<String>,
    file_name: String,
}

impl JavaProvider {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }

    fn os(&self) -> &'static str {
        match self.platform.os {
            Os::Windows => "windows",
            Os::Macos => "mac",
            Os::Linux => "linux",
        }
    }

    fn arch(&self) -> &'static str {
        match self.platform.arch {
            Arch::X64 => "x64",
            Arch::Arm64 => "aarch64",
        }
    }

    fn latest(&self, major: u32, ctx: &ResolveContext<'_>) -> Result<Latest, ProviderError> {
        let url = format!(
            "{API}/assets/latest/{major}/hotspot?architecture={}&image_type=jdk&os={}&vendor=eclipse",
            self.arch(),
            self.os()
        );
        let payload = ctx.http.json(&url)?;
        let item = payload
            .as_array()
            .and_then(|items| items.first())
            .ok_or_else(|| {
                ProviderError::Unsupported(format!("no Temurin JDK {major} for this platform"))
            })?;
        let package = item
            .get("binary")
            .and_then(|value| value.get("package"))
            .ok_or_else(|| ProviderError::Unexpected {
                detail: format!("Temurin JDK {major} payload has no binary.package"),
            })?;
        let version = item
            .get("version")
            .and_then(|value| value.get("semver"))
            .and_then(|value| value.as_str())
            .map(normalize_version)
            .ok_or_else(|| ProviderError::Unexpected {
                detail: format!("Temurin JDK {major} payload has no version.semver"),
            })?;
        let link = package
            .get("link")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: format!("Temurin JDK {major} package has no link"),
            })?;
        let file_name = package
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| link.rsplit('/').next().unwrap_or("jdk.zip"))
            .to_string();
        Ok(Latest {
            version,
            url: link.to_string(),
            sha256: package
                .get("checksum")
                .and_then(|value| value.as_str())
                .map(|value| value.to_ascii_lowercase()),
            signature: package
                .get("signature_link")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            file_name,
        })
    }
}

fn normalize_version(semver: &str) -> String {
    semver
        .split('+')
        .next()
        .unwrap_or(semver)
        .trim()
        .to_string()
}

impl RuntimeProvider for JavaProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "api.adoptium.net (Temurin)"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["java", "javac", "jar", "jlink"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        let payload = ctx.http.json(&format!("{API}/info/available_releases"))?;
        let majors = payload
            .get("available_releases")
            .and_then(|value| value.as_array())
            .ok_or_else(|| ProviderError::Unexpected {
                detail: "Adoptium available_releases is not an array".to_string(),
            })?;
        let mut releases = Vec::new();
        for major in majors.iter().filter_map(|value| value.as_u64()) {
            if let Ok(latest) = self.latest(major as u32, ctx) {
                releases.push(Release {
                    version: latest.version,
                    prerelease: false,
                    tag: major.to_string(),
                });
            }
        }
        Ok(releases)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let major = release
            .version
            .split('.')
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| {
                ProviderError::Unsupported(format!("invalid Java version {}", release.version))
            })?;
        let latest = self.latest(major, ctx)?;
        Ok(ArtifactPlan {
            url: latest.url,
            sha256: latest.sha256,
            signature: latest.signature,
            file_name: latest.file_name,
        })
    }

    fn install_layout(
        &self,
        _extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        Ok(Layout::new("bin", self.entrypoints())
            .with_env("JAVA_HOME", "{root}")
            .with_env("JDK_HOME", "{root}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_adoptium_semver() {
        assert_eq!(normalize_version("21.0.13+9.0.LTS"), "21.0.13");
        assert_eq!(normalize_version("17.0.9+9"), "17.0.9");
    }
}
