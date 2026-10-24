//! 长任务等待与中断收尾逻辑。

use std::time::Duration;

use anyhow::{anyhow, Result};
use lania_logger::LogLevel;
use lania_node_bridge::{BridgeExchange, BridgeRequest};
use tokio_util::sync::CancellationToken;

use super::super::context::CommandExecutionContext;

impl<'a> CommandExecutionContext<'a> {
    pub(in crate::execution) async fn maybe_wait_for_shutdown(
        &self,
        request: &BridgeRequest,
        exchange: &BridgeExchange,
    ) -> Result<(Option<BridgeExchange>, bool)> {
        if !self.is_long_running(request, exchange) {
            return Ok((None, false));
        }

        // 这里用一个 `CancellationToken` 同时承载两种“停止条件”：
        // - 用户主动按 Ctrl-C
        // - 测试/调试场景下通过策略触发自动中断
        let cancellation = CancellationToken::new();
        let signal_token = cancellation.clone();
        let signal_task = tokio::spawn(async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                return Err(anyhow!("failed to wait for interrupt: {error}"));
            }
            signal_token.cancel();
            Ok::<(), anyhow::Error>(())
        });
        let auto_interrupt_task = self.policy.auto_interrupt_after_ms.map(|delay_ms| {
            let auto_interrupt_token = cancellation.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                auto_interrupt_token.cancel();
            })
        });

        cancellation.cancelled().await;

        signal_task.abort();
        if let Some(task) = auto_interrupt_task {
            task.abort();
        }
        match signal_task.await {
            Ok(Ok(())) | Err(_) => {}
            Ok(Err(error)) => return Err(error),
        }

        self.log(
            LogLevel::Info,
            "command_execute",
            Some(request.method.clone()),
            "stopping long-running bridge task",
        );

        // 目前长任务统一走 compiler.stop 作为“温和停止”信号。
        let shutdown = self
            .dispatch_bridge_request(&self.node_bridge.compiler_stop_request())
            .await?;
        Ok((Some(shutdown), true))
    }

    fn is_long_running(&self, request: &BridgeRequest, exchange: &BridgeExchange) -> bool {
        is_long_running_request(request, exchange)
    }
}

fn is_long_running_request(request: &BridgeRequest, exchange: &BridgeExchange) -> bool {
    // 失败结果（success=false）不是长任务：否则非 web dev 启动失败会一直等 Ctrl-C。
    let result = exchange.response.result.as_ref();
    let failed = result
        .and_then(|value| value.get("success"))
        .and_then(|value| value.as_bool())
        == Some(false);
    if failed {
        return false;
    }
    matches!(request.method.as_str(), "compiler.dev")
        || (request.method == "compiler.build"
            && result
                .and_then(|value| value.get("watch"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false))
        || result
            .and_then(|value| value.get("longRunning"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use lania_node_bridge::{BridgeExchange, BridgeRequest, BridgeResponse};
    use serde_json::json;

    use super::is_long_running_request;

    fn request(method: &str) -> BridgeRequest {
        BridgeRequest {
            id: "req-1".into(),
            method: method.into(),
            params: json!({}),
        }
    }

    fn exchange(result: serde_json::Value) -> BridgeExchange {
        BridgeExchange {
            response: BridgeResponse {
                id: "req-1".into(),
                result: Some(result),
                error: None,
            },
            events: vec![],
        }
    }

    #[test]
    fn failed_dev_result_is_not_long_running() {
        assert!(!is_long_running_request(
            &request("compiler.dev"),
            &exchange(json!({ "success": false }))
        ));
    }

    #[test]
    fn successful_dev_result_is_long_running() {
        assert!(is_long_running_request(
            &request("compiler.dev"),
            &exchange(json!({ "success": true, "longRunning": true }))
        ));
        assert!(is_long_running_request(
            &request("compiler.dev"),
            &exchange(json!({}))
        ));
    }

    #[test]
    fn build_watch_is_long_running_but_plain_build_is_not() {
        assert!(is_long_running_request(
            &request("compiler.build"),
            &exchange(json!({ "watch": true }))
        ));
        assert!(!is_long_running_request(
            &request("compiler.build"),
            &exchange(json!({ "watch": false }))
        ));
    }

    #[test]
    fn explicit_long_running_flag_wins() {
        assert!(is_long_running_request(
            &request("something.else"),
            &exchange(json!({ "longRunning": true }))
        ));
    }
}
