//! 多语言工具链版本管理的核心模型。
//!
//! 设计要点：
//! - 本 crate 只负责“探测 + 计划（plan）”，不直接执行安装/卸载命令；
//!   真正执行交给调用方（`lan env` 命令插件经由 `lania-exec`）。
//! - 版本管理器优先复用生态既有方案（nvm / fnm / volta / pyenv / uv / rustup /
//!   go 自带 / dart / sdkman / scoop / winget / brew …），避免自造安装器。
//! - 所有能力都是“声明式描述 + 探测”，这样 GUI 与 CLI 可以共用同一份数据。
//!
//! 主要导出：Language、ManagerKind、EnvAction、EnvPlan、EnvService。

mod detect;
mod language;
pub mod managers;
mod plan;

#[cfg(test)]
mod tests;

pub use detect::{extract_semver, EnvService, InstalledVersion, ProjectVersion, ToolStatus};
pub use language::Language;
pub use managers::{install_location_hint, probe as probe_manager, ManagerKind, ManagerProbe};
pub use plan::{build_plan, pick_manager, EnvAction, EnvPlan, PlanStep};

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
