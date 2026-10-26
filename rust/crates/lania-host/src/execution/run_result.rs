//! 统一结果模型（`RunResult`）。
//!
//! 见 `docs/design/multi-runtime-bridge.zh-CN.md` §7.3：
//! 日志与结果是两件事，必须分开建模。
//!
//! ```text
//! ① 原始流（event.diag）        —— 逐行，保真，可能有噪声
//!         │  bridge 内解析
//!         ▼
//! ② 结构化诊断（Diagnostic）    —— 去重、带位置、带规则
//!         │  宿主聚合
//!         ▼
//! ③ 统一结果（RunResult）      —— 供渲染与 JSON 输出
//! ```
//!
//! 这里只实现 ② → ③ 的聚合；渲染器（`LintReportRenderer` / `BuildReportRenderer`）
//! 独立实现，共用 `Diagnostic` 但不共用布局。

use lania_node_bridge::{DiagCounts, Diagnostic, DiagnosticSeverity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Lint,
    Build,
    Dev,
    App,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Success,
    Failed,
    Partial,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub kind: RunKind,
    pub status: RunStatus,
    /// 去重 + 排序后的诊断。
    pub diagnostics: Vec<Diagnostic>,
    pub counts: DiagCounts,
    pub duration_ms: u64,
}

impl RunResult {
    /// 从 bridge 的 `diagnostics` 数组聚合。
    pub fn from_diagnostics(
        kind: RunKind,
        status: RunStatus,
        diagnostics: Vec<Diagnostic>,
        duration_ms: u64,
    ) -> Self {
        let diagnostics = lania_node_bridge::normalize_diagnostics(diagnostics);
        let counts = DiagCounts::from_diagnostics(&diagnostics);
        Self {
            kind,
            status,
            diagnostics,
            counts,
            duration_ms,
        }
    }

    /// 没有任何 error 诊断 = 成功。
    pub fn status_for(&self) -> RunStatus {
        match self.status {
            RunStatus::Success if self.counts.error > 0 => RunStatus::Failed,
            other => other,
        }
    }
}

/// 解析 bridge 返回的 `diagnostics` 数组。
///
/// 宽容处理：单条解析失败就跳过，不让一条脏数据炸掉整份报告。
pub fn parse_diagnostics(value: Option<&serde_json::Value>) -> Vec<Diagnostic> {
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| serde_json::from_value::<Diagnostic>(item.clone()).ok())
        .collect()
}

/// 解析 bridge 返回的 `counts` 对象（缺失时从诊断重算）。
pub fn parse_counts(value: Option<&serde_json::Value>, diagnostics: &[Diagnostic]) -> DiagCounts {
    match value {
        Some(value) => DiagCounts {
            error: value["error"].as_u64().unwrap_or_default() as usize,
            warning: value["warning"].as_u64().unwrap_or_default() as usize,
            info: value["info"].as_u64().unwrap_or_default() as usize,
            hint: value["hint"].as_u64().unwrap_or_default() as usize,
        },
        None => DiagCounts::from_diagnostics(diagnostics),
    }
}

/// 供渲染失败的兜底：把一段自由文本变成一个文件级 error 诊断。
pub fn diagnostic_from_text(tool: &str, text: &str) -> Diagnostic {
    Diagnostic {
        tool: tool.to_string(),
        id: None,
        severity: DiagnosticSeverity::Error,
        message: text.trim().to_string(),
        file: None,
        range: None,
        url: None,
        fix: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn diag(tool: &str, file: &str, line: u64, col: u64, id: &str) -> Diagnostic {
        Diagnostic {
            tool: tool.into(),
            id: Some(id.into()),
            severity: DiagnosticSeverity::Warning,
            message: "m".into(),
            file: Some(file.into()),
            range: Some(lania_node_bridge::DiagnosticRange {
                start: lania_node_bridge::DiagnosticPosition { line, col },
                end: None,
            }),
            url: None,
            fix: None,
        }
    }

    #[test]
    fn parses_diagnostics_from_bridge_payload() {
        let payload = json!([
            {
                "tool": "ruff",
                "id": "E501",
                "severity": "error",
                "message": "line too long",
                "file": "src/a.py",
                "range": { "start": { "line": 12, "col": 5 } },
                "fix": { "available": true, "kind": "safe" }
            }
        ]);
        let parsed = parse_diagnostics(Some(&payload));
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].tool, "ruff");
        assert_eq!(parsed[0].id.as_deref(), Some("E501"));
        assert_eq!(parsed[0].range.unwrap().start.line, 12);
    }

    #[test]
    fn skips_malformed_entries_instead_of_failing() {
        let payload = json!([
            { "tool": "ruff", "severity": "error", "message": "ok" },
            { "severity": "not-a-severity" },
            "garbage"
        ]);
        assert_eq!(parse_diagnostics(Some(&payload)).len(), 1);
    }

    #[test]
    fn derives_counts_when_absent() {
        let diagnostics = vec![
            diag("ruff", "a.py", 1, 1, "E1"),
            diag("ruff", "a.py", 2, 1, "E2"),
        ];
        let counts = parse_counts(None, &diagnostics);
        assert_eq!(counts.warning, 2);
        assert_eq!(counts.error, 0);
    }

    #[test]
    fn run_result_normalizes_and_counts() {
        let result = RunResult::from_diagnostics(
            RunKind::Lint,
            RunStatus::Success,
            vec![
                diag("ruff", "b.py", 2, 1, "W1"),
                diag("ruff", "a.py", 1, 1, "E1"),
            ],
            42,
        );
        // 排序：file 升序。
        assert_eq!(result.diagnostics[0].file.as_deref(), Some("a.py"));
        assert_eq!(result.counts.warning, 2);
    }

    #[test]
    fn run_result_downgrades_success_when_errors_exist() {
        let result = RunResult::from_diagnostics(
            RunKind::Lint,
            RunStatus::Success,
            vec![Diagnostic {
                severity: DiagnosticSeverity::Error,
                ..diag("ruff", "a.py", 1, 1, "E1")
            }],
            1,
        );
        assert_eq!(result.status_for(), RunStatus::Failed);
    }
}
