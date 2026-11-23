//! bridge 请求分发、重试、超时与错误收口逻辑。

use anyhow::Result;
use lania_logger::LogLevel;
use lania_bridge_host::node_bridge::{BridgeActiveCall, BridgeError, BridgeExchange, BridgeRequest};

use super::super::{
    context::{CommandExecutionContext, ExecutionError},
    types::{EXIT_RUNTIME_ERROR, EXIT_TIMEOUT},
};
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
