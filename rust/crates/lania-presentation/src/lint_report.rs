//! lint 结果的统一渲染（§7.4）。
//!
//! 为什么需要一个**独立渲染器**而不是复用 `LogRenderer`：
//! `LogRenderer` 的契约是 `render_entry(&LogEntry) -> String`，一次一行字符串，
//! 无法表达「按文件分组 + 对齐成列 + 组间空行」。硬塞进去只会让两边都变糟。
//!
//! 与之配套的 `BuildReportRenderer` 复用同一份 `Diagnostic`，但布局完全不同 ——
//! **共用诊断模型，不共用布局**，这正是分层带来的价值。

use crate::{StyledText, StyledTextRenderer, TerminalTextRenderer, TextColor, TextStyle};

/// 诊断严重程度（与 `lania_node_bridge::DiagnosticSeverity` 同构）。
///
/// 这里刻意不复用 bridge 的类型：`lania-presentation` 是最底层展示库，
/// 让它依赖 `lania-node-bridge` 会把进程管理/协议一起拖进来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// 渲染器看到的一条诊断（bridge `Diagnostic` 的展示子集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticView {
    pub tool: String,
    pub id: Option<String>,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u64>,
    pub col: Option<u64>,
    /// `Some(true)` 表示工具提供了自动修复。
    pub fix_available: Option<bool>,
}

impl DiagnosticView {
    fn identity(&self) -> (String, String, u64, u64, String) {
        (
            self.tool.clone(),
            self.file.clone().unwrap_or_default(),
            self.line.unwrap_or_default(),
            self.col.unwrap_or_default(),
            self.id.clone().unwrap_or_default(),
        )
    }
}

/// 按工具汇总的一行（summary 的第二行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCount {
    pub tool: String,
    pub count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LintReport {
    pub diagnostics: Vec<DiagnosticView>,
    /// 每个工具的诊断数（渲染为 `ruff 2 · mypy 1`）。
    pub tools: Vec<ToolCount>,
    pub duration_ms: Option<u64>,
}

impl LintReport {
    pub fn from_diagnostics(diagnostics: Vec<DiagnosticView>, duration_ms: Option<u64>) -> Self {
        let mut merged = normalize(diagnostics);
        // 同一位置的多个工具诊断**全部保留**，只在渲染层并排展示。
        let mut tools: Vec<ToolCount> = Vec::new();
        for diagnostic in &merged {
            match tools.iter_mut().find(|entry| entry.tool == diagnostic.tool) {
                Some(entry) => entry.count += 1,
                None => tools.push(ToolCount {
                    tool: diagnostic.tool.clone(),
                    count: 1,
                }),
            }
        }
        tools.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then(left.tool.cmp(&right.tool))
        });
        merged.shrink_to_fit();
        Self {
            diagnostics: merged,
            tools,
            duration_ms,
        }
    }

    pub fn counts(&self) -> (usize, usize) {
        let errors = self
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
            .count();
        (errors, self.diagnostics.len() - errors)
    }

    pub fn files(&self) -> usize {
        let mut files: Vec<&str> = self
            .diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.file.as_deref())
            .collect();
        files.sort_unstable();
        files.dedup();
        files.len()
    }
}

/// 去重 + 排序：`file` → `line` → `col` → `severity`（error 优先）→ `id`。
fn normalize(diagnostics: Vec<DiagnosticView>) -> Vec<DiagnosticView> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<DiagnosticView> = diagnostics
        .into_iter()
        .filter(|diagnostic| seen.insert(diagnostic.identity()))
        .collect();
    out.sort_by(|left, right| {
        left.file
            .as_deref()
            .unwrap_or("")
            .cmp(right.file.as_deref().unwrap_or(""))
            .then(left.line.cmp(&right.line))
            .then(left.col.cmp(&right.col))
            .then(left.severity.cmp(&right.severity))
            .then(left.id.cmp(&right.id))
            .then(left.tool.cmp(&right.tool))
    });
    out
}

/// 把 lint 结果渲染成「按文件分组 + 列对齐 + severity 高亮」的文本行。
#[derive(Debug, Clone)]
pub struct LintReportRenderer {
    renderer: TerminalTextRenderer,
    locale: String,
}

impl LintReportRenderer {
    pub fn new(renderer: TerminalTextRenderer, locale: impl Into<String>) -> Self {
        Self {
            renderer,
            locale: locale.into(),
        }
    }

    /// 颜色只在 TTY 时启用，且遵守 `NO_COLOR`（`TerminalTextRenderer` 已处理）。
    pub fn for_terminal(is_terminal: bool, locale: impl Into<String>) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        Self::new(TerminalTextRenderer::new(is_terminal && !no_color), locale)
    }

    /// 渲染成多行文本（每行已含/不含 ANSI，取决于 renderer 配置）。
    pub fn render(&self, report: &LintReport) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current_file: Option<&str> = None;

        for diagnostic in &report.diagnostics {
            let file = diagnostic.file.as_deref().unwrap_or("<project>");
            if current_file != Some(file) {
                if current_file.is_some() {
                    lines.push(String::new());
                }
                lines.push(
                    self.renderer
                        .render(&StyledText::styled(file, TextStyle::default().bold())),
                );
                current_file = Some(file);
            }
            lines.push(self.renderer.render(&self.render_diagnostic(diagnostic)));
        }

        if !report.diagnostics.is_empty() {
            lines.push(String::new());
        }
        lines.push(self.renderer.render(&self.render_summary(report)));
        if !report.tools.is_empty() {
            lines.push(self.renderer.render(&self.render_tool_breakdown(report)));
        }
        lines
    }

    fn render_diagnostic(&self, diagnostic: &DiagnosticView) -> StyledText {
        let mut text = StyledText::plain("  ");
        // severity 图标 + 名称：error 红色加粗，warning 黄色，info/hint 蓝色或暗淡。
        let (icon, label, style) = match diagnostic.severity {
            DiagnosticSeverity::Error => (
                "✖",
                self.pick("error", "错误"),
                TextStyle::colored(TextColor::Red).bold(),
            ),
            DiagnosticSeverity::Warning => (
                "⚠",
                self.pick("warning", "警告"),
                TextStyle::colored(TextColor::Yellow),
            ),
            DiagnosticSeverity::Info => (
                "ℹ",
                self.pick("info", "信息"),
                TextStyle::colored(TextColor::Blue),
            ),
            DiagnosticSeverity::Hint => {
                ("·", self.pick("hint", "提示"), TextStyle::default().dim())
            }
        };
        text.push(format!("{icon} "), style.clone());
        // 行列号用 dim，避免抢走消息的注意力。
        let position = match (diagnostic.line, diagnostic.col) {
            (Some(line), Some(col)) => format!("{line}:{col}"),
            (Some(line), None) => format!("{line}"),
            _ => "-".to_string(),
        };
        text.push(format!("{position:<8}"), TextStyle::default().dim());
        text.push(format!("{label:<8}"), style);
        // 规则号青色；缺失时补空格占位以保持列对齐。
        match &diagnostic.id {
            Some(id) => text.push(format!("{id:<10}"), TextStyle::colored(TextColor::Cyan)),
            None => text.push(format!("{:<10}", ""), TextStyle::default()),
        }
        text.push(diagnostic.message.clone(), TextStyle::default());
        // 工具名洋红 + dim，`[fix]` 绿色。
        text.push(
            format!("  {}", diagnostic.tool),
            TextStyle::colored(TextColor::Magenta).dim(),
        );
        if diagnostic.fix_available == Some(true) {
            text.push(" [fix]", TextStyle::colored(TextColor::Green));
        }
        text
    }

    fn render_summary(&self, report: &LintReport) -> StyledText {
        let (errors, rest) = report.counts();
        let warnings = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
            .count();
        let files = report.files();
        let mut text = StyledText::styled(
            format!("{}: ", self.pick("Summary", "汇总")),
            TextStyle::default().bold(),
        );
        text.push(
            format!("{errors} {}", self.pick("errors", "个错误")),
            if errors > 0 {
                TextStyle::colored(TextColor::Red).bold()
            } else {
                TextStyle::colored(TextColor::Green)
            },
        );
        text.push(
            format!(", {warnings} {}", self.pick("warnings", "个警告")),
            if warnings > 0 {
                TextStyle::colored(TextColor::Yellow)
            } else {
                TextStyle::default().dim()
            },
        );
        if rest > warnings {
            text.push(
                format!(", {} {}", rest - warnings, self.pick("other", "其他")),
                TextStyle::default().dim(),
            );
        }
        text.push(
            format!(
                " {} {files} {}",
                self.pick("in", "涉及"),
                self.pick("files", "个文件")
            ),
            TextStyle::default(),
        );
        if let Some(duration) = report.duration_ms {
            text.push(
                format!(" ({:.1}s)", duration as f64 / 1000.0),
                TextStyle::default().dim(),
            );
        }
        text
    }

    fn render_tool_breakdown(&self, report: &LintReport) -> StyledText {
        let mut text = StyledText::plain("  ");
        for (index, entry) in report.tools.iter().enumerate() {
            if index > 0 {
                text.push(" · ", TextStyle::default().dim());
            }
            text.push(
                entry.tool.clone(),
                TextStyle::colored(TextColor::Magenta).dim(),
            );
            text.push(format!(" {}", entry.count), TextStyle::default().dim());
        }
        text
    }

    fn pick<'a>(&self, en: &'a str, zh: &'a str) -> &'a str {
        if self.locale == "zh" {
            zh
        } else {
            en
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(
        tool: &str,
        file: &str,
        line: u64,
        col: u64,
        id: &str,
        severity: DiagnosticSeverity,
    ) -> DiagnosticView {
        DiagnosticView {
            tool: tool.into(),
            id: Some(id.into()),
            severity,
            message: format!("{id} message"),
            file: Some(file.into()),
            line: Some(line),
            col: Some(col),
            fix_available: None,
        }
    }

    #[test]
    fn groups_by_file_and_sorts_within_group() {
        let report = LintReport::from_diagnostics(
            vec![
                view("ruff", "src/b.py", 1, 1, "W1", DiagnosticSeverity::Warning),
                view("ruff", "src/a.py", 20, 1, "W2", DiagnosticSeverity::Warning),
                view("ruff", "src/a.py", 12, 5, "E1", DiagnosticSeverity::Error),
            ],
            Some(2100),
        );
        let lines = LintReportRenderer::for_terminal(false, "en").render(&report);
        let plain = lines.join("\n");

        assert!(plain.contains("src/a.py"), "{plain}");
        assert!(plain.contains("src/b.py"), "{plain}");
        // 文件内按行号升序。
        let a_start = plain.find("src/a.py").unwrap();
        let b_start = plain.find("src/b.py").unwrap();
        assert!(a_start < b_start, "files must be sorted: {plain}");
        let line12 = plain.find("12:5").unwrap();
        let line20 = plain.find("20:1").unwrap();
        assert!(line12 < line20, "lines must ascend: {plain}");
    }

    #[test]
    fn aligns_columns_across_diagnostics() {
        let report = LintReport::from_diagnostics(
            vec![
                view("ruff", "a.py", 1, 5, "E501", DiagnosticSeverity::Error),
                view("mypy", "a.py", 100, 10, "E0602", DiagnosticSeverity::Error),
            ],
            None,
        );
        let lines = LintReportRenderer::for_terminal(false, "en").render(&report);
        let rows: Vec<&String> = lines
            .iter()
            .filter(|line| line.contains("message"))
            .collect();
        assert_eq!(rows.len(), 2, "{lines:?}");
        // 列对齐：两条诊断的「消息起始列」必须相同。
        let msg_cols: Vec<usize> = rows
            .iter()
            .map(|row| row.find("E501").or_else(|| row.find("E0602")).unwrap())
            .collect();
        assert_eq!(msg_cols[0], msg_cols[1], "rule column must align: {rows:?}");
    }

    #[test]
    fn renders_summary_and_tool_breakdown() {
        let report = LintReport::from_diagnostics(
            vec![
                view("ruff", "a.py", 1, 1, "E1", DiagnosticSeverity::Error),
                view("ruff", "b.py", 1, 1, "E2", DiagnosticSeverity::Error),
                view("mypy", "c.py", 1, 1, "E3", DiagnosticSeverity::Warning),
            ],
            Some(1500),
        );
        let plain = LintReportRenderer::for_terminal(false, "en")
            .render(&report)
            .join("\n");

        assert!(plain.contains("Summary:"), "{plain}");
        assert!(plain.contains("2 errors"), "{plain}");
        assert!(plain.contains("1 warning"), "{plain}");
        assert!(plain.contains("3 files"), "{plain}");
        assert!(plain.contains("1.5s"), "{plain}");
        assert!(plain.contains("ruff 2"), "{plain}");
        assert!(plain.contains("mypy 1"), "{plain}");
    }

    #[test]
    fn localizes_summary() {
        let report = LintReport::from_diagnostics(
            vec![view("ruff", "a.py", 1, 1, "E1", DiagnosticSeverity::Error)],
            None,
        );
        let plain = LintReportRenderer::for_terminal(false, "zh")
            .render(&report)
            .join("\n");
        assert!(plain.contains("汇总"), "{plain}");
        assert!(plain.contains("个错误"), "{plain}");
    }

    #[test]
    fn no_ansi_when_terminal_colors_disabled() {
        let report = LintReport::from_diagnostics(
            vec![view("ruff", "a.py", 1, 1, "E1", DiagnosticSeverity::Error)],
            None,
        );
        let plain = LintReportRenderer::for_terminal(false, "en")
            .render(&report)
            .join("\n");
        // NO_COLOR / 非 TTY 时不得出现任何 ANSI 转义序列。
        assert!(
            !plain.contains('\u{1b}'),
            "unexpected ANSI escape: {plain:?}"
        );
    }

    #[test]
    fn ansi_is_emitted_when_terminal_colors_enabled() {
        let report = LintReport::from_diagnostics(
            vec![view("ruff", "a.py", 1, 1, "E1", DiagnosticSeverity::Error)],
            None,
        );
        let rendered = LintReportRenderer::new(TerminalTextRenderer::new(true), "en")
            .render(&report)
            .join("\n");
        assert!(rendered.contains('\u{1b}'), "expected ANSI escapes");
        // 但纯文本内容必须仍然可读。
        assert!(rendered.contains("E1"));
    }

    #[test]
    fn marks_fixable_diagnostics() {
        let mut diagnostic = view("ruff", "a.py", 1, 1, "E1", DiagnosticSeverity::Error);
        diagnostic.fix_available = Some(true);
        let report = LintReport::from_diagnostics(vec![diagnostic], None);
        let plain = LintReportRenderer::for_terminal(false, "en")
            .render(&report)
            .join("\n");
        assert!(plain.contains("[fix]"), "{plain}");
    }

    #[test]
    fn keeps_multiple_tools_at_the_same_position() {
        let report = LintReport::from_diagnostics(
            vec![
                view("ruff", "a.py", 3, 1, "E1", DiagnosticSeverity::Error),
                view("mypy", "a.py", 3, 1, "E2", DiagnosticSeverity::Error),
            ],
            None,
        );
        // 同一位置不同工具都必须保留（去重键包含 tool）。
        assert_eq!(report.diagnostics.len(), 2);
    }

    #[test]
    fn deduplicates_identical_diagnostics() {
        let report = LintReport::from_diagnostics(
            vec![
                view("ruff", "a.py", 3, 1, "E1", DiagnosticSeverity::Error),
                view("ruff", "a.py", 3, 1, "E1", DiagnosticSeverity::Error),
            ],
            None,
        );
        assert_eq!(report.diagnostics.len(), 1);
    }

    #[test]
    fn handles_file_level_diagnostics_without_position() {
        let report = LintReport::from_diagnostics(
            vec![DiagnosticView {
                tool: "clippy".into(),
                id: None,
                severity: DiagnosticSeverity::Warning,
                message: "crate-level warning".into(),
                file: None,
                line: None,
                col: None,
                fix_available: None,
            }],
            None,
        );
        let plain = LintReportRenderer::for_terminal(false, "en")
            .render(&report)
            .join("\n");
        assert!(plain.contains("<project>"), "{plain}");
        assert!(plain.contains("crate-level warning"), "{plain}");
    }

    #[test]
    fn empty_report_still_renders_summary() {
        let report = LintReport::from_diagnostics(vec![], None);
        let plain = LintReportRenderer::for_terminal(false, "en")
            .render(&report)
            .join("\n");
        assert!(plain.contains("0 errors"), "{plain}");
    }
}
