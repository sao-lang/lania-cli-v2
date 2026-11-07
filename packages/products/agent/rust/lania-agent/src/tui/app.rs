//! TUI 的应用状态：**纯数据 + 状态迁移**，不碰终端。
//!
//! 这样两种验证方式都能用同一份逻辑：
//! - 单测直接构造 `App` 并喂按键 / 事件，断言行内容和状态；
//! - `--snapshot` 用 `TestBackend` 渲染一帧，把画面打成文本，CI 可比对。

use std::sync::mpsc::Sender;

use lania_agent_core::{AgentEvent, Usage};
use crate::approval::{ApprovalDecision, ApprovalRequest};

use crate::runtime::NoticeLevel;

/// 工具卡片的状态机：`pending → running → done(ok|error)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Running,
    Ok,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolRow {
    pub name: String,
    pub argument: String,
    pub state: ToolState,
    pub detail: Option<String>,
}

/// 对话区的一行。
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    User(String),
    Assistant(String),
    Thinking(String),
    Tool(ToolRow),
    Notice(NoticeLevel, String),
    Error(String),
}

/// 待审批的弹窗（`reply` 由 broker 阻塞等待）。
pub struct Dialog {
    pub request: ApprovalRequest,
    pub reply: Sender<ApprovalDecision>,
}

impl Dialog {
    /// 回复并关闭；`reply` 只在第一次生效。
    pub fn answer(&mut self, decision: ApprovalDecision) {
        let _ = self.reply.send(decision);
    }
}

/// 多行输入框（char 索引，避免 UTF-8 边界问题）。
#[derive(Debug, Default, Clone)]
pub struct Composer {
    pub text: String,
    /// 光标位置（按 char 计）。
    pub cursor: usize,
    pub history: Vec<String>,
    /// 历史回看位置：`None` 表示在编辑区。
    pub history_index: Option<usize>,
}

impl Composer {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    fn chars(&self) -> Vec<char> {
        self.text.chars().collect()
    }

    pub fn insert(&mut self, ch: char) {
        let mut chars = self.chars();
        let index = self.cursor.min(chars.len());
        chars.insert(index, ch);
        self.cursor = index + 1;
        self.text = chars.into_iter().collect();
        self.history_index = None;
    }

    /// 批量插入（快照/粘贴用）。
    pub fn insert_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.insert(ch);
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut chars = self.chars();
        let index = self.cursor - 1;
        chars.remove(index);
        self.cursor = index;
        self.text = chars.into_iter().collect();
    }

    pub fn delete(&mut self) {
        let mut chars = self.chars();
        if self.cursor >= chars.len() {
            return;
        }
        chars.remove(self.cursor);
        self.text = chars.into_iter().collect();
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars().len());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.chars().len();
    }

    /// 取走内容并压入历史。
    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        self.history_index = None;
        if !text.trim().is_empty() {
            self.history.push(text.clone());
        }
        text
    }

    /// 上一条历史（仅当输入区为空或已在回看时生效，避免抢走多行编辑的方向键）。
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_index {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(index) => index - 1,
        };
        self.history_index = Some(next);
        self.text = self.history[next].clone();
        self.cursor = self.chars().len();
    }

    /// 下一条历史；到底后回到空编辑区。
    pub fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.history_index = None;
            self.text.clear();
            self.cursor = 0;
            return;
        }
        let next = index + 1;
        self.history_index = Some(next);
        self.text = self.history[next].clone();
        self.cursor = self.chars().len();
    }
}

/// 应用状态。
pub struct App {
    pub rows: Vec<Row>,
    pub composer: Composer,
    pub model: String,
    pub thinking: String,
    pub approval: String,
    pub session_id: String,
    pub context_tokens: u64,
    pub context_window: Option<u64>,
    pub busy: bool,
    pub spinner: usize,
    /// 距离末尾的滚动行数：0 表示跟随最新输出。
    pub scroll: usize,
    pub dialog: Option<Dialog>,
    pub quit: bool,
    /// 状态栏右侧的一次性提示（`/help` 之类）。
    pub hint: Option<String>,
    /// 正在流式生成的 assistant 行下标（用于追加 delta）。
    pub streaming_row: Option<usize>,
    /// 正在运行的工具行下标（`agent_tool_end` 用它收尾）。
    pub running_tool: Option<usize>,
    /// 待处理的思考提示是否已折叠成一行。
    pub thinking_row: Option<usize>,
}

impl App {
    pub fn new(model: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            rows: Vec::new(),
            composer: Composer::default(),
            model: model.into(),
            thinking: "medium".into(),
            approval: "always-ask".into(),
            session_id: session_id.into(),
            context_tokens: 0,
            context_window: None,
            busy: false,
            spinner: 0,
            scroll: 0,
            dialog: None,
            quit: false,
            hint: None,
            streaming_row: None,
            running_tool: None,
            thinking_row: None,
        }
    }

    pub fn push_notice(&mut self, level: NoticeLevel, text: impl Into<String>) {
        self.rows.push(Row::Notice(level, text.into()));
        self.follow();
    }

    pub fn push_error(&mut self, text: impl Into<String>) {
        self.rows.push(Row::Error(text.into()));
        self.follow();
    }

    pub fn push_user(&mut self, text: impl Into<String>) {
        self.rows.push(Row::User(text.into()));
        self.streaming_row = None;
        self.thinking_row = None;
        self.follow();
    }

    fn follow(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_up(&mut self, lines: usize) {
        let max = self.rows.len();
        self.scroll = (self.scroll + lines).min(max);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll = self.scroll.saturating_sub(lines);
    }

    /// 消费一个 agent 事件（TUI 侧唯一的事件入口）。
    pub fn on_event(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::Start { model, .. } => {
                if !model.is_empty() {
                    self.model = model.clone();
                }
                self.busy = true;
            }
            AgentEvent::TextDelta { delta, .. } => {
                if delta.is_empty() {
                    return;
                }
                let index = match self.streaming_row {
                    Some(index) if matches!(self.rows.get(index), Some(Row::Assistant(_))) => index,
                    _ => {
                        self.rows.push(Row::Assistant(String::new()));
                        let index = self.rows.len() - 1;
                        self.streaming_row = Some(index);
                        index
                    }
                };
                if let Some(Row::Assistant(text)) = self.rows.get_mut(index) {
                    text.push_str(delta);
                }
                if self.scroll == 0 {
                    self.follow();
                }
            }
            AgentEvent::ThinkingDelta { delta, .. } => {
                if delta.is_empty() {
                    return;
                }
                match self.thinking_row {
                    Some(index) if matches!(self.rows.get(index), Some(Row::Thinking(_))) => {
                        if let Some(Row::Thinking(text)) = self.rows.get_mut(index) {
                            text.push_str(delta);
                            // 只保留尾部，避免思考内容无限增长。
                            if text.chars().count() > 240 {
                                let tail: String =
                                    text.chars().skip(text.chars().count() - 240).collect();
                                *text = format!("…{tail}");
                            }
                        }
                    }
                    _ => {
                        self.rows.push(Row::Thinking(delta.clone()));
                        self.thinking_row = Some(self.rows.len() - 1);
                    }
                }
            }
            AgentEvent::ToolStart { tool, params, .. } => {
                self.rows.push(Row::Tool(ToolRow {
                    name: tool.clone(),
                    argument: crate::render::describe_tool(tool, params),
                    state: ToolState::Running,
                    detail: None,
                }));
                self.running_tool = Some(self.rows.len() - 1);
                self.thinking_row = None;
                self.streaming_row = None;
                self.follow();
            }
            AgentEvent::ToolUpdate { partial, .. } => {
                if let (Some(index), Some(note)) = (
                    self.running_tool,
                    partial.get("note").and_then(serde_json::Value::as_str),
                ) {
                    if let Some(Row::Tool(tool)) = self.rows.get_mut(index) {
                        tool.detail = Some(note.to_string());
                    }
                }
            }
            AgentEvent::ToolEnd {
                ok,
                duration_ms,
                summary,
                ..
            } => {
                if let Some(index) = self.running_tool.take() {
                    if let Some(Row::Tool(tool)) = self.rows.get_mut(index) {
                        tool.state = if *ok {
                            ToolState::Ok
                        } else {
                            ToolState::Failed
                        };
                        let mut detail = summary.clone().unwrap_or_default();
                        if let Some(ms) = duration_ms {
                            if !detail.is_empty() {
                                detail.push_str(" · ");
                            }
                            detail.push_str(&format!("{ms}ms"));
                        }
                        tool.detail = (!detail.is_empty()).then_some(detail);
                    }
                }
            }
            AgentEvent::TurnEnd { .. } => {
                self.streaming_row = None;
            }
            AgentEvent::End { usage, .. } => {
                self.apply_usage(*usage);
                self.busy = false;
            }
            AgentEvent::Error { message, .. } => {
                self.push_error(message.clone());
                self.busy = false;
            }
            AgentEvent::Other { .. } => {}
        }
    }

    pub fn apply_usage(&mut self, usage: Usage) {
        if usage.input_tokens > 0 {
            self.context_tokens = usage.input_tokens;
        }
    }

    pub fn set_turn_usage(&mut self, usage: Usage, context_tokens: u64, window: Option<u64>) {
        self.apply_usage(usage);
        if context_tokens > 0 {
            self.context_tokens = context_tokens;
        }
        if window.is_some() {
            self.context_window = window;
        }
    }

    /// 状态栏中间那段。
    pub fn status_line(&self) -> String {
        let mut parts = vec![self.model.clone()];
        parts.push(format!("thinking {}", self.thinking));
        parts.push(format!("approval {}", self.approval));
        if let Some(window) = self.context_window.filter(|window| *window > 0) {
            let percent = (self.context_tokens as f64 / window as f64 * 100.0).round() as u64;
            parts.push(format!("ctx {percent}%"));
        } else if self.context_tokens > 0 {
            parts.push(format!("ctx {} tok", self.context_tokens));
        }
        parts.join(" · ")
    }

    pub fn spinner_glyph(&self) -> &'static str {
        const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[self.spinner % FRAMES.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn delta(text: &str) -> AgentEvent {
        AgentEvent::TextDelta {
            session_id: "s".into(),
            message_id: "m".into(),
            delta: text.into(),
        }
    }

    #[test]
    fn text_deltas_accumulate_into_one_row() {
        let mut app = App::new("m", "s");
        app.push_user("hi");
        app.on_event(&delta("Hello"));
        app.on_event(&delta(", world"));
        assert_eq!(app.rows.len(), 2, "{:?}", app.rows);
        assert_eq!(app.rows[1], Row::Assistant("Hello, world".into()));
    }

    #[test]
    fn tool_rows_track_state_and_detail() {
        let mut app = App::new("m", "s");
        app.on_event(&AgentEvent::ToolStart {
            session_id: "s".into(),
            tool_call_id: "c".into(),
            tool: "read".into(),
            params: json!({ "path": "a.ts" }),
        });
        app.on_event(&AgentEvent::ToolEnd {
            session_id: "s".into(),
            tool_call_id: "c".into(),
            tool: "read".into(),
            ok: true,
            duration_ms: Some(7),
            summary: Some("lines=3".into()),
        });
        match &app.rows[0] {
            Row::Tool(tool) => {
                assert_eq!(tool.name, "read");
                assert_eq!(tool.argument, "a.ts");
                assert_eq!(tool.state, ToolState::Ok);
                assert_eq!(tool.detail.as_deref(), Some("lines=3 · 7ms"));
            }
            other => panic!("unexpected row: {other:?}"),
        }
    }

    #[test]
    fn thinking_is_collapsed_and_truncated() {
        let mut app = App::new("m", "s");
        for _ in 0..40 {
            app.on_event(&AgentEvent::ThinkingDelta {
                session_id: "s".into(),
                message_id: "m".into(),
                delta: "0123456789".into(),
            });
        }
        assert_eq!(app.rows.len(), 1, "thinking must stay in one row");
        match &app.rows[0] {
            Row::Thinking(text) => assert!(text.chars().count() <= 241, "{}", text.len()),
            other => panic!("unexpected row: {other:?}"),
        }
    }

    #[test]
    fn composer_editing_handles_unicode() {
        let mut composer = Composer::default();
        for ch in "héllo".chars() {
            composer.insert(ch);
        }
        composer.backspace();
        composer.backspace();
        assert_eq!(composer.text, "hél");
        composer.move_home();
        composer.insert('>');
        assert_eq!(composer.text, ">hél");
        composer.move_end();
        composer.delete();
        assert_eq!(composer.text, ">hél");
    }

    #[test]
    fn composer_history_walks_back_and_forth() {
        let mut composer = Composer::default();
        composer.insert('a');
        composer.take();
        composer.insert('b');
        composer.take();
        composer.history_prev();
        assert_eq!(composer.text, "b");
        composer.history_prev();
        assert_eq!(composer.text, "a");
        composer.history_next();
        assert_eq!(composer.text, "b");
        composer.history_next();
        assert_eq!(composer.text, "");
    }

    #[test]
    fn usage_updates_status_percent() {
        let mut app = App::new("m", "s");
        app.set_turn_usage(
            Usage {
                input_tokens: 50_000,
                output_tokens: 100,
                total_tokens: 50_100,
            },
            50_000,
            Some(100_000),
        );
        assert!(
            app.status_line().contains("ctx 50%"),
            "{}",
            app.status_line()
        );
    }
}
