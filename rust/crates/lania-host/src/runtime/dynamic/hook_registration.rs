use std::sync::Arc;

use lania_hooks::{
    default_hook_kind, HookBusImpl, HookErrorPolicy, HookInvokerOptions, HookKind, HookRuntime,
};
use lania_logger::LoggerService;
use lania_node_bridge::NodeBridgeClient;
use serde_json::Value;

use crate::runtime::ProjectExtensionBootstrapSummary;

use super::super::types::{BridgeHookInvoker, InlineHookInvoker};

// 负责把动态命令 target 中声明的 hooks 注册到 HookBus。
// 它只做“解析 target 声明 -> 构造 invoker -> 注册到 runtime”这条链路，
// 不负责真正执行 hook。
pub(in crate::runtime) fn register_dynamic_target_hooks(
    hooks: &mut HookBusImpl,
    node_bridge: &NodeBridgeClient,
    logger: &LoggerService,
    cwd: &str,
    handler_id: &str,
    target: &Value,
    summary: &mut ProjectExtensionBootstrapSummary,
) {
    // 没有 hooks 字段就直接返回；动态命令允许完全不声明生命周期 hook。
    let Some(object) = target.get("hooks").and_then(Value::as_object) else {
        return;
    };
    for (hook_key, items) in object {
        // 只接受宿主认识的 hook key，未知键写 warning 但不阻塞动态命令注册。
        if !crate::is_known_hook_key(hook_key) {
            summary.warnings.push(format!(
                "skip dynamic command hook `{hook_key}` on `{handler_id}`: unsupported hook key"
            ));
            continue;
        }
        let Some(bindings) = items.as_array() else {
            summary.warnings.push(format!(
                "skip dynamic command hook `{hook_key}` on `{handler_id}`: bindings must be an array"
            ));
            continue;
        };
        for binding in bindings {
            let Some(binding_object) = binding.as_object() else {
                continue;
            };
            let binding_type = binding_object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("plugin");
            let on_error = match binding_object.get("onError").and_then(Value::as_str) {
                Some("collect") => HookErrorPolicy::Collect,
                _ => HookErrorPolicy::Throw,
            };
            let invoker_options = HookInvokerOptions {
                timeout_ms: binding_object.get("timeoutMs").and_then(Value::as_u64),
                on_error,
            };
            if binding_type == "inline" {
                // inline 绑定直接通过 id 指向内联 hook 逻辑，不依赖插件名。
                let Some(inline_id) = binding_object
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                else {
                    summary.warnings.push(format!(
                        "skip dynamic command hook `{hook_key}` on `{handler_id}`: inline binding requires id"
                    ));
                    continue;
                };
                let kind = match binding_object.get("kind").and_then(Value::as_str) {
                    Some("waterfall") => HookKind::Waterfall,
                    Some("parallel") => HookKind::Parallel,
                    _ => default_hook_kind(hook_key),
                };
                hooks.register(crate::HookRegistration {
                    key: hook_key.clone(),
                    kind,
                    plugin: "inline".into(),
                    handler: inline_id.clone(),
                    description: format!(
                        "dynamic command inline hook {inline_id} for {hook_key} on {handler_id}"
                    ),
                });
                hooks.register_invoker_with_options(
                    hook_key.clone(),
                    Arc::new(InlineHookInvoker {
                        node_bridge: node_bridge.clone(),
                        logger: logger.clone(),
                        cwd: cwd.to_string(),
                        inline_id,
                        command_handler_id: Some(handler_id.to_string()),
                    }),
                    invoker_options,
                );
                summary.lifecycle_hooks += 1;
                continue;
            }
            if binding_type != "plugin" {
                summary.warnings.push(format!(
                    "skip dynamic command hook `{hook_key}` on `{handler_id}`: unsupported binding type `{binding_type}`"
                ));
                continue;
            }
            // plugin 绑定则必须明确给出 plugin + handler；
            // 解析后交给 BridgeHookInvoker 在运行时转发。
            let Some(plugin) = binding_object
                .get("plugin")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let Some(handler) = binding_object
                .get("handler")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let kind = match binding_object.get("kind").and_then(Value::as_str) {
                Some("waterfall") => HookKind::Waterfall,
                Some("parallel") => HookKind::Parallel,
                _ => default_hook_kind(hook_key),
            };
            hooks.register(crate::HookRegistration {
                key: hook_key.clone(),
                kind,
                plugin: plugin.clone(),
                handler: handler.clone(),
                description: format!(
                    "dynamic command hook handler {handler} for {hook_key} on {handler_id}"
                ),
            });
            hooks.register_invoker_with_options(
                hook_key.clone(),
                Arc::new(BridgeHookInvoker {
                    node_bridge: node_bridge.clone(),
                    logger: logger.clone(),
                    cwd: cwd.to_string(),
                    plugin,
                    handler,
                    command_handler_id: Some(handler_id.to_string()),
                }),
                invoker_options,
            );
            summary.lifecycle_hooks += 1;
        }
    }
}
