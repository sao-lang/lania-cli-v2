//! `HostRuntime` 的基础构造与只读访问器。
//!
//! 这一层只处理“宿主本身怎么被装配出来”，不掺杂生命周期或命令执行流程，
//! 方便在阅读时先把运行时有哪些组件看清楚。

use std::{env, sync::Arc};

use anyhow::Result;
use lania_exec::ExecService;
use lania_fs::FsService;
use lania_git::GitService;
use lania_hooks::HookBusImpl;
use lania_logger::{LogLevel, LoggerService};
use lania_node_bridge::{BridgeClientConfig, NodeBridgeClient};
use lania_pm::PackageManagerService;
use lania_progress::ProgressService;
use lania_prompt::PromptService;
use lania_runtime::RuntimeService;
use lania_task::TaskService;

use crate::{
    plugin::Plugin,
    registry::{
        CommandHandlerRegistryImpl, CommandRegistryImpl, PluginRegistry, PluginRegistryImpl,
    },
};

use super::bridges::RuntimeKind;
use super::{
    host_rpc::{HostRpcAdapter, HostRpcAdapterDeps},
    HostRuntime, HostRuntimeRegistries, HostRuntimeServices, HostRuntimeState,
};

impl HostRuntime {
    pub fn new() -> Self {
        let locale = lania_preferences::load_preferences().locale;
        let tasks = TaskService::default();
        let progress = ProgressService::default();
        // TaskService 和 ProgressService 通过 sink 连接：
        // - task 执行会产生事件
        // - progress 负责把这些事件转成终端可见的进度/状态
        tasks.add_sink(progress.task_sink());
        let logger = configured_logger();
        let exec = ExecService::new(false);
        let fs = FsService;
        let git = GitService::default();
        let package_manager = PackageManagerService;
        let runtime = RuntimeService::new();
        let prompt = PromptService::default();
        let node_bridge = NodeBridgeClient::new(BridgeClientConfig::default())
            .with_host_rpc_handler(HostRpcAdapter::new(HostRpcAdapterDeps {
                exec: exec.clone(),
                git: git.clone(),
                package_manager: package_manager.clone(),
                runtime: runtime.clone(),
                fs: fs.clone(),
                logger: logger.clone(),
                tasks: tasks.clone(),
                progress: progress.clone(),
                prompt: prompt.clone(),
            }));
        Self {
            registries: HostRuntimeRegistries {
                plugin_registry: PluginRegistryImpl::new(),
                commands: CommandRegistryImpl::new(),
                handlers: CommandHandlerRegistryImpl::new(),
            },
            services: HostRuntimeServices {
                logger,
                exec,
                fs,
                tasks,
                progress,
                prompt,
                git,
                package_manager,
                runtime,
                node_bridge,
                bridges: super::bridges::shared_registry(),
            },
            state: HostRuntimeState {
                hooks: Arc::new(HookBusImpl::new()),
                capabilities: crate::capability::CapabilityContainer::new(),
                phase_history: vec![],
                locale,
            },
        }
    }

    pub fn set_locale(&mut self, locale: impl Into<String>) {
        self.state.locale = lania_preferences::normalize_locale(&locale.into());
    }

    pub fn register_plugin(&mut self, plugin: Box<dyn Plugin>) -> Result<()> {
        self.registries.plugin_registry.register(plugin)
    }

    pub fn handshake_preview(&self) -> lania_node_bridge::HandshakeRequest {
        self.services.node_bridge.handshake_request()
    }

    pub fn logger(&self) -> &LoggerService {
        &self.services.logger
    }

    pub fn exec(&self) -> &ExecService {
        &self.services.exec
    }

    pub fn tasks(&self) -> &TaskService {
        &self.services.tasks
    }

    pub fn fs(&self) -> &FsService {
        &self.services.fs
    }

    pub fn progress(&self) -> &ProgressService {
        &self.services.progress
    }

    pub fn prompt(&self) -> &PromptService {
        &self.services.prompt
    }

    pub fn git(&self) -> &GitService {
        &self.services.git
    }

    pub fn package_manager(&self) -> &PackageManagerService {
        &self.services.package_manager
    }

    pub fn runtime(&self) -> &RuntimeService {
        &self.services.runtime
    }

    pub fn node_bridge(&self) -> &NodeBridgeClient {
        &self.services.node_bridge
    }

    /// 按运行时取 bridge 客户端。
    ///
    /// JS 簇（node / bun / deno）永远返回共享的 node bridge；
    /// 其他运行时若未登记/未启动，返回错误 —— 调用方据此回落到 CLI spawn（§5.3）。
    pub fn bridge(&self, kind: RuntimeKind) -> Result<NodeBridgeClient> {
        self.services
            .bridges
            .borrow()
            .client_for(kind, &self.services.node_bridge)
    }

    /// 登记一个独立运行时 bridge 的可执行文件（不启动进程）。
    pub fn register_runtime_bridge(
        &self,
        kind: RuntimeKind,
        program: crate::runtime::bridges::BridgeProgram,
    ) -> Result<()> {
        self.services.bridges.borrow_mut().register(kind, program)
    }

    /// 该运行时的 bridge 是否可用（用于 `doctor` / 回落决策）。
    pub fn has_runtime_bridge(&self, kind: RuntimeKind) -> bool {
        self.services.bridges.borrow().is_registered(kind)
    }
}

fn configured_logger() -> LoggerService {
    let level = match env::var("LANIA_LOG_LEVEL")
        .ok()
        .map(|value| value.to_lowercase())
        .as_deref()
    {
        // 显式的 `LANIA_LOG_LEVEL` 优先级最高，因为它表达的是“我要哪个精确等级”。
        Some("trace") => LogLevel::Trace,
        Some("debug") => LogLevel::Debug,
        Some("warn") => LogLevel::Warn,
        Some("error") => LogLevel::Error,
        Some("info") => LogLevel::Info,
        // 兼容旧环境变量：
        // - `LANIA_TRACE=1` / `LANIA_DEBUG=1` 更像快捷开关
        // - 只有在没设置 `LANIA_LOG_LEVEL` 时才生效
        _ if env::var("LANIA_TRACE").ok().as_deref() == Some("1") => LogLevel::Trace,
        _ if env::var("LANIA_DEBUG").ok().as_deref() == Some("1") => LogLevel::Debug,
        _ => LogLevel::Info,
    };
    LoggerService::default().with_min_level(level)
}

impl Default for HostRuntime {
    fn default() -> Self {
        Self::new()
    }
}
