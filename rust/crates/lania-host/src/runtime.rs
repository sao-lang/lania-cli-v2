//! HostRuntime 负责装配宿主能力、驱动插件生命周期并执行命令。
//!
//! 这个文件现在只保留运行时的公共入口与结构定义：
//! - `Host` trait 定义对外只读视图
//! - `HostRuntime` 结构体保存运行时状态
//! - 具体实现拆到 `runtime/` 下的内部子模块，降低单文件复杂度

use std::sync::Arc;

use lania_command::CommandSpec;
use lania_exec::ExecService;
use lania_fs::FsService;
use lania_git::GitService;
use lania_hooks::{HookBusImpl, HookRuntime, HookSnapshot};
use lania_logger::LoggerService;
use lania_node_bridge::NodeBridgeClient;
use lania_pm::PackageManagerService;
use lania_progress::ProgressService;
use lania_prompt::PromptService;
use lania_task::TaskService;

use crate::{
    capability::CapabilityContainer,
    plugin::LifecyclePhase,
    registry::{
        CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl, PluginRegistryImpl,
    },
    CapabilityResolver,
};

pub use self::config::{
    PluginManifest, ProjectExtensionBootstrapSummary, ProjectPluginReport, RuntimeSummary,
};

pub trait Host {
    // `Host` 是一个“只暴露只读视图”的 trait：
    // - 外部只需要拿命令树、能力解析器和 hook 快照来做查询/展示；
    // - 不需要也不应该直接修改运行时内部状态。
    fn command_specs(&self) -> &[CommandSpec];
    fn capability_resolver(&self) -> &dyn CapabilityResolver;
    fn hook_snapshot(&self) -> HookSnapshot;
}

pub(super) struct HostRuntimeRegistries {
    pub(super) plugin_registry: PluginRegistryImpl,
    pub(super) commands: CommandRegistryImpl,
    pub(super) handlers: CommandHandlerRegistryImpl,
}

pub(super) struct HostRuntimeServices {
    pub(super) logger: LoggerService,
    pub(super) exec: ExecService,
    pub(super) fs: FsService,
    pub(super) tasks: TaskService,
    pub(super) progress: ProgressService,
    pub(super) prompt: PromptService,
    pub(super) git: GitService,
    pub(super) package_manager: PackageManagerService,
    pub(super) node_bridge: NodeBridgeClient,
}

pub(super) struct HostRuntimeState {
    pub(super) hooks: Arc<HookBusImpl>,
    pub(super) capabilities: CapabilityContainer,
    pub(super) phase_history: Vec<LifecyclePhase>,
    pub(super) locale: String,
}

pub struct HostRuntime {
    // 顶层只保留 3 组聚合：
    // - registries: 插件/命令/handler 的注册中心
    // - services: 执行命令时要用到的宿主能力
    // - state: hook/capability/locale/lifecycle 等运行时状态
    pub(super) registries: HostRuntimeRegistries,
    pub(super) services: HostRuntimeServices,
    pub(super) state: HostRuntimeState,
}

impl Host for HostRuntime {
    fn command_specs(&self) -> &[CommandSpec] {
        self.registries.commands.commands()
    }

    fn capability_resolver(&self) -> &dyn CapabilityResolver {
        &self.state.capabilities
    }

    fn hook_snapshot(&self) -> HookSnapshot {
        self.state.hooks.snapshot()
    }
}

mod catalog;
mod config;
mod core;
mod dynamic;
mod execute;
mod host_rpc;
mod lifecycle;

#[cfg(test)]
mod tests;
