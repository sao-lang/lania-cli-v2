//! 动态命令执行入口。
//!
//! 这一层负责把 Rust 侧的 `CommandExecutionContext` 和 Node 侧动态命令目标接起来，
//! 并在真正转发前补上宿主侧 prompt 机会。

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

use crate::execution::{CommandExecution, CommandExecutionContext};

use super::{prompt::maybe_prompt_dynamic_command, types::BridgeCommandHandler};

#[async_trait(?Send)]
impl crate::CommandHandler for BridgeCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let mut argv = ctx.command().argv.clone();
        // 动态命令在真正发给 Node 之前，仍然有一次“宿主侧补问”的机会。
        // 这样项目自定义命令也能复用统一的 prompt / fallback / timeout / 脱敏逻辑。
        maybe_prompt_dynamic_command(ctx, &self.method, &self.target, &mut argv).await?;
        // Rust host 在这里把本地执行上下文补齐成 bridge 请求参数：
        // - cwd/workspaceRoot/productRoot 描述本次运行所在的宿主环境
        // - handlerId/traceId/argv 还原当前命令调用现场
        // - target 携带动态命令在解析阶段得到的完整目标描述
        let request = ctx.node_bridge().request(
            self.method.clone(),
            {
                let runtime_mode = std::env::var("LANIA_RUNTIME_MODE")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| {
                        if std::env::var("LANIA_PRODUCT_ROOT")
                            .ok()
                            .filter(|value| !value.trim().is_empty() && value != &ctx.command().cwd)
                            .is_some()
                        {
                            "installed".into()
                        } else {
                            "development".into()
                        }
                    });
            json!({
                "cwd": ctx.command().cwd,
                "workspaceRoot": ctx.command().cwd,
                "productRoot": std::env::var("LANIA_PRODUCT_ROOT").unwrap_or_else(|_| ctx.command().cwd.clone()),
                "runtimeMode": runtime_mode,
                "handlerId": ctx.command().handler_id,
                "traceId": ctx.command().trace_id,
                "argv": argv,
                "target": self.target.clone(),
            })
            },
        );
        // 真正的业务执行发生在 Node bridge 侧；Rust 这里只负责等待 exchange 并把结果
        // 重新包装回统一的 CommandExecution。
        let run = ctx.call_bridge(request).await?;
        // 动态命令返回的 exitCode 由 bridge 结果提供；缺失时回退为 0，
        // 保持与内建命令“未显式失败即成功”的语义一致。
        let exit_code = run
            .exchange
            .response
            .result
            .as_ref()
            .and_then(|value| value["exitCode"].as_i64())
            .unwrap_or(0) as i32;
        Ok(ctx.complete_bridge(run, exit_code))
    }
}
