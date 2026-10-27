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
    if let Some((framework, evidence)) = detect_from_contents(cwd) {
        return FrameworkDetection::Detected {
            framework: framework.into(),
            reason: format!("detected {framework} in {evidence}"),
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
///
/// 1–4 是纯文件存在性判断；5–7 需要读文件内容（语言生态没有「框架标记文件」约定），
/// 由 `detect_from_contents` 处理。
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

/// 依赖声明探测（§4.5.3 第 2 步）：`go.mod` + gin / `pyproject.toml` + fastapi /
/// `Cargo.toml` + axum / `pubspec.yaml` + flutter。
///
/// 只在**依赖声明里出现框架名**时才算命中 —— 有 `go.mod` 不代表是 gin 项目，
/// 猜错框架会生成不符合约定的文件，代价高于「让用户显式选」。
fn detect_from_contents(cwd: &Path) -> Option<(&'static str, &'static str)> {
    const MANIFESTS: &[(&str, &str)] = &[
        ("go.mod", "gin"),
        ("pyproject.toml", "fastapi"),
        ("Cargo.toml", "axum"),
        ("pubspec.yaml", "flutter"),
    ];
    for (file, framework) in MANIFESTS {
        let Ok(contents) = std::fs::read_to_string(cwd.join(file)) else {
            continue;
        };
        if manifest_declares(&contents, framework) {
            return Some((framework, file));
        }
    }
    None
}

/// 依赖声明里是否出现了该框架（大小写不敏感；`cargo add axum` 会写成 `axum = "0.7"`）。
fn manifest_declares(contents: &str, framework: &str) -> bool {
    let haystack = contents.to_ascii_lowercase();
    match framework {
        // Go module path 带域名，避免 `.gin` 之类的误判。
        "gin" => haystack.contains("github.com/gin-gonic/gin"),
        // Flutter 是 `pubspec.yaml` 的 SDK 段（`flutter:` / `sdk: flutter`）。
        "flutter" => haystack.contains("sdk: flutter") || haystack.contains("flutter:"),
        // Python / Rust 依赖名即可唯一命中。
        other => haystack
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
            .any(|token| token == other),
    }
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

    fn detect_with(name: &str, file: &str, contents: &str) -> FrameworkDetection {
        let dir =
            std::env::temp_dir().join(format!("lania-detect-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join(file), contents).expect("write marker");
        let detection = detect_framework(&dir, None);
        let _ = std::fs::remove_dir_all(dir);
        detection
    }

    #[test]
    fn dependency_manifests_detect_the_b7_frameworks() {
        for (framework, file, contents) in [
            (
                "gin",
                "go.mod",
                "module example.com/project\n\nrequire github.com/gin-gonic/gin v1.10.0\n",
            ),
            (
                "fastapi",
                "pyproject.toml",
                "[project]\ndependencies = [\"fastapi>=0.110\", \"uvicorn\"]\n",
            ),
            ("axum", "Cargo.toml", "[dependencies]\naxum = \"0.7\"\n"),
            ("flutter", "pubspec.yaml", "environment:\n  sdk: flutter\n"),
        ] {
            let detection = detect_with(framework, file, contents);
            assert_eq!(
                detection.framework(),
                framework,
                "{file} 应判定为 {framework}"
            );
            assert!(detection.reason().contains(file), "{}", detection.reason());
        }
    }

    #[test]
    fn manifests_without_the_framework_stay_generic() {
        // 有 `go.mod` 但没有 gin：不猜框架（§4.5.3 关键决策）。
        let detection = detect_with(
            "plain-go",
            "go.mod",
            "module example.com/project\n\nrequire github.com/go-chi/chi/v5 v5.0.0\n",
        );
        assert!(detection.is_generic());
        let detection = detect_with(
            "plain-rust",
            "Cargo.toml",
            "[dependencies]\nserde = \"1\"\n",
        );
        assert!(detection.is_generic());
    }

    #[test]
    fn next_config_detects_nextjs() {
        let detection = detect_with("next-config", "next.config.mjs", "export default {};\n");
        assert_eq!(detection.framework(), "nextjs");
    }
}
