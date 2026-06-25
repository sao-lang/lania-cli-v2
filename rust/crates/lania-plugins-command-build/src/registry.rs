//! `command-build` 的注册胶水（commands + handlers）。
//!
//! 这类代码的特点：
//! - 逻辑不复杂但非常“啰嗦”
//! - 如果放在 `lib.rs` 会把插件入口淹没
//!
//! 因此这里提供几个小函数，让 `BuildCommandPlugin::setup()` 更像“流程描述”。

use anyhow::Result;
use lania_host::{
    plugin::{register_builtin_command_handlers, PluginSetupContext},
    CommandHandler,
};

use crate::handlers::{
    BuildCommandHandler, DoctorProductCommandHandler, InspectProductCommandHandler,
    PackProductCommandHandler, ProductBuildCommandHandler, PublishProductCommandHandler,
};
use crate::{
    BuildCommandPlugin, DOCTOR_PRODUCT_HANDLER_ID, HANDLER_ID, INSPECT_PRODUCT_HANDLER_ID,
    PACK_PRODUCT_HANDLER_ID, PRODUCT_HANDLER_ID, PUBLISH_PRODUCT_HANDLER_ID,
};

// 把注册胶水从 handler/spec 中分离出来：
// - 让 `BuildCommandPlugin::setup()` 更像一段“做什么”的描述
// - 具体怎么注册/怎么 mount 的细节集中在这里

pub(crate) fn register_build_commands(ctx: &mut PluginSetupContext<'_>) -> Result<()> {
    // 注册基础 `build` 命令及其 handler，并统一记录内建命令注册事件。
    register_builtin_command_handlers(
        ctx,
        "command-build",
        "build",
        BuildCommandPlugin::spec(),
        vec![
            (HANDLER_ID, Box::new(BuildCommandHandler) as Box<dyn CommandHandler>),
            (
                PRODUCT_HANDLER_ID,
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
        ],
    )?;
    // 所有 product 相关命令统一挂到 `lan product ...` 下。
    ctx.commands
        .mount_subcommand("product", BuildCommandPlugin::product_build_spec())?;
    ctx.commands
        .mount_subcommand("product", BuildCommandPlugin::product_pack_spec())?;
    ctx.commands
        .mount_subcommand("product", BuildCommandPlugin::product_publish_spec())?;
    ctx.commands
        .mount_subcommand("product", BuildCommandPlugin::product_inspect_spec())?;
    ctx.commands
        .mount_subcommand("product", BuildCommandPlugin::product_doctor_spec())?;
    Ok(())
}
