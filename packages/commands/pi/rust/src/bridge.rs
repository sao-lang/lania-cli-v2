//! 传输实现：把 Hub `BridgeProvider` 包成产品层认识的 `AgentSessionClient`。
//!
//! 放在适配层而不是产品层，是因为「跟谁通信」是集成决策，
//! 产品只认 trait。独立二进制可以换一份传输而不动产品代码。

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use lania_agent_core::client::{
    models_from_result, prompt_outcome_from_result, AgentSessionClient, EventSink, ModelInfo,
    SessionInterrupt,
};
use lania_agent_core::{
    AgentEvent, PromptOutcome, PromptRequest, SessionInfo, SessionSpec, SessionState,
};
use lania_host::{BridgeProvider, BridgeProviderEvent};
use lania_node_bridge::{BridgeEventMethod, RequestFactory};

/// 基于 Hub provider 的会话客户端（单轨，无 1:1 NodeBridgeClient）。
///
/// `Arc<dyn BridgeProvider>` clone 共享同一个 Hub；`RequestFactory` 零大小，
/// 只负责生成 BridgeRequest（method + params），执行统一经 provider.call[_stream]。
#[derive(Clone)]
pub struct BridgeAgentSessionClient {
    provider: Arc<dyn BridgeProvider>,
    factory: RequestFactory,
}

impl BridgeAgentSessionClient {
    pub fn new(provider: Arc<dyn BridgeProvider>, factory: RequestFactory) -> Self {
        Self { provider, factory }
    }
}

#[async_trait]
impl AgentSessionClient for BridgeAgentSessionClient {
    async fn create(&self, spec: &SessionSpec) -> Result<SessionInfo> {
        let request = self.factory.request("agent.session.create", spec.create_params());
        let result = self.call(&request.method, request.params).await?;
        Ok(SessionInfo::from_result(&spec.session_id, &result))
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<PromptOutcome> {
        let bridge_request = self.factory.request("agent.prompt", request.params());
        // Hub 流式调用：事件到达即回调（逐 token 渲染），不等 response。
        let result = self
            .provider
            .call_stream(&bridge_request.method, bridge_request.params, &mut |event: BridgeProviderEvent| {
                if let Some(parsed) = AgentEvent::from_method(&event.method, &event.params) {
                    on_event(parsed);
                }
            })
            .await?;
        Ok(prompt_outcome_from_result(&result))
    }

    async fn interrupt(&self, session_id: &str) -> Result<()> {
        self.call("agent.interrupt", serde_json::json!({ "sessionId": session_id }))
            .await?;
        Ok(())
    }

    async fn state(&self, session_id: &str) -> Result<SessionState> {
        let result = self
            .call("agent.state", serde_json::json!({ "sessionId": session_id }))
            .await?;
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
        let result = self
            .call(
                "agent.models.list",
                serde_json::json!({ "provider": provider }),
            )
            .await?;
        Ok(models_from_result(&result))
    }

    async fn close(&self, session_id: &str) -> Result<()> {
        // 关闭失败不该阻塞退出流程。
        let _ = self
            .call("agent.session.dispose", serde_json::json!({ "sessionId": session_id }))
            .await;
        Ok(())
    }

    fn interruptor(&self) -> Option<Arc<dyn SessionInterrupt>> {
        Some(Arc::new(BridgeInterrupt {
            provider: Arc::clone(&self.provider),
            factory: self.factory.clone(),
        }))
    }
}

impl BridgeAgentSessionClient {
    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let exchange = self.provider.call(method, params).await?;
        Ok(exchange.result)
    }
}

/// 轻量中断句柄：只带 provider clone，可以在任意任务里用。
struct BridgeInterrupt {
    provider: Arc<dyn BridgeProvider>,
    factory: RequestFactory,
}

#[async_trait]
impl SessionInterrupt for BridgeInterrupt {
    async fn interrupt(&self, session_id: &str) -> Result<()> {
        let request = self
            .factory
            .request("agent.interrupt", serde_json::json!({ "sessionId": session_id }));
        let _ = self.provider.call(&request.method, request.params).await;
        Ok(())
    }
}

fn method_name(method: &BridgeEventMethod) -> String {
    serde_json::to_value(method)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_default()
}

/// 便于测试/替换的构造入口（Hub 单轨：不再接受 NodeBridgeClient）。
pub fn bridge_client(
    provider: Arc<dyn BridgeProvider>,
    factory: RequestFactory,
) -> Box<dyn AgentSessionClient + Send> {
    Box::new(BridgeAgentSessionClient::new(provider, factory))
}

