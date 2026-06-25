//! 动态命令与动态 hook 所需的基础类型定义。
//!
//! 这里刻意只放“桥接层共享类型”，不放执行逻辑：
//! - `command.rs` 依赖这里的 `BridgeCommandHandler`
//! - `hook_invokers.rs` 依赖这里的 `BridgeHookInvoker` / `InlineHookInvoker`
//! - `project_extensions_*` 负责创建这些类型并注册到运行时
//!
//! 这样动态命令执行、动态 hook 调用和项目扩展 bootstrap 可以共享同一套类型，
//! 但彼此不会为了拿结构体定义而反向依赖对方的实现模块。

use lania_command::CommandSpec;
use lania_logger::LoggerService;
use lania_node_bridge::NodeBridgeClient;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::runtime) struct ResolvedDynamicCommands {
    // 这是 Node bridge 返回给 Rust 的“动态命令解析结果”：
    // - commands：要挂进命令树里的 CommandSpec
    // - handlers：每个 handler_id 实际要转发到哪个 bridge method
    // - warnings：解析期间发现但不阻塞启动的问题
    pub(in crate::runtime) commands: Vec<CommandSpec>,
    pub(in crate::runtime) handlers: Vec<ResolvedDynamicHandler>,
    pub(in crate::runtime) warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::runtime) struct ResolvedDynamicHandler {
    pub(in crate::runtime) handler_id: String,
    pub(in crate::runtime) method: String,
    pub(in crate::runtime) target: Value,
}

// Rust 命令注册表里真正挂载的是这个 handler。
// 它本身不解析业务语义，只保存“当命令被执行时要转发到哪个 bridge method，
// 以及对应的动态 target 描述”。
pub(in crate::runtime) struct BridgeCommandHandler {
    pub(in crate::runtime) method: String,
    pub(in crate::runtime) target: Value,
}

// 插件型 hook invoker：最终会转发到 `hooks.invoke`，
// 由 Node bridge 决定具体插件和 handler 如何执行。
pub(in crate::runtime) struct BridgeHookInvoker {
    pub(in crate::runtime) node_bridge: NodeBridgeClient,
    pub(in crate::runtime) logger: LoggerService,
    pub(in crate::runtime) cwd: String,
    pub(in crate::runtime) plugin: String,
    pub(in crate::runtime) handler: String,
    pub(in crate::runtime) command_handler_id: Option<String>,
}

// inline 型 hook invoker：最终会转发到 `hooks.invokeInline`，
// 用于执行直接挂在动态命令 target/config 中的内联 hook 逻辑。
pub(in crate::runtime) struct InlineHookInvoker {
    pub(in crate::runtime) node_bridge: NodeBridgeClient,
    pub(in crate::runtime) logger: LoggerService,
    pub(in crate::runtime) cwd: String,
    pub(in crate::runtime) inline_id: String,
    pub(in crate::runtime) command_handler_id: Option<String>,
}
