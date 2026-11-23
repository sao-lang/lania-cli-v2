//! 长任务等待与中断收尾逻辑。

use std::time::Duration;

use anyhow::{anyhow, Result};
use lania_logger::LogLevel;
use lania_bridge_host::node_bridge::{
    BridgeActiveCall, BridgeEvent, BridgeExchange, BridgeRequest, BridgeResponse,
};
use tokio_util::sync::CancellationToken;

use super::super::context::CommandExecutionContext;
use super::requests::BridgeDispatchResult;

impl<'a> CommandExecutionContext<'a> {
    pub(in crate::execution) async fn maybe_wait_for_shutdown(
        &self,
        request: &BridgeRequest,
        exchange: &BridgeExchange,
        mut active: Option<BridgeActiveCall>,
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

        // 长任务运行期日志：Ctrl-C 与事件流并行消费，边到边打印。
        //
        // 两条链路都能承载运行期事件：
        // - 1:1 回落：active 句柄的 next_event()（逐事件 awaited）
        // - Hub 单轨：active=None，改由 provider.drain_events() 周期性泵出
        //   Hub 侧到达的 Event 帧（与 1:1 的 next_event 语义等价）。
        let hub_provider = self.bridge_provider();
        if let Some(call) = active.as_mut() {
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => break,
                    maybe_event = call.next_event() => {
                        match maybe_event {
                            Some(event) => self.apply_bridge_event(request, &event),
                            None => {
                                cancellation.cancelled().await;
                                break;
                            }
                        }
                    }
                }
            }
        } else if let Some(provider) = hub_provider {
            // Hub 路径：每 25ms 泵一次事件，Ctrl-C 随时可中断。
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_millis(25)) => {
                        for provider_event in provider.drain_events() {
                            if let Ok(event) = serde_json::from_value::<BridgeEvent>(
                                serde_json::json!({
                                    "method": provider_event.method,
                                    "params": provider_event.params,
                                }),
                            ) {
                                self.apply_bridge_event(request, &event);
                            }
                        }
                    }
                }
            }
        } else {
            cancellation.cancelled().await;
        }

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
        // Hub 单轨下必须经同一个 Hub provider 发 stop，才能停掉真正的 dev 端点；
        // 若误用 1:1 node_bridge 会 spawn 全新进程（activeCompiler=null），停不掉。
        let provider = self
            .bridge_provider()
            .ok_or_else(|| anyhow!("hub bridge provider is not attached; cannot stop long task"))?;
        let stop = self.node_bridge.compiler_stop_request();
        // provider.call 失败经 Err 直接上抛（Hub 单轨不掩盖错误）；
        // 成功时 error 恒 None，与 1:1 的 response 语义一致。
        let exchange = provider
            .call(&stop.method, stop.params)
            .await
            .map_err(|err| anyhow!("compiler.stop via hub failed: {err}"))?;
        let shutdown = BridgeDispatchResult {
            exchange: BridgeExchange {
                response: BridgeResponse {
                    id: stop.id,
                    result: Some(exchange.result),
                    error: None,
                },
                events: exchange
                    .events
                    .into_iter()
                    .filter_map(|e| {
                        serde_json::from_value::<BridgeEvent>(serde_json::json!({
                            "method": e.method,
                            "params": e.params,
                        }))
                        .ok()
                    })
                    .collect(),
            },
            active: None,
        };
        // stop 之后把剩余事件排空，确保最后的编译/退出日志可见。
        if let Some(call) = active.as_mut() {
            let timeout = tokio::time::sleep(Duration::from_secs(2));
            tokio::pin!(timeout);
            loop {
                tokio::select! {
                    maybe_event = call.next_event() => {
                        match maybe_event {
                            Some(event) => self.apply_bridge_event(request, &event),
                            None => break,
                        }
                    }
                    _ = &mut timeout => break,
                }
            }
        }
        Ok((Some(shutdown.exchange), true))
    }

    fn is_long_running(&self, request: &BridgeRequest, exchange: &BridgeExchange) -> bool {
        is_long_running_request(request, exchange)
    }
}

pub(in crate::execution) fn is_long_running_request(
    request: &BridgeRequest,
    exchange: &BridgeExchange,
) -> bool {
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
    use lania_bridge_host::node_bridge::{BridgeExchange, BridgeRequest, BridgeResponse};
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
