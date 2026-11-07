//! 终端渲染：把 [`AgentEvent`] 变成人看的输出。
//!
//! 这一层是**唯一**写 stdout/stderr 的地方（§3.1：`print!` 属于命令层）。
//! 因此它可以在测试里被替换成 buffer，事件流转本身不依赖终端。

use std::io::{self, Write};

use lania_agent_core::{AgentEvent, ApprovalMode, SessionInfo, Usage};
use serde_json::Value;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const MAGENTA: &str = "\x1b[35m";

/// 输出模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// 人读：流式文本 + 工具卡片 + 状态行。
    Human,
    /// 机器读：一行一个 JSON 事件（`--json`）。
    Json,
}

/// 事件渲染器。
pub struct Renderer {
    mode: RenderMode,
    color: bool,
    unicode: bool,
    /// 本轮是否已经输出过正文（用于决定换行）。
    wrote_text: bool,
    /// 本轮是否输出过 thinking（用于折叠成一行提示）。
    wrote_thinking: bool,
    out: Box<dyn Write + Send>,
}

impl Renderer {
    pub fn new(mode: RenderMode, color: bool) -> Self {
        Self {
            mode,
            color,
            unicode: color,
            wrote_text: false,
            wrote_thinking: false,
            out: Box::new(io::stdout()),
        }
    }

    /// 供测试注入缓冲区（生产路径固定写 stdout）。
    #[cfg(test)]
    pub fn with_writer(mode: RenderMode, color: bool, out: Box<dyn Write + Send>) -> Self {
        Self {
            mode,
            color,
            unicode: color,
            wrote_text: false,
            wrote_thinking: false,
            out,
        }
    }

    /// 当前是否机器可读模式（调用方据此决定要不要额外输出）。
    pub fn is_json(&self) -> bool {
        self.mode == RenderMode::Json
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("{code}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    fn dim(&self, text: &str) -> String {
        self.paint(DIM, text)
    }

    /// 选字形：彩色终端（UAX#11 可用）用 Unicode 符号，否则退回 ASCII。
    fn glyph(&self, unicode: &'static str, ascii: &'static str) -> &'static str {
        if self.unicode {
            unicode
        } else {
            ascii
        }
    }

    fn write_line(&mut self, line: &str) {
        let _ = writeln!(self.out, "{line}");
        let _ = self.out.flush();
    }

    /// 会话开头的一行摘要。
    pub fn banner(&mut self, info: &SessionInfo, approval: ApprovalMode, resumed: bool) {
        if self.mode == RenderMode::Json {
            return;
        }
        let status = if resumed { "resumed" } else { "new" };
        let tools = if info.tools.is_empty() {
            "no tools".to_string()
        } else {
            info.tools.join(", ")
        };
        self.write_line(&format!(
            "{} {} {} {}",
            self.paint(MAGENTA, "lan pi"),
            self.dim(&format!("· {status} · {approval}")),
            self.dim("·"),
            self.paint(CYAN, &info.model),
        ));
        self.write_line(&format!("  {}", self.dim(&tools)));
    }

    /// 处理一个事件。
    pub fn handle(&mut self, event: &AgentEvent) {
        if self.mode == RenderMode::Json {
            if let Ok(line) = serde_json::to_string(&event_to_json(event)) {
                self.write_line(&line);
            }
            return;
        }

        match event {
            // 模型信息由 `banner` 打印；事件里重复出现时保持静默，避免每轮刷屏。
            AgentEvent::Start { .. } => {}
            AgentEvent::TextDelta { delta, .. } => {
                if delta.is_empty() {
                    return;
                }
                self.ensure_text_gap();
                let _ = write!(self.out, "{delta}");
                let _ = self.out.flush();
                self.wrote_text = true;
            }
            AgentEvent::ThinkingDelta { delta, .. } => {
                if delta.is_empty() || self.wrote_thinking {
                    return;
                }
                // 只提示「思考过」，不把推理内容倒进终端。
                self.ensure_text_gap();
                let hint = self.dim(&format!("{} thinking…", self.glyph("✻", "~")));
                let _ = write!(self.out, "{hint}");
                let _ = self.out.flush();
                self.wrote_thinking = true;
            }
            AgentEvent::ToolStart { tool, params, .. } => {
                self.newline_if_needed();
                let summary = describe_tool(tool, params);
                self.write_line(&format!(
                    "{} {} {}",
                    self.paint(CYAN, self.glyph("⏺", ">")),
                    self.paint(CYAN, tool),
                    self.dim(&summary),
                ));
                self.wrote_text = false;
                self.wrote_thinking = false;
            }
            AgentEvent::ToolUpdate { partial, .. } => {
                if let Some(note) = partial.get("note").and_then(Value::as_str) {
                    self.write_line(&format!("  {}", self.dim(note)));
                }
            }
            AgentEvent::ToolEnd {
                tool,
                ok,
                duration_ms,
                summary,
                ..
            } => {
                let mark = if *ok {
                    self.paint(GREEN, self.glyph("✔", "+"))
                } else {
                    self.paint(RED, self.glyph("✘", "x"))
                };
                let mut detail = summary.clone().unwrap_or_default();
                if let Some(ms) = duration_ms {
                    if !detail.is_empty() {
                        detail.push_str(" · ");
                    }
                    detail.push_str(&format!("{ms}ms"));
                }
                self.write_line(&format!("  {mark} {tool} {}", self.dim(&detail)));
            }
            AgentEvent::TurnEnd { usage, .. } | AgentEvent::End { usage, .. } => {
                if usage.is_empty() {
                    return;
                }
                let label = usage.compact_label();
                if matches!(event, AgentEvent::End { .. }) {
                    self.newline_if_needed();
                    self.write_line(&format!("  {}", self.dim(&label)));
                }
            }
            AgentEvent::Error { code, message, .. } => {
                self.newline_if_needed();
                self.write_line(&format!(
                    "{} {}",
                    self.paint(RED, "error"),
                    if code.is_empty() {
                        message.clone()
                    } else {
                        format!("[{code}] {message}")
                    }
                ));
                self.wrote_text = false;
            }
            AgentEvent::Other { .. } => {}
        }
    }

    /// 一轮结束后的状态行（token、上下文占用）。
    pub fn status(&mut self, usage: Usage, context_tokens: u64, context_window: Option<u64>) {
        if self.mode == RenderMode::Json {
            return;
        }
        self.newline_if_needed();
        let mut parts = vec![usage.compact_label()];
        if let Some(window) = context_window.filter(|window| *window > 0) {
            let percent = (context_tokens as f64 / window as f64 * 100.0).round() as u64;
            parts.push(format!("context {percent}%"));
        } else if context_tokens > 0 {
            parts.push(format!("context {context_tokens} tok"));
        }
        self.write_line(&format!("  {}", self.dim(&parts.join(" · "))));
    }

    /// 提示信息（不属于模型输出）。
    pub fn notice(&mut self, text: &str) {
        if self.mode == RenderMode::Json {
            return;
        }
        self.write_line(&format!("{} {}", self.paint(YELLOW, "·"), self.dim(text)));
    }

    pub fn warning(&mut self, text: &str) {
        if self.mode == RenderMode::Json {
            return;
        }
        self.write_line(&format!("{} {text}", self.paint(YELLOW, "warning:")));
    }

    pub fn error(&mut self, text: &str) {
        if self.mode == RenderMode::Json {
            let _ = writeln!(self.out, "{}", serde_json::json!({ "error": text }));
            let _ = self.out.flush();
            return;
        }
        self.write_line(&format!("{} {text}", self.paint(RED, "error:")));
    }

    fn newline_if_needed(&mut self) {
        if self.wrote_text || self.wrote_thinking {
            self.write_line("");
        }
        self.wrote_text = false;
        self.wrote_thinking = false;
    }

    /// thinking 提示后面紧接正文时补一个换行，避免文字粘在提示行尾。
    fn ensure_text_gap(&mut self) {
        if self.wrote_thinking {
            self.write_line("");
            self.wrote_thinking = false;
        }
    }
}

/// 行式模式的 UI：把运行时消息翻译成终端输出。
///
/// 与 TUI 的 `ChannelUi` 是同一接口的两个实现——运行时不关心谁在画。
impl crate::runtime::UiSink for Renderer {
    fn event(&mut self, event: AgentEvent) {
        self.handle(&event);
    }

    fn notice(&mut self, level: crate::runtime::NoticeLevel, text: &str) {
        match level {
            crate::runtime::NoticeLevel::Info => Renderer::notice(self, text),
            crate::runtime::NoticeLevel::Warn => Renderer::warning(self, text),
        }
    }

    fn turn_finished(&mut self, usage: Usage, context_tokens: u64, context_window: Option<u64>) {
        self.status(usage, context_tokens, context_window);
    }
}

/// 把工具调用参数压成一行给人看的摘要。
pub fn describe_tool(tool: &str, params: &Value) -> String {
    let pick = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    };
    let text = match tool {
        "read" | "write" | "edit" => {
            let path = pick("path").unwrap_or_else(|| "?".into());
            match (
                params.get("start").and_then(Value::as_u64),
                params.get("end").and_then(Value::as_u64),
            ) {
                (Some(start), Some(end)) => format!("{path}:{start}-{end}"),
                (Some(start), None) => format!("{path}:{start}-"),
                _ => path,
            }
        }
        "glob" => pick("pattern").unwrap_or_else(|| "?".into()),
        "grep" => {
            let pattern = pick("pattern").unwrap_or_else(|| "?".into());
            match pick("path") {
                Some(path) => format!("{pattern} ({path})"),
                None => pattern,
            }
        }
        "exec" => pick("command").unwrap_or_else(|| "?".into()),
        "git_status" => "status".into(),
        "git_diff" => pick("path").unwrap_or_else(|| "all".into()),
        _ => {
            if params.is_null() || params.as_object().map(|o| o.is_empty()).unwrap_or(false) {
                String::new()
            } else {
                let text = params.to_string();
                lania_agent_core::tokens::truncate_chars(&text, 120)
            }
        }
    };
    lania_agent_core::tokens::truncate_chars(&text, 160)
}

fn event_to_json(event: &AgentEvent) -> Value {
    match event {
        AgentEvent::Start { session_id, model } => {
            serde_json::json!({ "type": "start", "sessionId": session_id, "model": model })
        }
        AgentEvent::TextDelta { delta, .. } => {
            serde_json::json!({ "type": "text", "delta": delta })
        }
        AgentEvent::ThinkingDelta { delta, .. } => {
            serde_json::json!({ "type": "thinking", "delta": delta })
        }
        AgentEvent::ToolStart { tool, params, .. } => {
            serde_json::json!({ "type": "tool_start", "tool": tool, "params": params })
        }
        AgentEvent::ToolUpdate { partial, .. } => {
            serde_json::json!({ "type": "tool_update", "partial": partial })
        }
        AgentEvent::ToolEnd {
            tool,
            ok,
            duration_ms,
            summary,
            ..
        } => serde_json::json!({
            "type": "tool_end",
            "tool": tool,
            "ok": ok,
            "durationMs": duration_ms,
            "summary": summary,
        }),
        AgentEvent::TurnEnd { usage, .. } => serde_json::json!({
            "type": "turn_end",
            "inputTokens": usage.input_tokens,
            "outputTokens": usage.output_tokens,
        }),
        AgentEvent::End {
            usage, stop_reason, ..
        } => serde_json::json!({
            "type": "end",
            "usage": {
                "inputTokens": usage.input_tokens,
                "outputTokens": usage.output_tokens,
                "totalTokens": usage.total_tokens,
            },
            "stopReason": stop_reason,
        }),
        AgentEvent::Error { code, message, .. } => {
            serde_json::json!({ "type": "error", "code": code, "message": message })
        }
        AgentEvent::Other { method, params } => {
            serde_json::json!({ "type": "other", "method": method, "params": params })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Buffer {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn renderer() -> (Renderer, Buffer) {
        let buffer = Buffer::default();
        let renderer = Renderer::with_writer(RenderMode::Human, false, Box::new(buffer.clone()));
        (renderer, buffer)
    }

    #[test]
    fn text_deltas_are_streamed_without_newlines() {
        let (mut renderer, buffer) = renderer();
        for delta in ["he", "llo"] {
            renderer.handle(&AgentEvent::TextDelta {
                session_id: "s".into(),
                message_id: "m".into(),
                delta: delta.into(),
            });
        }
        assert_eq!(buffer.text(), "hello");
    }

    #[test]
    fn tool_card_uses_ascii_when_color_disabled() {
        let (mut renderer, buffer) = renderer();
        renderer.handle(&AgentEvent::ToolStart {
            session_id: "s".into(),
            tool_call_id: "c".into(),
            tool: "read".into(),
            params: json!({ "path": "src/a.ts", "start": 10, "end": 20 }),
        });
        renderer.handle(&AgentEvent::ToolEnd {
            session_id: "s".into(),
            tool_call_id: "c".into(),
            tool: "read".into(),
            ok: true,
            duration_ms: Some(12),
            summary: None,
        });
        let text = buffer.text();
        assert!(text.contains("> read src/a.ts:10-20"), "{text}");
        assert!(text.contains("+ read 12ms"), "{text}");
    }

    #[test]
    fn thinking_is_collapsed_into_one_hint() {
        let (mut renderer, buffer) = renderer();
        for _ in 0..3 {
            renderer.handle(&AgentEvent::ThinkingDelta {
                session_id: "s".into(),
                message_id: "m".into(),
                delta: "hmm".into(),
            });
        }
        let text = buffer.text();
        assert_eq!(text.matches("thinking").count(), 1, "{text}");
        // 推理正文不应出现在终端。
        assert!(!text.contains("hmm"), "{text}");
    }

    #[test]
    fn json_mode_emits_one_object_per_line() {
        let buffer = Buffer::default();
        let mut renderer = Renderer::with_writer(RenderMode::Json, false, Box::new(buffer.clone()));
        renderer.handle(&AgentEvent::TextDelta {
            session_id: "s".into(),
            message_id: "m".into(),
            delta: "hi".into(),
        });
        let text = buffer.text();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("\"type\":\"text\""));
    }

    #[test]
    fn tool_summaries_handle_missing_params() {
        assert_eq!(describe_tool("read", &json!({})), "?");
        assert_eq!(describe_tool("git_status", &json!({})), "status");
        assert_eq!(describe_tool("exec", &json!({ "command": "ls" })), "ls");
    }
}
