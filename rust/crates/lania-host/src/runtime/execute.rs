//! `HostRuntime::execute_command()` 相关实现。
//!
//! 这个子模块聚焦“命令真正开始执行时”宿主需要做的工作：
//! - 查找 handler
//! - 触发参数改写类 hook
//! - 组装 `CommandExecutionContext`
//! - 统一成功/失败收尾

use std::sync::Arc;

use anyhow::{anyhow, Result};
use lania_command::{CommandContext, CommandSpec};
use lania_hooks::{hook_keys, HookRuntime};
use lania_logger::LogLevel;
use serde_json::{json, Value};

use crate::{
    execution::{
        CommandExecution, CommandExecutionContext, ExecutionError, HostExecutionServices,
        EXIT_RUNTIME_ERROR,
    },
    registry::{CommandRegistry, HandlerRegistry},
};

use super::HostRuntime;

impl HostRuntime {
    pub async fn execute_command(&mut self, context: &CommandContext) -> Result<CommandExecution> {
        // 每次执行新命令前，都重置“本次命令的 secret 注册表”。
        // 这样最终输出做脱敏时，不会把上一次命令的敏感信息串进来。
        self.services.prompt.reset_secrets();

        let handler = self
            .registries
            .handlers
            .get(context.handler_id.as_str())
            .ok_or_else(|| {
                anyhow!(
                    "no command handler registered for {} in command_execute phase",
                    context.handler_id
                )
            })?;
        let command_name = command_name_for_handler(&self.registries.commands, &context.handler_id);
        // v2.1: `onCommandPreInit` / `onArgsParsed` 是 waterfall hook：
        // - 允许插件“改写” argv（例如默认值补全、别名展开、兼容旧参数）
        // - 因此这里必须把 payload 中的新 argv 重新写回 command_context
        let mut command_context = context.clone();
        let args_parsed_payload: Value = self
            .state
            .hooks
            .call_waterfall(
                "host-runtime".into(),
                hook_keys::ON_COMMAND_PRE_INIT.to_string(),
                json!({
                    "cwd": command_context.cwd,
                    "traceId": command_context.trace_id,
                    "command": { "name": command_name, "handlerId": command_context.handler_id },
                    "argv": { "raw": [] }
                }),
            )
            .await
            .unwrap_or_else(|_| {
                // pre-init hook 失败时不直接中断执行，而是退回最小 payload。
                json!({
                    "cwd": command_context.cwd,
                    "traceId": command_context.trace_id,
                    "command": { "name": command_name, "handlerId": command_context.handler_id },
                    "argv": { "raw": [] }
                })
            });
        let args_parsed_payload: Value = self
            .state
            .hooks
            .call_waterfall(
                "host-runtime".into(),
                hook_keys::ON_ARGS_PARSED.to_string(),
                json!({
                    "cwd": command_context.cwd,
                    "traceId": command_context.trace_id,
                    "command": { "name": command_name, "handlerId": command_context.handler_id },
                    "argv": { "args": command_context.argv.args, "options": command_context.argv.options }
                }),
            )
            .await
            .unwrap_or(args_parsed_payload);

        // 这里不是增量 merge，而是“按 hook 返回值整体覆盖”：
        // HostRuntime 把 waterfall 的输出视为“新的最终参数视图”。
        if let Some(argv) = args_parsed_payload.get("argv").and_then(Value::as_object) {
            if let Some(args) = argv.get("args").and_then(Value::as_object) {
                command_context.argv.args =
                    args.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            }
            if let Some(options) = argv.get("options").and_then(Value::as_object) {
                command_context.argv.options = options
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
            }
        }
        self.services.logger.log_with_context(
            LogLevel::Info,
            "host.runtime",
            format!("executing handler {}", context.handler_id),
            Some(context.trace_id.clone()),
            Some("command_execute".into()),
            Some(context.handler_id.clone()),
        );

        // 项目配置不是强依赖，因此这里只做 best-effort 读取。
        let project_config = self
            .load_lan_config_snapshot_from_cwd_async(context.cwd.clone())
            .await
            .ok();
        let execution_context = CommandExecutionContext::new(&command_context, HostExecutionServices {
            capabilities: &self.state.capabilities,
            logger: &self.services.logger,
            exec: &self.services.exec,
            fs: &self.services.fs,
            tasks: &self.services.tasks,
            progress: &self.services.progress,
            prompt: &self.services.prompt,
            git: &self.services.git,
            package_manager: &self.services.package_manager,
            node_bridge: &self.services.node_bridge,
            hooks: Arc::clone(&self.state.hooks) as Arc<dyn HookRuntime>,
            project_config,
            locale: self.state.locale.clone(),
        });
        let mut execution = match handler.execute(&execution_context).await {
            Ok(execution) => execution,
            Err(error) => {
                self.state
                    .hooks
                    .call_parallel(
                        "host-runtime".into(),
                        hook_keys::ON_ERROR.to_string(),
                        json!({
                            "cwd": context.cwd,
                            "traceId": context.trace_id,
                            "command": {
                                "name": command_name_for_handler(&self.registries.commands, &context.handler_id),
                                "handlerId": context.handler_id
                            },
                            "error": { "message": error.to_string() }
                        }),
                    )
                    .await
                    .ok();
                self.services.logger.log_with_context(
                    LogLevel::Error,
                    "host.runtime",
                    error.to_string(),
                    Some(context.trace_id.clone()),
                    Some("command_execute".into()),
                    Some(context.handler_id.clone()),
                );
                return Err(preserve_execution_error(
                    anyhow!(
                        "handler {} failed during command_execute phase: {error}",
                        context.handler_id
                    )
                    .context(error.to_string()),
                    error
                        .downcast_ref::<ExecutionError>()
                        .map(|inner| inner.exit_code),
                ));
            }
        };
        if execution.exit_code() == 0 {
            self.state
                .hooks
                .call_parallel(
                    "host-runtime".into(),
                    hook_keys::ON_SUCCESS.to_string(),
                    json!({
                        "cwd": context.cwd,
                        "traceId": context.trace_id,
                        "command": {
                            "name": command_name_for_handler(&self.registries.commands, &context.handler_id),
                            "handlerId": context.handler_id
                        },
                        "result": { "exitCode": execution.exit_code() }
                    }),
                )
                .await
                .ok();
        } else {
            self.state
                .hooks
                .call_parallel(
                    "host-runtime".into(),
                    hook_keys::ON_ERROR.to_string(),
                    json!({
                        "cwd": context.cwd,
                        "traceId": context.trace_id,
                        "command": {
                            "name": command_name_for_handler(&self.registries.commands, &context.handler_id),
                            "handlerId": context.handler_id
                        },
                        "error": { "message": "command failed", "exitCode": execution.exit_code() }
                    }),
                )
                .await
                .ok();
        }
        execution = execution.with_host_state(execution_context.host_state());
        self.record_command_execution();
        Ok(execution)
    }
}

fn command_name_for_handler(
    commands: &crate::registry::CommandRegistryImpl,
    handler_id: &str,
) -> String {
    fn find(specs: &[CommandSpec], handler_id: &str) -> Option<String> {
        for spec in specs {
            if spec.handler_id == handler_id {
                return Some(spec.name.clone());
            }
            if let Some(found) = find(&spec.subcommands, handler_id) {
                return Some(found);
            }
        }
        None
    }

    // 优先返回“用户认识的命令名”，而不是内部 handler_id。
    // 这会影响日志、hooks payload、错误信息的可读性。
    find(commands.commands(), handler_id).unwrap_or_else(|| handler_id.to_string())
}

fn preserve_execution_error(error: anyhow::Error, exit_code: Option<i32>) -> anyhow::Error {
    if let Some(exit_code) = exit_code {
        // 已经知道更具体的退出码时，优先保留它，避免被 `anyhow` 抹平成统一失败。
        return ExecutionError {
            exit_code,
            message: error.to_string(),
        }
        .into();
    }

    // 否则退回宿主通用的 runtime error。
    ExecutionError {
        exit_code: EXIT_RUNTIME_ERROR,
        message: error.to_string(),
    }
    .into()
}
