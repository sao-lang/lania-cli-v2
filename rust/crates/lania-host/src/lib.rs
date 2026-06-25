//! 宿主运行时总入口，聚合 capability、hook、plugin、registry 与 runtime。
pub mod capability;
pub mod execution;
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
pub use runtime::{Host, HostRuntime};
