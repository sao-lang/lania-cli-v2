//! agent 事件模型：bridge 的 `event.agent_*` 到强类型枚举的**唯一**映射点。
//!
//! 之所以单独一层而不是到处 `params.get("delta")`：
//! - 渲染层、会话落盘、hooks 都消费同一种结构，字段改名只改这里；
//! - 单测可以脱离 bridge 构造事件，不需要起 node 进程。

use serde_json::Value;

/// 一次模型调用的用量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

impl Usage {
    /// 兼容 pi 的两种命名（`inputTokens` / `input`）以及缺失字段。
    pub fn from_value(value: Option<&Value>) -> Self {
        let Some(value) = value.filter(|value| value.is_object()) else {
            return Self::default();
        };
        let input = pick_u64(value, &["inputTokens", "input", "promptTokens"]);
        let output = pick_u64(value, &["outputTokens", "output", "completionTokens"]);
        let total = pick_u64(value, &["totalTokens", "total"]).max(input + output);
        Self {
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total_tokens == 0 && self.input_tokens == 0 && self.output_tokens == 0
    }

    /// `12.3k in / 456 out` 这样的紧凑标签，供状态栏使用。
    pub fn compact_label(&self) -> String {
        format!(
            "{} in / {} out",
            compact_number(self.input_tokens),
            compact_number(self.output_tokens)
        )
    }
}

fn compact_number(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn pick_u64(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or_default()
}

/// 工具调用的生命周期事件在渲染层对应三态：`pending → running → done`。
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    Start {
        session_id: String,
        model: String,
    },
    TextDelta {
        session_id: String,
        message_id: String,
        delta: String,
    },
    ThinkingDelta {
        session_id: String,
        message_id: String,
        delta: String,
    },
    ToolStart {
        session_id: String,
        tool_call_id: String,
        tool: String,
        params: Value,
    },
    ToolUpdate {
        session_id: String,
        tool_call_id: String,
        partial: Value,
    },
    ToolEnd {
        session_id: String,
        tool_call_id: String,
        tool: String,
        ok: bool,
        duration_ms: Option<u64>,
        summary: Option<String>,
    },
    TurnEnd {
        session_id: String,
        usage: Usage,
    },
    End {
        session_id: String,
        usage: Usage,
        stop_reason: Option<String>,
    },
    Error {
        session_id: String,
        code: String,
        message: String,
    },
    /// 未识别的事件（向前兼容：新事件不应当让旧 CLI 崩掉）。
    Other {
        method: String,
        params: Value,
    },
}

impl AgentEvent {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Start { session_id, .. }
            | Self::TextDelta { session_id, .. }
            | Self::ThinkingDelta { session_id, .. }
            | Self::ToolStart { session_id, .. }
            | Self::ToolUpdate { session_id, .. }
            | Self::ToolEnd { session_id, .. }
            | Self::TurnEnd { session_id, .. }
            | Self::End { session_id, .. }
            | Self::Error { session_id, .. } => Some(session_id),
            Self::Other { .. } => None,
        }
    }

    /// 从「方法名 + 参数」转换；非 agent 事件返回 `None`。
    ///
    /// 参数用方法名字符串而不是枚举，是为了让 core 不依赖任何传输协议包：
    /// 传输层（bridge / in-process）自己把事件拆成 (method, params) 再进来。
    pub fn from_method(method: &str, params: &Value) -> Option<Self> {
        let session_id = || {
            params
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };

        Some(match method {
            "event.agent_start" => Self::Start {
                session_id: session_id(),
                model: text("model"),
            },
            "event.agent_text_delta" => Self::TextDelta {
                session_id: session_id(),
                message_id: text("messageId"),
                delta: text("delta"),
            },
            "event.agent_thinking_delta" => Self::ThinkingDelta {
                session_id: session_id(),
                message_id: text("messageId"),
                delta: text("delta"),
            },
            "event.agent_tool_start" => Self::ToolStart {
                session_id: session_id(),
                tool_call_id: text("toolCallId"),
                tool: text("tool"),
                params: params.get("params").cloned().unwrap_or(Value::Null),
            },
            "event.agent_tool_update" => Self::ToolUpdate {
                session_id: session_id(),
                tool_call_id: text("toolCallId"),
                partial: params.get("partial").cloned().unwrap_or(Value::Null),
            },
            "event.agent_tool_end" => Self::ToolEnd {
                session_id: session_id(),
                tool_call_id: text("toolCallId"),
                tool: text("tool"),
                ok: params.get("ok").and_then(Value::as_bool).unwrap_or(true),
                duration_ms: params.get("durationMs").and_then(Value::as_u64),
                summary: params
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            },
            "event.agent_turn_end" => Self::TurnEnd {
                session_id: session_id(),
                usage: Usage::from_value(params.get("usage")),
            },
            "event.agent_end" => Self::End {
                session_id: session_id(),
                usage: Usage::from_value(params.get("usage")),
                stop_reason: params
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            },
            "event.agent_error" => Self::Error {
                session_id: session_id(),
                code: text("code"),
                message: text("message"),
            },
            _ => Self::Other {
                method: method.to_string(),
                params: params.clone(),
            },
        })
    }

    /// agent 域事件（其余是编译器/lint 等无关事件）。
    pub fn is_agent_event(method: &str) -> bool {
        method.starts_with("event.agent_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_delta_maps_to_typed_event() {
        let parsed = AgentEvent::from_method(
            "event.agent_text_delta",
            &json!({ "sessionId": "s", "messageId": "m1", "delta": "hi" }),
        )
        .expect("agent event");
        assert_eq!(
            parsed,
            AgentEvent::TextDelta {
                session_id: "s".into(),
                message_id: "m1".into(),
                delta: "hi".into(),
            }
        );
    }

    #[test]
    fn usage_accepts_both_namings() {
        assert_eq!(
            Usage::from_value(Some(&json!({ "inputTokens": 10, "outputTokens": 5 }))),
            Usage {
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15
            }
        );
        assert_eq!(
            Usage::from_value(Some(&json!({ "input": 7, "output": 3, "total": 10 }))),
            Usage {
                input_tokens: 7,
                output_tokens: 3,
                total_tokens: 10
            }
        );
        assert!(Usage::from_value(None).is_empty());
    }

    #[test]
    fn non_agent_event_is_filtered() {
        assert!(!AgentEvent::is_agent_event("event.log"));
        assert!(AgentEvent::is_agent_event("event.agent_text_delta"));
    }

    #[test]
    fn compact_labels_round_numbers() {
        assert_eq!(
            Usage {
                input_tokens: 12_300,
                output_tokens: 456,
                total_tokens: 12_756
            }
            .compact_label(),
            "12.3k in / 456 out"
        );
    }
}
