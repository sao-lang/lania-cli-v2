//! 能力层对外接口：一个 [`AgentSessionClient`] 就是一次「agent 会话」的全部依赖。
//!
//! 依赖倒置的意义（§3.1 验收项）：命令层只认这个 trait，
//! 单元测试可以用 [`mock::MockSessionClient`] 跑通主流程，不需要启动 node 进程。

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::event::{AgentEvent, Usage};
use crate::spec::{PromptOutcome, PromptRequest, SessionInfo, SessionSpec, SessionState};

/// 事件回调。同步 fn 足够：调用方在事件到达时只做渲染/记账，不做 I/O。
pub type EventSink<'a> = dyn FnMut(AgentEvent) + Send + 'a;

/// 模型目录条目（`agent.models.list`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub provider: String,
    pub id: String,
    pub context_window: Option<u64>,
    pub reasoning: bool,
}

impl ModelInfo {
    pub fn label(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }
}

/// 中断句柄：可以在**另一个任务**里打断生成，而不必持有整个 client。
///
/// TUI 的 Esc / Ctrl-C 走这条路：运行时正被 `run_turn` 可变借用，
/// 只有这个轻量句柄能跨任务调用。
#[async_trait]
pub trait SessionInterrupt: Send + Sync {
    async fn interrupt(&self, session_id: &str) -> Result<()>;
}

#[async_trait]
pub trait AgentSessionClient: Send + Sync {
    /// 建立（或恢复）一个会话。`spec.messages` 非空即为恢复。
    async fn create(&self, spec: &SessionSpec) -> Result<SessionInfo>;

    /// 跑一轮，事件边到边抛给 `on_event`。
    async fn prompt(
        &self,
        request: PromptRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<PromptOutcome>;

    /// 中断当前生成（不结束会话）。
    async fn interrupt(&self, session_id: &str) -> Result<()>;

    /// 查询会话运行状态。
    async fn state(&self, session_id: &str) -> Result<SessionState>;

    /// 列出可用模型。
    async fn models(&self, provider: Option<&str>) -> Result<Vec<ModelInfo>>;

    /// 释放会话（进程内对象，不删会话文件）。
    async fn close(&self, session_id: &str) -> Result<()>;

    /// 供 Ctrl-C / Esc 使用的独立中断入口；不支持时返回 `None`。
    fn interruptor(&self) -> Option<std::sync::Arc<dyn SessionInterrupt>> {
        None
    }
}

/// 让 `Box<dyn AgentSessionClient + Send>` 也能直接当 client 用：
/// 产品层按 trait object 持有传输，不想为每种实现写一遍泛型。
#[async_trait]
impl<T: AgentSessionClient + ?Sized> AgentSessionClient for Box<T> {
    async fn create(&self, spec: &SessionSpec) -> Result<SessionInfo> {
        (**self).create(spec).await
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<PromptOutcome> {
        (**self).prompt(request, on_event).await
    }

    async fn interrupt(&self, session_id: &str) -> Result<()> {
        (**self).interrupt(session_id).await
    }

    async fn state(&self, session_id: &str) -> Result<SessionState> {
        (**self).state(session_id).await
    }

    async fn models(&self, provider: Option<&str>) -> Result<Vec<ModelInfo>> {
        (**self).models(provider).await
    }

    async fn close(&self, session_id: &str) -> Result<()> {
        (**self).close(session_id).await
    }

    fn interruptor(&self) -> Option<std::sync::Arc<dyn SessionInterrupt>> {
        (**self).interruptor()
    }
}

/// 解析 `agent.prompt` 的 response 结果。
pub fn prompt_outcome_from_result(result: &Value) -> PromptOutcome {
    let messages = result
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let text = result
        .get("text")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| last_assistant_text(&messages));
    PromptOutcome {
        text,
        messages,
        usage: Usage::from_value(result.get("usage")),
        stop_reason: result
            .get("stopReason")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    }
}

/// 从消息序列里取最后一条 assistant 的纯文本（没有 `text` 字段时的兜底）。
pub fn last_assistant_text(messages: &[Value]) -> String {
    let mut collected = String::new();
    for message in messages.iter().rev() {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        if let Some(content) = message.get("content").and_then(Value::as_array) {
            collected = content
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
        }
        break;
    }
    collected
}

/// 解析 `agent.models.list` 的结果。
pub fn models_from_result(result: &Value) -> Vec<ModelInfo> {
    result
        .get("models")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let provider = item.get("provider").and_then(Value::as_str)?;
                    let id = item.get("id").and_then(Value::as_str)?;
                    Some(ModelInfo {
                        provider: provider.to_string(),
                        id: id.to_string(),
                        context_window: item.get("contextWindow").and_then(Value::as_u64),
                        reasoning: item
                            .get("reasoning")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 测试替身：按脚本回放事件，不碰 bridge。
pub mod mock {
    use std::sync::{Arc, Mutex};

    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::json;

    use super::*;

    /// 记录一次 `create` 的入参，便于断言「历史是否被正确回放」。
    ///
    /// 内部用 `Arc<Mutex<_>>`：clone 出来的是同一个记账本，
    /// 这样测试可以在把 client 交给运行时之后仍然读调用记录。
    #[derive(Debug, Clone, Default)]
    pub struct MockSessionClient {
        pub create_calls: Arc<Mutex<Vec<SessionSpec>>>,
        pub prompt_calls: Arc<Mutex<Vec<String>>>,
        pub interrupt_calls: Arc<Mutex<Vec<String>>>,
        /// 每轮回放的事件。
        pub scripted_events: Vec<AgentEvent>,
        /// 每轮返回的消息。
        pub scripted_messages: Vec<Value>,
        pub fail_create: Option<String>,
    }

    impl MockSessionClient {
        pub fn new() -> Self {
            Self {
                scripted_events: vec![
                    AgentEvent::Start {
                        session_id: "mock".into(),
                        model: "mock/model".into(),
                    },
                    AgentEvent::TextDelta {
                        session_id: "mock".into(),
                        message_id: "m1".into(),
                        delta: "hello".into(),
                    },
                ],
                scripted_messages: vec![json!({
                    "role": "assistant",
                    "id": "m1",
                    "content": [{ "type": "text", "text": "hello" }]
                })],
                ..Self::default()
            }
        }

        pub fn create_count(&self) -> usize {
            self.create_calls.lock().expect("lock").len()
        }

        pub fn last_spec(&self) -> Option<SessionSpec> {
            self.create_calls.lock().expect("lock").last().cloned()
        }
    }

    #[async_trait]
    impl AgentSessionClient for MockSessionClient {
        async fn create(&self, spec: &SessionSpec) -> Result<SessionInfo> {
            if let Some(message) = self.fail_create.as_deref() {
                anyhow::bail!("{message}");
            }
            self.create_calls.lock().expect("lock").push(spec.clone());
            Ok(SessionInfo {
                session_id: spec.session_id.clone(),
                model: "mock/model".into(),
                context_window: Some(200_000),
                tools: vec!["read".into(), "edit".into()],
            })
        }

        async fn prompt(
            &self,
            request: PromptRequest,
            on_event: &mut EventSink<'_>,
        ) -> Result<PromptOutcome> {
            self.prompt_calls
                .lock()
                .expect("lock")
                .push(request.prompt.clone());
            for event in &self.scripted_events {
                on_event(event.clone());
            }
            Ok(PromptOutcome {
                text: "hello".into(),
                messages: self.scripted_messages.clone(),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    total_tokens: 15,
                },
                stop_reason: Some("end_turn".into()),
            })
        }

        async fn interrupt(&self, session_id: &str) -> Result<()> {
            self.interrupt_calls
                .lock()
                .expect("lock")
                .push(session_id.to_string());
            Ok(())
        }

        async fn state(&self, _session_id: &str) -> Result<SessionState> {
            Ok(SessionState {
                is_streaming: false,
                pending_tool_calls: 0,
                context_tokens: Some(1234),
            })
        }

        async fn models(&self, _provider: Option<&str>) -> Result<Vec<ModelInfo>> {
            Ok(vec![ModelInfo {
                provider: "mock".into(),
                id: "model".into(),
                context_window: Some(200_000),
                reasoning: false,
            }])
        }

        async fn close(&self, _session_id: &str) -> Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outcome_falls_back_to_last_assistant_text() {
        let result = json!({
            "messages": [
                { "role": "assistant", "id": "a1", "content": [{ "type": "text", "text": "ok" }] }
            ],
            "usage": { "inputTokens": 3, "outputTokens": 1 }
        });
        let outcome = prompt_outcome_from_result(&result);
        assert_eq!(outcome.text, "ok");
        assert_eq!(outcome.messages.len(), 1);
        assert_eq!(outcome.usage.total_tokens, 4);
    }

    #[test]
    fn models_skip_entries_without_id() {
        let models = models_from_result(&json!({
            "models": [
                { "provider": "anthropic", "id": "claude", "contextWindow": 200000 },
                { "provider": "anthropic" }
            ]
        }));
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].label(), "anthropic/claude");
    }
}
