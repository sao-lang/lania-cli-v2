//! 宿主运行时总入口，聚合 capability、hook、plugin、registry 与 runtime。
pub mod approval;
pub mod bridge_setup;
pub mod capability;
pub mod execution;
pub mod guard;
// Hook 运行时已拆分到独立的 `lania-hooks` crate（避免与 host/workflows 等形成循环依赖）。
pub mod plugin;
pub mod registry;
pub mod runtime;

pub use capability::{
    CapabilityContainer, CapabilityName, CapabilityRegistrar, CapabilityResolver,
    CapabilitySnapshot,
};
pub use execution::{
    BridgeCommandRun, CommandExecution, CommandExecutionContext, CommandHandler, ExecutionError,
    ExecutionPolicy, EXIT_CANCELLED, EXIT_LINT_FAILED, EXIT_RUNTIME_ERROR, EXIT_SUCCESS,
    EXIT_TIMEOUT,
};
pub use lania_hooks::{
    default_hook_kind, hook_keys, is_known_hook_key, HookBusImpl, HookInvokeOutcome, HookInvoker,
    HookKind, HookRegistration, HookRuntime, HookSnapshot,
};
pub use plugin::{
    record_builtin_command_registration, register_builtin_command,
    register_builtin_command_handlers, LifecyclePhase, NodePluginMeta, Plugin, PluginKind,
    PluginMeta, PluginSetupContext,
};
pub use registry::{
    CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl, HandlerRegistry,
    PluginRegistry, PluginRegistryImpl,
};
pub use runtime::bridges::{BridgeProgram, BridgeRegistry, RuntimeKind};
pub use runtime::{
    decide_runtime, default_lint_tool, detect_project_runtime, detect_runtime, RuntimeDecision,
    FEATURE_PRIORITY,
};
pub use runtime::{Host, HostRuntime};

// 命令二进制的统一桥接装配入口（消除各 bin 里的重复块）。
pub use bridge_setup::{configure_command_bridges, maybe_run_rust_bridge};

// facade：host-core 的桥无关接缝由宿主统一重新导出（§5.1 A.5(6) 去环过渡）。
pub use lania_host_core::{
    capability_not_linked, BridgeLaunchSpec, BridgeProvider, BridgeSlot, ConfigSource,
    NullConfigSource,
};
