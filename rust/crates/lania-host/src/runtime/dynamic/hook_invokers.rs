//! 动态 hook 的 Rust 侧调用适配层。
//!
//! Node bridge 上既可能注册插件型 hook，也可能注册 inline hook。
//! 这个模块把两种来源都适配到统一的 `HookInvoker` trait，并补上 handler 过滤、
//! bridge 日志回写和错误归一化。
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use lania_hooks::{HookInvokeOutcome, HookInvoker, HookKind};
use lania_logger::{LogLevel, LoggerService};
use lania_node_bridge::{BridgeEvent, BridgeEventMethod};
use serde_json::{json, Value};

use super::super::types::{BridgeHookInvoker, InlineHookInvoker};

// 某些 hook 只应该在特定动态命令 handler 上触发。
// 这里通过 payload.command.handlerId 做一次守卫，避免注册在 A 命令上的 hook
// 被 B 命令的生命周期事件误触发。
fn matches_command_handler(expected_handler_id: &Option<String>, payload: &Value) -> bool {
    let Some(expected_handler_id) = expected_handler_id.as_ref() else {
        return true;
    };
    let actual = payload
        .get("command")
        .and_then(|command| command.get("handlerId"))
        .and_then(Value::as_str);
    actual == Some(expected_handler_id.as_str())
}

#[async_trait]
impl HookInvoker for BridgeHookInvoker {
    async fn invoke(
        &self,
        source: &str,
        hook_key: &str,
        kind: HookKind,
        payload: &serde_json::Value,
    ) -> Result<HookInvokeOutcome> {
        if !matches_command_handler(&self.command_handler_id, payload) {
            return Ok(HookInvokeOutcome { payload: None });
        }
        // 插件型 hook 调用统一走 `hooks.invoke`，
        // Rust 侧只提供宿主上下文与当前 hook 元信息，具体插件执行留给 bridge。
        let exchange = self
            .node_bridge
            .call_async(self.node_bridge.request(
                "hooks.invoke",
                json!({
                    "cwd": self.cwd,
                    "source": source,
                    "hook": hook_key,
                    "plugin": self.plugin,
                    "handler": self.handler,
                    "kind": match kind {
                        HookKind::Waterfall => "waterfall",
                        HookKind::Parallel => "parallel",
                    },
                    "payload": payload,
                }),
            ))
            .await?;

        // bridge 侧产生的日志事件在这里回写到 host logger，
        // 这样动态 hook 的执行轨迹能够进入宿主日志体系。
        for event in &exchange.events {
            log_bridge_hook_event(&self.logger, &self.handler, event);
        }

        if let Some(error) = exchange.response.error {
            return Err(anyhow!(
                "hooks.invoke failed for {} -> {}: [{}] {}",
                hook_key,
                self.handler,
                error.code,
                error.message
            ));
        }

        let payload = exchange
            .response
            .result
            .as_ref()
            .and_then(|value: &Value| value.get("payload").cloned());
        Ok(HookInvokeOutcome { payload })
    }
}

#[async_trait]
impl HookInvoker for InlineHookInvoker {
    async fn invoke(
        &self,
        source: &str,
        hook_key: &str,
        kind: HookKind,
        payload: &serde_json::Value,
    ) -> Result<HookInvokeOutcome> {
        if !matches_command_handler(&self.command_handler_id, payload) {
            return Ok(HookInvokeOutcome { payload: None });
        }
        // inline hook 与插件 hook 的区别只在 bridge method 与目标标识不同：
        // - 插件 hook 使用 plugin + handler
        // - inline hook 使用内联 id
        let exchange = self
            .node_bridge
            .call_async(self.node_bridge.request(
                "hooks.invokeInline",
                json!({
                    "cwd": self.cwd,
                    "source": source,
                    "hook": hook_key,
                    "id": self.inline_id,
                    "kind": match kind {
                        HookKind::Waterfall => "waterfall",
                        HookKind::Parallel => "parallel",
                    },
                    "payload": payload,
                }),
            ))
            .await?;

        for event in &exchange.events {
            log_bridge_hook_event(&self.logger, "inline", event);
        }

        if let Some(error) = exchange.response.error {
            return Err(anyhow!(
                "hooks.invokeInline failed for {} -> {}: [{}] {}",
                hook_key,
                self.inline_id,
                error.code,
                error.message
            ));
        }

        let payload = exchange
            .response
            .result
            .as_ref()
            .and_then(|value: &Value| value.get("payload").cloned());
        Ok(HookInvokeOutcome { payload })
    }
}

// 目前只把 bridge 的 `event.log` 映射回宿主日志；
// 其他事件类型如果未来需要透传，可继续在这里扩展。
fn log_bridge_hook_event(logger: &LoggerService, handler: &str, event: &BridgeEvent) {
    if event.method == BridgeEventMethod::Log {
        let level = match event.params["level"].as_str().unwrap_or("info") {
            "trace" => LogLevel::Trace,
            "debug" => LogLevel::Debug,
            "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => LogLevel::Info,
        };
        let message = event.params["message"]
            .as_str()
            .unwrap_or("hook event")
            .to_string();
        logger.log_with_context(
            level,
            "host.hook",
            message,
            None,
            Some("hook_invoke".into()),
            Some(handler.into()),
        );
    }
}
