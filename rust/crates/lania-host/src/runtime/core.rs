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

    /// 登记内置的 Python bridge（`lan-py-bridge`，§3.3）。
    ///
    /// 失败（没有 python 解释器）时**不报错**：回落矩阵要求「装了就更好，不装也能用」
    /// （§5.3），因此这里按 best-effort 处理，由调用方用 `has_runtime_bridge` 判断。
    pub fn register_python_bridge(&self, cwd: &std::path::Path) -> Result<()> {
        let launch = lania_py_bridge::launch_spec(cwd)?;
        self.register_runtime_bridge(
            RuntimeKind::Python,
            crate::runtime::bridges::BridgeProgram::new(launch.program, launch.args, launch.dir),
        )
    }

    /// 登记内置的 Go bridge（`lan-go-bridge`，§3.4）。
    ///
    /// **延迟解析**：登记只记下 cwd 与解析函数，首次真正需要时才会
    /// 「定位 go + 编译一次 bridge 并缓存」（见 `lania_go_bridge::ensure_binary`）。
    /// 这样 Go 项目里执行 `lan env` / `lan add` 不会白白付一次 `go build`。
    pub fn register_go_bridge(&self, cwd: &std::path::Path) -> Result<()> {
        self.services
            .bridges
            .borrow_mut()
            .register_deferred(RuntimeKind::Go, cwd, |dir| {
                let launch = lania_go_bridge::launch_spec(dir)?;
                Ok(crate::runtime::bridges::BridgeProgram::new(
                    launch.program,
                    launch.args,
                    launch.dir,
                ))
            })
    }

    /// 登记内置的 Dart bridge（`lan-dart-bridge`，§3.6，同样延迟解析）。
    pub fn register_dart_bridge(&self, cwd: &std::path::Path) -> Result<()> {
        self.services
            .bridges
            .borrow_mut()
            .register_deferred(RuntimeKind::Dart, cwd, |dir| {
                let launch = lania_dart_bridge::launch_spec(dir)?;
                Ok(crate::runtime::bridges::BridgeProgram::new(
                    launch.program,
                    launch.args,
                    launch.dir,
                ))
            })
    }

    /// 登记内置的 Rust bridge（`lan-rs-bridge`，§3.5 / B11）。
    ///
    /// 与其它 bridge 不同：它**不引入新产物** —— `lan` 二进制自己以 bridge 模式
    /// （`lan __lania_rs_bridge`）承担协议循环，因此这里就是「自己再起一个自己」。
    /// 仍然走延迟解析，避免 `lan env` 这类命令白解析一次 `current_exe`。
    pub fn register_rust_bridge(&self, cwd: &std::path::Path) -> Result<()> {
        self.services
            .bridges
            .borrow_mut()
            .register_deferred(RuntimeKind::Rust, cwd, |dir| {
                let launch = lania_rs_bridge::launch_spec(dir)?;
                Ok(crate::runtime::bridges::BridgeProgram::new(
                    launch.program,
                    launch.args,
                    launch.dir,
                ))
            })
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
