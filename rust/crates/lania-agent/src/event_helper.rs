//! 测试与快照共用的 `AgentEvent` 构造器。
//!
//! 手写 `AgentEvent::ToolStart { session_id: "s".into(), .. }` 太吵，
//! 这里只保留 TUI 测试真正关心的字段。

use serde_json::json;

use lania_agent_core::AgentEvent;

pub fn text_delta(delta: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        session_id: "s".into(),
        message_id: "m".into(),
        delta: delta.into(),
    }
}

pub fn thinking_delta(delta: &str) -> AgentEvent {
    AgentEvent::ThinkingDelta {
        session_id: "s".into(),
        message_id: "m".into(),
        delta: delta.into(),
    }
}

/// `read` / `edit` / `write` 用 `path`，`exec` 用 `command`：按工具名挑字段，
/// 保证渲染出来的摘要和真实运行时一致。
pub fn tool_start(tool: &str, argument: &str) -> AgentEvent {
    let params = if tool == "exec" {
        json!({ "command": argument })
    } else {
        json!({ "path": argument })
    };
    AgentEvent::ToolStart {
        session_id: "s".into(),
        tool_call_id: "c".into(),
        tool: tool.into(),
        params,
    }
}

pub fn tool_end(ok: bool) -> AgentEvent {
    AgentEvent::ToolEnd {
        session_id: "s".into(),
        tool_call_id: "c".into(),
        tool: "tool".into(),
        ok,
        duration_ms: Some(12),
        summary: None,
    }
}
