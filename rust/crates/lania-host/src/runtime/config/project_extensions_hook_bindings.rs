use std::sync::Arc;

use lania_config::HookBindingSource;
use lania_hooks::{
    default_hook_kind, hook_keys, HookErrorPolicy, HookInvokerOptions, HookKind, HookRuntime,
};
use serde_json::{json, Value};

use crate::registry::CommandRegistry;
use crate::runtime::{config::collect_command_names, ProjectExtensionBootstrapSummary};
use crate::HostRuntime;

use crate::runtime::dynamic::{BridgeHookInvoker, InlineHookInvoker};

// 负责把项目配置中的 hook 绑定声明转成运行时可执行的 hook registration/invoker。
//
// 这里处理两类来源：
// - inline: handler 逻辑由 node bridge 侧的 inline hook 机制解释执行
// - plugin: handler 逻辑由某个 bridge 插件提供
//
// 拆分出来之后，项目扩展总入口只需要关心“是否要执行 hook bootstrap”，
// 而不必再感知具体的 hook kind、错误策略、invoker 选择和启动后广播逻辑。
pub(super) async fn bootstrap_project_hook_bindings(
    host: &mut HostRuntime,
    cwd: &str,
    snapshot: &lania_config::LanConfigSnapshot,
    summary: &mut ProjectExtensionBootstrapSummary,
) {
    for (hook_key, bindings) in &snapshot.hooks {
        // 非内置 hook key 直接跳过，但保留 warning，避免因为拼写错误或未来字段
        // 造成“看似配置成功，实际完全没生效”的隐式失败。
        if !crate::is_known_hook_key(hook_key) {
            summary
                .warnings
                .push(format!("skip hook `{hook_key}`: unsupported hook key"));
            continue;
        }
        for binding in bindings {
            let kind = binding
                .kind
                .as_ref()
                .map(|kind| match kind {
                    lania_config::HookBindingKind::Waterfall => HookKind::Waterfall,
                    lania_config::HookBindingKind::Parallel => HookKind::Parallel,
                })
                .unwrap_or_else(|| default_hook_kind(hook_key));
            let on_error = match binding.on_error.as_deref() {
                Some("collect") => HookErrorPolicy::Collect,
                _ => HookErrorPolicy::Throw,
            };
            let invoker_options = HookInvokerOptions {
                timeout_ms: binding.timeout_ms,
                on_error,
            };

            // inline hook 不依赖外部插件名，而是依赖配置内声明的 id。
            // 因此这里单独校验 id，并注册 InlineHookInvoker。
            if binding.r#type == Some(HookBindingSource::Inline) {
                let inline_id = binding
                    .raw
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let Some(inline_id) = inline_id else {
                    summary.warnings.push(format!(
                        "skip hook `{hook_key}`: inline hook binding requires id"
                    ));
                    continue;
                };
                Arc::get_mut(&mut host.state.hooks)
                    .expect("hooks must be uniquely held during extension bootstrap")
                    .register(crate::HookRegistration {
                        key: hook_key.clone(),
                        kind,
                        plugin: "inline".into(),
                        handler: inline_id.clone(),
                        description: format!("configured inline hook {inline_id} for {hook_key}"),
                    });
                Arc::get_mut(&mut host.state.hooks)
                    .expect("hooks must be uniquely held during extension bootstrap")
                    .register_invoker_with_options(
                        hook_key.clone(),
                        Arc::new(InlineHookInvoker {
                            node_bridge: host.services.node_bridge.clone(),
                            logger: host.services.logger.clone(),
                            cwd: cwd.to_string(),
                            inline_id,
                            command_handler_id: None,
                        }),
                        invoker_options,
                    );
                summary.lifecycle_hooks += 1;
                continue;
            }

            // plugin hook 需要显式给出 plugin + handler，二者缺一不可；
            // 否则运行阶段无法定位到真正的 bridge 调用目标。
            let (Some(plugin), Some(handler)) = (binding.plugin.clone(), binding.handler.clone())
            else {
                summary.warnings.push(format!(
                    "skip hook `{hook_key}`: plugin hook binding requires plugin and handler"
                ));
                continue;
            };
            Arc::get_mut(&mut host.state.hooks)
                .expect("hooks must be uniquely held during extension bootstrap")
                .register(crate::HookRegistration {
                    key: hook_key.clone(),
                    kind,
                    plugin: plugin.clone(),
                    handler: handler.clone(),
                    description: format!("configured hook handler {handler} for {hook_key}"),
                });
            Arc::get_mut(&mut host.state.hooks)
                .expect("hooks must be uniquely held during extension bootstrap")
                .register_invoker_with_options(
                    hook_key.clone(),
                    Arc::new(BridgeHookInvoker {
                        node_bridge: host.services.node_bridge.clone(),
                        logger: host.services.logger.clone(),
                        cwd: cwd.to_string(),
                        plugin,
                        handler,
                        command_handler_id: None,
                    }),
                    invoker_options,
                );
            summary.lifecycle_hooks += 1;
        }
    }

    // hook 注册完成后，主动广播一次配置解析完成事件。
    // 这样订阅 `on_config_resolve` 的 hook 可以感知到最终生效的原始配置快照。
    host.state
        .hooks
        .call_parallel(
            "host-runtime".into(),
            hook_keys::ON_CONFIG_RESOLVE.to_string(),
            json!({
                "cwd": cwd,
                "traceId": "",
                "command": { "name": "", "handlerId": null },
                "config": {
                    "path": snapshot.config_path.clone(),
                    "value": snapshot.raw
                }
            }),
        )
        .await
        .ok();
    // 已注册的命令也会逐个补发一次 `on_command_register`，确保项目级 hook 在
    // bootstrap 之后仍能观察到内建命令注册事件，而不是只看到后续动态新增的命令。
    for command_name in collect_command_names(host.registries.commands.commands()) {
        host.state
            .hooks
            .call_parallel(
                "host-runtime".into(),
                hook_keys::ON_COMMAND_REGISTER.to_string(),
                json!({
                    "cwd": cwd,
                    "traceId": "",
                    "command": { "name": "", "handlerId": null },
                    "registry": { "command": command_name, "source": "builtin" }
                }),
            )
            .await
            .ok();
    }
}
