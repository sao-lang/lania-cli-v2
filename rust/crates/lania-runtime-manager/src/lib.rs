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

// ─────────────────────────────────────────────────────────────────────────
// 对外稳定面（冻结清单，见 `docs/design/self-managed-runtime-manager.zh-CN.md` §11.3）
//
// 这一组导出**承诺稳定**：GUI / IDE 插件 / CI 脚本可以直接嵌入。
// 改名、删除或改签名都属于破坏性变更，必须同步更新 §11.3 与 changeset。
// `tests::frozen_export_surface_is_intact` 盯住这份清单，删漏任何一项都会编译失败。
// ─────────────────────────────────────────────────────────────────────────

// 模型
pub use home::{RuntimeHome, HOME_ENV, META_FILE};
pub use language::Language;
pub use platform::{Arch, Libc, Os, Platform};
pub use version::{pick_best, VersionTag};

// 服务
pub use detect::{EnvService, InstalledVersion, ProjectVersion, ToolStatus};
pub use store::{Diagnostic, InstallEvent, InstallOutcome, RuntimeStore};

// 扩展点
pub use provider::{
    describe as describe_provider, provider_for, ProviderArtifact, ProviderContext, ProviderInfo,
    RuntimeProvider,
};

// 计划与执行
pub use plan::{build_plan, execute_plan, EnvAction, EnvPlan, PlanStep};

// ─────────────────────────────────────────────────────────────────────────
// 内部实现（**不承诺稳定**：可自由重构，改名/改签名不视为破坏性变更）
//
// 主要在 crate 内部与 `lania-plugins-command-env` 之间使用；外部消费者不应依赖。
// ─────────────────────────────────────────────────────────────────────────

pub use archive::{extract as extract_archive, verify_integrity, ArchiveFormat};
pub use checksum::{find_in_manifest, hex_digest_sha256, verify_file};
pub use detect::{extract_semver, which};
pub use error::{ArchiveError, NetworkError, PlatformError, RuntimeError, VerifyError};
pub use home::PATH_ENV;
pub use http::HttpClient;
pub use plan::validate_version;
pub use provider::{
    is_planned, platform_limitation, supported_language_ids, upstream_for, ProviderLayout,
    ProviderRelease,
};
pub use shims::{resolve_entry as resolve_shim_entry, shell_snippet, ShimHost};
pub use state::{InstalledMeta, InstalledRuntime, RuntimeState};
pub use store::DiagnosticLevel;

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
