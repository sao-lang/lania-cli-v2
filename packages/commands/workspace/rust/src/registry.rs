//! `command-workspace` 的注册胶水（commands + handlers）。
//!
//! 注册顺序：
//! 1. `register_builtin_command_handlers`：workspace root spec + 全部 handler
//!    （root/dev/product dev/build/pack/publish/inspect/doctor/lint/lint check/fix/
//!    create/template/add/config 子树，handler_id 全部保持原值）
//! 2. `ensure_product_root`：幂等补齐共享的 `lan product` 根命令
//! 3. mount 5 个原顶级命令 + template 到 `lan workspace` 下
//! 4. mount 6 个 product 子命令到 `lan product` 下

use anyhow::Result;
use lania_host::{
    plugin::{register_builtin_command_handlers, PluginSetupContext},
    CommandHandler,
};

use crate::add::AddCommandHandler;
use crate::build::{
    BuildCommandHandler, DoctorProductCommandHandler, InspectProductCommandHandler,
    PackProductCommandHandler, ProductBuildCommandHandler, PublishProductCommandHandler,
};
use crate::create::CreateCommandHandler;
use crate::create_template::{TemplateCommandHandler, TemplateCommandSpec, TEMPLATE_HANDLER_ID};
use crate::dev::{DevCommandHandler, ProductDevCommandHandler};
use crate::handlers::{ConfigHandler, WorkspaceRootHandler};
use crate::lint::LintCommandHandler;
use crate::spec::{
    add_spec, build_spec, create_spec, dev_spec, lint_spec, product_build_spec, product_dev_spec,
    product_doctor_spec, product_inspect_spec, product_pack_spec, product_publish_spec,
    workspace_root_spec,
};
use crate::{
    ADD_HANDLER_ID, BUILD_HANDLER_ID, CREATE_HANDLER_ID, DEV_HANDLER_ID,
    DOCTOR_PRODUCT_HANDLER_ID, HANDLER_ID, INSPECT_PRODUCT_HANDLER_ID, LINT_CHECK_HANDLER_ID,
    LINT_FIX_HANDLER_ID, LINT_HANDLER_ID, PACK_PRODUCT_HANDLER_ID, PRODUCT_BUILD_HANDLER_ID,
    PRODUCT_DEV_HANDLER_ID, PRODUCT_ROOT_HANDLER_ID, PUBLISH_PRODUCT_HANDLER_ID,
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
            PRODUCT_DEV_HANDLER_ID,
            Box::new(ProductDevCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            PRODUCT_ROOT_HANDLER_ID,
            Box::new(lania_host::ProductRootCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            BUILD_HANDLER_ID,
            Box::new(BuildCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            PRODUCT_BUILD_HANDLER_ID,
            Box::new(ProductBuildCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            PACK_PRODUCT_HANDLER_ID,
            Box::new(PackProductCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            PUBLISH_PRODUCT_HANDLER_ID,
            Box::new(PublishProductCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            INSPECT_PRODUCT_HANDLER_ID,
            Box::new(InspectProductCommandHandler) as Box<dyn CommandHandler>,
        ),
        (
            DOCTOR_PRODUCT_HANDLER_ID,
            Box::new(DoctorProductCommandHandler) as Box<dyn CommandHandler>,
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

    // product 根命令：幂等注册（独立二进制形态没有其它包预先注册 root，缺失时补齐）。
    lania_host::ensure_product_root(ctx)?;

    // 5 个原顶级命令降级为 `lan workspace <sub>` 子命令；template（原 create 的
    // template 子命令）上移到 workspace 根。
    ctx.commands.mount_subcommand("workspace", dev_spec())?;
    ctx.commands.mount_subcommand("workspace", build_spec())?;
    ctx.commands.mount_subcommand("workspace", lint_spec())?;
    ctx.commands.mount_subcommand("workspace", create_spec())?;
    ctx.commands.mount_subcommand("workspace", add_spec())?;
    ctx.commands
        .mount_subcommand("workspace", TemplateCommandSpec::spec())?;

    // product 子树：仍挂在共享的 `lan product` 根下（命令拼写不变）。
    ctx.commands.mount_subcommand("product", product_dev_spec())?;
    ctx.commands
        .mount_subcommand("product", product_build_spec())?;
    ctx.commands.mount_subcommand("product", product_pack_spec())?;
    ctx.commands
        .mount_subcommand("product", product_publish_spec())?;
    ctx.commands
        .mount_subcommand("product", product_inspect_spec())?;
    ctx.commands
        .mount_subcommand("product", product_doctor_spec())?;
    Ok(())
}
