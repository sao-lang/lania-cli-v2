//! 命令执行上下文与执行结果封装，统一 bridge 调用、超时、重试和中断处理。
//!
//! 主要导出：exit_code、from_env、new、command、has_capability、require_capability。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定

mod bridge_api;
mod bridge_helpers;
mod context;
mod types;
mod utils;

// `execution` 模块可以看成宿主的“命令执行中台”：
// - `context` 提供执行期可用的能力集合
// - `bridge_api` 封装 Rust -> Node bridge 调用
// - `bridge_helpers` 处理超时、重试、中断等控制流细节
// - `types` 统一“命令执行结果”的外部表示
//
// 这样做的好处是：命令 handler 只需要描述业务逻辑，
// 不需要在每个命令里重复写一遍 timeout / Ctrl-C / exit code / host_state 拼装。
pub use context::{CommandExecutionContext, ExecutionError, ExecutionPolicy, HostExecutionServices};
pub use types::{
    BridgeCommandRun, CommandExecution, CommandHandler, EXIT_CANCELLED, EXIT_LINT_FAILED,
    EXIT_RUNTIME_ERROR, EXIT_SUCCESS, EXIT_TIMEOUT,
};

#[cfg(test)]
mod tests;
