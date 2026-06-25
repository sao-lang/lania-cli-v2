//! 运行时生命周期阶段相关实现。
//!
//! 这里聚焦的是宿主从启动到关闭这条主时间线：
//! - 初始化阶段如何装配插件与 hook
//! - 生命周期阶段如何记录
//! - 关闭阶段如何发出 shutdown 信号并收尾

use std::{env, sync::Arc};

use anyhow::Result;
use lania_hooks::{hook_keys, HookRuntime};
use lania_logger::LogLevel;
use serde_json::json;

use crate::{plugin::LifecyclePhase, registry::PluginRegistry};

use super::HostRuntime;

impl HostRuntime {
    pub async fn initialize(&mut self) -> Result<()> {
        // 可以把 initialize 理解成“宿主启动脚本”：
        // 1. 发现插件
        // 2. 解析插件依赖/元信息
        // 3. 装载宿主内建能力
        // 4. 让插件向命令树、handler、hook bus 注册自己
        // 5. 记录生命周期阶段并进入可执行状态
        self.discover_plugins();
        self.resolve_plugins();
        self.load_plugins();
        self.bootstrap_host_capabilities();
        // 初始化 hook 用 `call_parallel`，因为这里只是广播“runtime 正在启动”这一事实；
        // 不希望某个插件把 payload 改掉，再影响别的插件看到的初始化上下文。
        self.state
            .hooks
            .call_parallel(
                "host-runtime".into(),
                hook_keys::ON_INITIALIZE.to_string(),
                json!({
                    "cwd": env::current_dir()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|_| ".".into()),
                    "traceId": "",
                    "command": { "name": "", "handlerId": null }
                }),
            )
            .await
            .ok();
        self.services.logger.log_with_context(
            LogLevel::Debug,
            "host.runtime",
            "bootstrapping host runtime",
            None,
            Some("setup".into()),
            None,
        );
        // setup 阶段要求 hooks 被唯一持有（Arc::get_mut）：
        // 这里会注册 invokers/handlers 等可变结构，避免运行中并发修改。
        let hooks = Arc::get_mut(&mut self.state.hooks)
            .expect("hooks must be uniquely held during setup");
        self.registries.plugin_registry.setup_all(
            &mut self.registries.commands,
            hooks,
            &mut self.state.capabilities,
            &mut self.registries.handlers,
        )?;
        for meta in self.registries.plugin_registry.metas() {
            self.state
                .hooks
                .call_parallel(
                    meta.name.clone(),
                    hook_keys::ON_PLUGIN_LOADED.to_string(),
                    json!({
                        "cwd": env::current_dir()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|_| ".".into()),
                        "traceId": "",
                        "command": { "name": "", "handlerId": null },
                        "plugin": { "name": meta.name.clone(), "kind": "rust" }
                    }),
                )
                .await
                .ok();
            self.services.logger.log_with_context(
                LogLevel::Debug,
                "host.runtime",
                format!("loaded plugin {}", meta.name),
                None,
                Some("setup".into()),
                Some(meta.name.clone()),
            );
        }
        self.state.phase_history.push(LifecyclePhase::Setup);
        self.runtime_start();
        Ok(())
    }

    pub async fn shutdown_async(&mut self) -> Result<()> {
        self.services.logger.log_with_context(
            LogLevel::Debug,
            "host.runtime",
            "shutting down host runtime",
            None,
            Some("shutdown".into()),
            None,
        );
        self.state
            .hooks
            .call_parallel(
                "host-runtime".into(),
                hook_keys::ON_SHUTDOWN.to_string(),
                json!({
                    "cwd": env::current_dir()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|_| ".".into()),
                    "traceId": "",
                    "command": { "name": "", "handlerId": null },
                    "shutdown": { "reason": "completed" }
                }),
            )
            .await
            .ok();
        self.services.node_bridge.shutdown_async().await?;
        self.state.phase_history.push(LifecyclePhase::Shutdown);
        Ok(())
    }

    pub fn lifecycle_phases(&self) -> Vec<LifecyclePhase> {
        if self.state.phase_history.is_empty() {
            // phase_history 为空时返回“理想生命周期模板”，
            // 方便外部在 runtime 尚未真正跑过时也能知道完整阶段顺序长什么样。
            return vec![
                LifecyclePhase::Discover,
                LifecyclePhase::Resolve,
                LifecyclePhase::Load,
                LifecyclePhase::Setup,
                LifecyclePhase::RuntimeStart,
                LifecyclePhase::CommandExecute,
                LifecyclePhase::Shutdown,
            ];
        }

        self.state.phase_history.clone()
    }

    pub fn discover_plugins(&mut self) {
        self.state.phase_history.push(LifecyclePhase::Discover);
    }

    pub fn resolve_plugins(&mut self) {
        self.state.phase_history.push(LifecyclePhase::Resolve);
    }

    pub fn load_plugins(&mut self) {
        self.state.phase_history.push(LifecyclePhase::Load);
    }

    pub fn runtime_start(&mut self) {
        self.services.logger.log_with_context(
            LogLevel::Debug,
            "host.runtime",
            "runtime_start phase reached",
            None,
            Some("runtime_start".into()),
            None,
        );
        self.state.phase_history.push(LifecyclePhase::RuntimeStart);
    }

    pub fn record_command_execution(&mut self) {
        self.state.phase_history.push(LifecyclePhase::CommandExecute);
    }

    pub fn shutdown(&mut self) {
        self.services.logger.log_with_context(
            LogLevel::Debug,
            "host.runtime",
            "shutting down host runtime",
            None,
            Some("shutdown".into()),
            None,
        );
        self.services.node_bridge.shutdown();
        self.state.phase_history.push(LifecyclePhase::Shutdown);
    }
}
