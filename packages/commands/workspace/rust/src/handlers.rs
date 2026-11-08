//! `command-workspace` 的 root/config handler。
//!
//! - `WorkspaceRootHandler`：bare `lan workspace` 时给出子命令提示（不可直接执行）。
//! - `ConfigHandler`：`config` 子树统一 handler，转发共享实现
//!   （包级偏好 / 全局偏好 / init/doctor/sync）。
//!
//! dev/build/lint/create/add/template 的 handler 分别在各自模块。

use anyhow::{bail, Result};
use async_trait::async_trait;
use lania_host::execution::{CommandExecution, CommandExecutionContext, CommandHandler};

pub(crate) struct WorkspaceRootHandler;

#[async_trait(?Send)]
impl CommandHandler for WorkspaceRootHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // bare `lan workspace` 不可执行：与 product root 同策略，给出引导文案。
        bail!(
            "{}",
            if ctx.locale() == "zh" {
                "缺少 workspace 子命令，请使用 `lan workspace <dev|build|lint|create|add>`"
            } else {
                "missing workspace subcommand, try `lan workspace <dev|build|lint|create|add>`"
            }
        );
    }
}

/// `config` 子树统一 handler：转发共享实现（包级偏好 / 全局偏好 / init/doctor/sync）。
pub(crate) struct ConfigHandler;

#[async_trait(?Send)]
impl CommandHandler for ConfigHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let command = ctx.command();
        let value = lania_command_config::execute_config(
            crate::COMMAND_NAME,
            &crate::config::config_keys(),
            command,
        )?;
        Ok(ctx.complete_template_info(value, 0))
    }
}
