use std::sync::Arc;

use anyhow::{anyhow, Result};
use lania_logger::LogLevel;
use serde_json::json;

use crate::registry::{CommandRegistry, HandlerRegistry};

use super::{super::super::HostRuntime, ProjectExtensionBootstrapSummary};
use crate::runtime::dynamic::{
    register_dynamic_target_hooks, BridgeCommandHandler, ResolvedDynamicCommands,
};

// 负责从 node bridge 拉取项目级动态命令定义，并把解析结果注入当前 HostRuntime。
//
// 这一层只关心两类注册动作：
// 1. 把解析出来的命令 spec 注册到命令注册表
// 2. 把命令 handler 及其关联 hook 注册到运行时
//
// 这样 `project_extensions.rs` 作为总编排入口时，只需要决定“要不要启动动态命令 bootstrap”，
// 不需要再了解 bridge 请求格式、handler 注册细节和统计信息更新逻辑。
pub(super) async fn bootstrap_dynamic_commands(
    host: &mut HostRuntime,
    cwd: &str,
    product_root: Option<&String>,
    is_installed_mode: bool,
    summary: &mut ProjectExtensionBootstrapSummary,
) -> Result<()> {
    // 动态命令解析由 node bridge 负责，因为 schema、插件声明和 node 侧工具能力
    // 都集中在那里。Rust host 这里只发送运行时上下文，等待 bridge 返回标准化结果。
    let exchange = host
        .services
        .node_bridge
        .call_async(
            host.services.node_bridge.request(
                "commands.resolveDynamic",
                json!({
                    "cwd": cwd,
                    "workspaceRoot": cwd,
                    "productRoot": product_root.cloned().unwrap_or_else(|| cwd.to_string()),
                    "runtimeMode": if is_installed_mode { "installed" } else { "development" }
                }),
            ),
        )
        .await?;
    if let Some(error) = exchange.response.error {
        return Err(anyhow!(
            "commands.resolveDynamic failed: [{}] {}",
            error.code,
            error.message
        ));
    }

    // bridge 成功返回后，将 JSON payload 反序列化为结构化的动态命令结果。
    // summary 中的统计字段在这里一次性更新，便于上层输出 bootstrap 摘要。
    let payload = exchange
        .response
        .result
        .ok_or_else(|| anyhow!("commands.resolveDynamic returned no payload"))?;
    let resolved: ResolvedDynamicCommands = serde_json::from_value(payload)?;
    summary.dynamic_commands = resolved.commands.len();
    summary.dynamic_handlers = resolved.handlers.len();

    // 解析阶段产生的 warning 不阻塞启动，但需要同时写日志和 summary：
    // - 日志用于即时可观测性
    // - summary 用于最终统一汇总给调用方
    for warning in &resolved.warnings {
        host.services.logger.log_with_context(
            LogLevel::Warn,
            "host.runtime",
            warning.clone(),
            None,
            Some("project_extensions".into()),
            Some("commands.resolveDynamic".into()),
        );
    }
    summary.warnings.extend(resolved.warnings.clone());

    // 先注册命令，再注册 handler/hook。
    // 原因是后续 hook 触发和命令发现都默认命令 spec 已经存在于 registry 中。
    for command in resolved.commands {
        host.registries.commands.register(command)?;
    }
    for handler in resolved.handlers {
        // 动态命令 target 自身可能附带 hook 定义，这里先把 target hooks 绑定到运行时，
        // 这样真正执行命令时，生命周期 hook 已经可以正常触发。
        register_dynamic_target_hooks(
            Arc::get_mut(&mut host.state.hooks)
                .expect("hooks must be uniquely held during extension bootstrap"),
            &host.services.node_bridge,
            &host.services.logger,
            cwd,
            &handler.handler_id,
            &handler.target,
            summary,
        );
        // 命令处理器本身通过 BridgeCommandHandler 适配到 Rust host 的 handler registry。
        // 运行阶段实际执行时，会再转发回 node bridge 对应 method。
        host.registries.handlers.register(
            &handler.handler_id,
            Box::new(BridgeCommandHandler {
                method: handler.method,
                target: handler.target,
            }),
        )?;
    }

    Ok(())
}
