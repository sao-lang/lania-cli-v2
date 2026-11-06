//! 传输实现：把宿主的 `NodeBridgeClient` 包成产品层认识的 `AgentSessionClient`。
//!
//! 放在适配层而不是产品层，是因为「跟谁通信」是集成决策，
//! 产品只认 trait。独立二进制可以换一份传输而不动产品代码。

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;

use lania_agent_core::client::{
    models_from_result, prompt_outcome_from_result, AgentSessionClient, EventSink, ModelInfo,
    SessionInterrupt,
};
use lania_agent_core::{
    AgentEvent, PromptOutcome, PromptRequest, SessionInfo, SessionSpec, SessionState,
};
use lania_node_bridge::{BridgeEventMethod, BridgeExchange, NodeBridgeClient};

/// 基于宿主 bridge 的会话客户端。
///
/// `NodeBridgeClient` 内部是 `Arc`，clone 出去仍指向同一个 bridge 子进程，
/// 因此这里直接持有一份 clone（而不是借用），运行时才能满足 `'static`。
#[derive(Clone)]
pub struct BridgeAgentSessionClient {
    client: NodeBridgeClient,
}

impl BridgeAgentSessionClient {
    pub fn new(client: NodeBridgeClient) -> Self {
        Self { client }
    }

    fn ok_result(exchange: BridgeExchange) -> Result<Value> {
        if let Some(error) = exchange.response.error.as_ref() {
            return Err(anyhow!("{}: {}", error.code, error.message));
        }
        Ok(exchange.response.result.unwrap_or(Value::Null))
    }
}

#[async_trait]
impl AgentSessionClient for BridgeAgentSessionClient {
    async fn create(&self, spec: &SessionSpec) -> Result<SessionInfo> {
        let request = self.client.request("agent.session.create", spec.create_params());
        let result = Self::ok_result(self.client.call_async(request).await?)?;
        Ok(SessionInfo::from_result(&spec.session_id, &result))
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<PromptOutcome> {
        let bridge_request = self.client.request("agent.prompt", request.params());

        let mut call = self.client.open_call(bridge_request)?;
        // 事件通道在 response 到达时关闭，此时再 `collect_exchange` 取最终结果。
        while let Some(event) = call.next_event().await {
            let method = method_name(&event.method);
            if let Some(parsed) = AgentEvent::from_method(&method, &event.params) {
                on_event(parsed);
            }
        }
        let result = Self::ok_result(call.collect_exchange().await?)?;
        Ok(prompt_outcome_from_result(&result))
    }

    async fn interrupt(&self, session_id: &str) -> Result<()> {
        let request = self.client.request(
            "agent.interrupt",
            serde_json::json!({ "sessionId": session_id }),
        );
        Self::ok_result(self.client.call_async(request).await?)?;
        Ok(())
    }

    async fn state(&self, session_id: &str) -> Result<SessionState> {
        let request = self
            .client
            .request("agent.state", serde_json::json!({ "sessionId": session_id }));
        let result = Self::ok_result(self.client.call_async(request).await?)?;
        Ok(SessionState {
            is_streaming: result
                .get("isStreaming")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            pending_tool_calls: result
                .get("pendingToolCalls")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
            context_tokens: result.get("contextTokens").and_then(Value::as_u64),
        })
    }

    async fn models(&self, provider: Option<&str>) -> Result<Vec<ModelInfo>> {
        let request = self.client.request(
            "agent.models.list",
            serde_json::json!({ "provider": provider }),
        );
        let result = Self::ok_result(self.client.call_async(request).await?)?;
        Ok(models_from_result(&result))
    }

    async fn close(&self, session_id: &str) -> Result<()> {
        let request = self.client.request(
            "agent.session.dispose",
            serde_json::json!({ "sessionId": session_id }),
        );
        // 关闭失败不该阻塞退出流程。
        let _ = self.client.call_async(request).await;
        Ok(())
    }

    fn interruptor(&self) -> Option<Arc<dyn SessionInterrupt>> {
        Some(Arc::new(BridgeInterrupt {
            client: self.client.clone(),
        }))
    }
}

/// 轻量中断句柄：只带一份 bridge clone，可以在任意任务里用。
struct BridgeInterrupt {
    client: NodeBridgeClient,
}

#[async_trait]
impl SessionInterrupt for BridgeInterrupt {
    async fn interrupt(&self, session_id: &str) -> Result<()> {
        let request = self.client.request(
            "agent.interrupt",
            serde_json::json!({ "sessionId": session_id }),
        );
        let _ = self.client.call_async(request).await;
        Ok(())
    }
}

/// 协议枚举 → 线上方法名（core 只认字符串，不认协议类型）。
fn method_name(method: &BridgeEventMethod) -> String {
    serde_json::to_value(method)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_default()
}

/// 便于测试/替换的构造入口。
pub fn bridge_client(client: NodeBridgeClient) -> Box<dyn AgentSessionClient + Send> {
    Box::new(BridgeAgentSessionClient::new(client))
}

