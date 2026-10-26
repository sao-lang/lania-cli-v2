//! 多语言工具链版本管理的核心模型。
//!
//! 设计要点：
//! - 本 crate 负责「探测本机运行时」与「自管运行时」（安装 / 切换 / 卸载 / 列举）。
//!   自管路径的目录布局见 `~/.runex`（[`RuntimeHome`]），不依赖任何外部版本管理器。
//! - 自管实现的契约是 [`RuntimeProvider`]：每种语言只需回答「有哪些版本 / 下哪个文件 /
//!   怎么摆目录 / 需要什么环境变量」，下载、校验、解压、登记、shim 由服务层统一完成。
//! - 所有能力都是「声明式描述 + 探测」，这样 GUI 与 CLI 可以共用同一份数据。
//!
//! 主要导出：Language、EnvService、RuntimeStore、RuntimeProvider。

mod archive;
mod checksum;
mod detect;
mod error;
mod home;
mod http;
mod language;
mod plan;
mod platform;
pub mod provider;
mod shims;
mod state;
mod store;
mod version;

#[cfg(test)]
mod tests;

pub use archive::{extract as extract_archive, verify_integrity, ArchiveFormat};
pub use checksum::{find_in_manifest, hex_digest_sha256, verify_file};
pub use detect::{extract_semver, which, EnvService, InstalledVersion, ProjectVersion, ToolStatus};
pub use error::{ArchiveError, NetworkError, PlatformError, RuntimeError, VerifyError};
pub use home::{RuntimeHome, HOME_ENV, META_FILE, PATH_ENV};
pub use http::HttpClient;
pub use language::Language;
pub use plan::{build_plan, execute_plan, validate_version, EnvAction, EnvPlan, PlanStep};
pub use platform::{Arch, Libc, Os, Platform};
pub use provider::{
    describe as describe_provider, is_planned, platform_limitation, provider_for,
    supported_language_ids, upstream_for, ProviderArtifact, ProviderContext, ProviderInfo,
    ProviderLayout, ProviderRelease, RuntimeProvider,
};
pub use shims::{resolve_entry as resolve_shim_entry, shell_snippet};
pub use state::{InstalledMeta, InstalledRuntime, RuntimeState};
pub use store::{Diagnostic, DiagnosticLevel, InstallEvent, InstallOutcome, RuntimeStore};
pub use version::{pick_best, VersionTag};

fn extract_version(language: Language, file_name: &str, content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    match (language, file_name) {
        (_, ".nvmrc") | (_, ".node-version") | (_, ".python-version") | (_, ".fvmrc") => {
            Some(trimmed.lines().next()?.trim().to_string())
        }
        (Language::Rust, "rust-toolchain.toml") => content
            .lines()
            .find_map(|line| line.trim().strip_prefix("channel").map(str::trim))
            .map(|value| value.trim_matches(|c| c == '"' || c == '\'').to_string())
            .filter(|value| !value.is_empty()),
        (Language::Rust, "rust-toolchain") => Some(trimmed.lines().next()?.trim().to_string()),
        (Language::Go, "go.mod") => content
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("go ")
                    .map(|value| value.trim().to_string())
            })
            .filter(|value| !value.is_empty()),
        (_, ".sdkmanrc") => content.lines().find_map(|line| {
            if line.trim_start().starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim().to_ascii_lowercase();
            if key.contains("kotlin") || key.contains("java") {
                Some(value.trim().to_string())
            } else {
                None
            }
        }),
        (Language::Dart, "pubspec.yaml") => content.lines().find_map(|line| {
            line.trim().strip_prefix("sdk:").map(|value| {
                value
                    .trim_matches(|c| c == '"' || c == '\'')
                    .trim()
                    .to_string()
            })
        }),
        _ => None,
    }
}
