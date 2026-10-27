//! bridge 请求分发、重试、超时与错误收口逻辑。

use std::time::Duration;

use anyhow::Result;
use lania_logger::LogLevel;
use lania_node_bridge::{BridgeActiveCall, BridgeError, BridgeExchange, BridgeRequest};

use super::super::{
    context::{CommandExecutionContext, ExecutionError},
    types::{EXIT_RUNTIME_ERROR, EXIT_TIMEOUT},
};
use super::long_running::is_long_running_request;

/// 分发结果：长任务会带回活跃句柄，供 response 之后继续消费运行期事件。
pub(in crate::execution) struct BridgeDispatchResult {
    pub exchange: BridgeExchange,
    pub active: Option<BridgeActiveCall>,
}

impl<'a> CommandExecutionContext<'a> {
    pub(in crate::execution) fn raise_bridge_error(
        &self,
        request: &BridgeRequest,
        error: &Option<BridgeError>,
    ) -> Result<()> {
        if let Some(error) = error {
            // 到这里说明“协议层成功返回了 response，但 response 里声明了 error”。
            let exit_code = if error.code == "E_TIMEOUT" {
                EXIT_TIMEOUT
            } else {
                EXIT_RUNTIME_ERROR
            };
            return Err(ExecutionError {
                exit_code,
                message: format!(
                    "bridge error in command_execute phase for handler {} (trace {}), method {}: [{}] {}",
                    self.command.handler_id,
                    self.command.trace_id,
                    request.method,
                    error.code,
                    error.message
                ),
            }
            .into());
        }

        Ok(())
    }

    pub(in crate::execution) async fn dispatch_bridge_request(
        &self,
        request: &BridgeRequest,
    ) -> Result<BridgeDispatchResult> {
        // 发送请求并在需要时重试。
        for attempt in 0..=self.policy.retry_attempts {
            let result = match self.node_bridge.open_call(request.clone()) {
                Ok(call) => {
                    tokio::time::timeout(
                        Duration::from_millis(self.policy.timeout_ms),
                        self.collect_bridge_exchange(request, call),
                    )
                    .await
                }
                Err(_) => {
                    // open_call 失败通常表示当前 bridge 不支持流式 events；退回到 request/response。
                    let result = tokio::time::timeout(
                        Duration::from_millis(self.policy.timeout_ms),
                        self.node_bridge.call_async(request.clone()),
                    )
                    .await;
                    result.map(|result| {
                        result.map(|exchange| {
                            self.apply_bridge_events(request, &exchange);
                            BridgeDispatchResult {
                                exchange,
                                active: None,
                            }
                        })
                    })
                }
            };
            match result {
                Ok(Ok(dispatch)) => {
                    if dispatch.exchange.response.error.is_some()
                        && attempt < self.policy.retry_attempts
                    {
                        self.log(
                            LogLevel::Warn,
                            "command_execute",
                            Some(request.method.clone()),
                            format!(
                                "retrying bridge request after error, attempt {}/{}",
                                attempt + 1,
                                self.policy.retry_attempts + 1
                            ),
                        );
                        continue;
                    }
                    return Ok(dispatch);
                }
                Ok(Err(error)) => {
                    self.log(
                        LogLevel::Warn,
                        "command_execute",
                        Some(request.method.clone()),
                        format!(
                            "bridge request failed before receiving a response, attempt {}/{}: {error}",
                            attempt + 1,
                            self.policy.retry_attempts + 1
                        ),
                    );
                    if attempt == self.policy.retry_attempts {
                        return Err(ExecutionError {
                            exit_code: EXIT_RUNTIME_ERROR,
                            message: format!(
                                "bridge request {} failed before receiving a response: {error}",
                                request.method
                            ),
                        }
                        .into());
                    }
                }
                Err(_) => {
                    self.log(
                        LogLevel::Warn,
                        "command_execute",
                        Some(request.method.clone()),
                        format!(
                            "bridge request timed out after {}ms, attempt {}/{}",
                            self.policy.timeout_ms,
                            attempt + 1,
                            self.policy.retry_attempts + 1
                        ),
                    );
                    if attempt == self.policy.retry_attempts {
                        return Err(ExecutionError {
                            exit_code: EXIT_TIMEOUT,
                            message: format!(
                                "bridge request {} timed out after {}ms",
                                request.method, self.policy.timeout_ms
                            ),
                        }
                        .into());
                    }
                }
            }
        }

        Err(ExecutionError {
            exit_code: EXIT_RUNTIME_ERROR,
            message: format!("bridge request {} exhausted retries", request.method),
        }
        .into())
    }

    async fn collect_bridge_exchange(
        &self,
        request: &BridgeRequest,
        mut call: BridgeActiveCall,
    ) -> Result<BridgeDispatchResult> {
        // 事件一边到达一边渲染，避免构建/启动日志被延迟到命令结束才显示。
        let exchange = call
            .collect_until_response(|event| self.apply_bridge_event(request, event))
            .await?;
        let active = if is_long_running_request(request, &exchange) {
            Some(call)
        } else {
            None
        };
        Ok(BridgeDispatchResult { exchange, active })
    }

    pub(in crate::execution) fn log(
        &self,
        level: LogLevel,
        phase: &str,
        operation: Option<String>,
        message: impl Into<String>,
    ) {
        self.logger.log_with_context(
            level,
            self.logger.scope().to_string(),
            message,
            Some(self.command.trace_id.clone()),
            Some(phase.to_string()),
            operation,
        );
    }
}
