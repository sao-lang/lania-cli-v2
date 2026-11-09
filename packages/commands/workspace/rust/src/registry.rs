//! `command-workspace` 的注册胶水（commands + handlers）。
//!
//! 注册顺序：
//! 1. `register_builtin_command_handlers`：workspace root spec + 全部 handler
//!    （root/dev/build/lint/lint check/fix/create（含 template）/add/config 子树，
//!    handler_id 全部保持原值）
//! 2. mount 5 个原顶级命令到 `lan workspace` 下（template 挂在 create 之下）

use anyhow::Result;
use lania_host::{
    plugin::{register_builtin_command_handlers, PluginSetupContext},
    CommandHandler,
};

use crate::add::AddCommandHandler;
use crate::build::BuildCommandHandler;
use crate::create::CreateCommandHandler;
use crate::create_template::{TemplateCommandHandler, TEMPLATE_HANDLER_ID};
use crate::dev::DevCommandHandler;
use crate::handlers::{ConfigHandler, WorkspaceRootHandler};
use crate::lint::LintCommandHandler;
use crate::spec::{add_spec, build_spec, create_spec, dev_spec, lint_spec, workspace_root_spec};
use crate::{
    ADD_HANDLER_ID, BUILD_HANDLER_ID, CREATE_HANDLER_ID, DEV_HANDLER_ID, HANDLER_ID,
    LINT_CHECK_HANDLER_ID, LINT_FIX_HANDLER_ID, LINT_HANDLER_ID,
};

pub(crate) fn register_workspace_commands(ctx: &mut PluginSetupContext<'_>) -> Result<()> {
    // 注册根命令与全部 handler（handler_id 保持原包冻结值）。
    let config_ids = lania_command_config::handler_ids(crate::COMMAND_NAME);
    let mut handlers: Vec<(&str, Box<dyn CommandHandler>)> = vec![
        (
            HANDLER_ID,
            Box::new(WorkspaceRootHandler) as Box<dyn CommandHandler>,
        ),
        (
            DEV_HANDLER_ID,
            Box::new(DevCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            BUILD_HANDLER_ID,
            Box::new(BuildCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            LINT_HANDLER_ID,
            Box::new(LintCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            LINT_CHECK_HANDLER_ID,
            Box::new(LintCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            LINT_FIX_HANDLER_ID,
            Box::new(LintCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            CREATE_HANDLER_ID,
            Box::new(CreateCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            TEMPLATE_HANDLER_ID,
            Box::new(TemplateCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            ADD_HANDLER_ID,
            Box::new(AddCommandHandler) as Box<dyn CommandHandler>,
        ),
    ];
    for id in &config_ids {
        handlers.push((
            id.as_str(),
            Box::new(ConfigHandler) as Box<dyn CommandHandler>,
        ));
    }
    register_builtin_command_handlers(
        ctx,
        "command-workspace",
        crate::COMMAND_NAME,
        workspace_root_spec(),
        handlers,
    )?;

    // 5 个原顶级命令降级为 `lan workspace <sub>` 子命令
    // （template 挂在 create 之下；product 由独立子包 packages/commands/product 承载）。
    ctx.commands.mount_subcommand("workspace", dev_spec())?;
    ctx.commands.mount_subcommand("workspace", build_spec())?;
    ctx.commands.mount_subcommand("workspace", lint_spec())?;
    ctx.commands.mount_subcommand("workspace", create_spec())?;
    ctx.commands.mount_subcommand("workspace", add_spec())?;
    Ok(())
}
