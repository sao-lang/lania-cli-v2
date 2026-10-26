//! 框架探测（§4.5.3）。
//!
//! 优先级：
//! 1. `lan.config.*` 的 `add.framework`（显式声明）
//! 2. 特征文件推断（`nest-cli.json` → nestjs，`next.config.*` → nextjs，…）
//! 3. 回退到「通用生成器」（`framework = ""`）
//!
//! **关键决策：探测失败时不猜框架。** 猜错的代价（生成不符合约定的文件）
//! 高于「让用户显式选」的成本，因此只暴露 `component` / `config`。

use std::path::Path;

use serde_json::Value;

use super::generators::GENERIC_FRAMEWORK;

/// 探测器识别出的框架。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameworkDetection {
    /// 明确识别（或配置显式声明）的框架。
    Detected { framework: String, reason: String },
    /// 没有识别出框架，只能使用通用生成器。
    Generic,
}

impl FrameworkDetection {
    pub fn framework(&self) -> &str {
        match self {
            Self::Detected { framework, .. } => framework,
            Self::Generic => GENERIC_FRAMEWORK,
        }
    }

    /// 人类可读的探测依据（用于 `lan add --list` 输出）。
    pub fn reason(&self) -> &str {
        match self {
            Self::Detected { reason, .. } => reason,
            Self::Generic => "no framework marker",
        }
    }

    pub fn is_generic(&self) -> bool {
        matches!(self, Self::Generic)
    }
}

/// 按优先级执行探测。`config_raw` 是 `lan.config.*` 的原始 JSON。
pub fn detect_framework(cwd: &Path, config_raw: Option<&Value>) -> FrameworkDetection {
    if let Some(explicit) = config_framework(config_raw) {
        return FrameworkDetection::Detected {
            framework: explicit,
            reason: "lan.config add.framework".into(),
        };
    }
    if let Some((framework, evidence)) = detect_from_files(cwd) {
        return FrameworkDetection::Detected {
            framework: framework.into(),
            reason: format!("detected {evidence}"),
        };
    }
    FrameworkDetection::Generic
}

/// `add.framework` 既可以写在顶层，也可以放在 `add` 段。
fn config_framework(config_raw: Option<&Value>) -> Option<String> {
    let raw = config_raw?;
    let candidate = raw
        .get("add")
        .and_then(|add| add.get("framework"))
        .or_else(|| raw.get("addFramework"))
        .and_then(Value::as_str)?;
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 特征文件表：顺序即优先级（Nest 与 Next 同时存在时不应发生，但仍需确定性）。
fn detect_from_files(cwd: &Path) -> Option<(&'static str, &'static str)> {
    const RULES: &[(&str, &str)] = &[
        ("nest-cli.json", "nestjs"),
        ("next.config.js", "nextjs"),
        ("next.config.mjs", "nextjs"),
        ("next.config.ts", "nextjs"),
    ];
    for (file, framework) in RULES {
        if cwd.join(file).exists() {
            return Some((framework, file));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn config_declaration_wins() {
        let detection = detect_framework(
            Path::new("."),
            Some(&json!({ "add": { "framework": "nestjs" } })),
        );
        assert_eq!(detection.framework(), "nestjs");
        assert!(detection.reason().contains("lan.config"));
    }

    #[test]
    fn nest_cli_json_detects_nestjs() {
        let dir = std::env::temp_dir().join(format!("lania-detect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("nest-cli.json"), "{}").expect("write marker");
        let detection = detect_framework(&dir, None);
        assert_eq!(detection.framework(), "nestjs");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_project_falls_back_to_generic() {
        let dir = std::env::temp_dir().join(format!("lania-detect-generic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let detection = detect_framework(&dir, None);
        assert!(detection.is_generic());
        assert_eq!(detection.framework(), GENERIC_FRAMEWORK);
        let _ = std::fs::remove_dir_all(dir);
    }
}
