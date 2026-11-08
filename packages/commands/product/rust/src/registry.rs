//! `command-product` 的注册胶水（commands + handlers）。

use anyhow::Result;
use lania_host::{
    plugin::{register_builtin_command_handlers, PluginSetupContext},
    CommandHandler,
};

use crate::build::{
    DoctorProductCommandHandler, InspectProductCommandHandler, PackProductCommandHandler,
    ProductBuildCommandHandler, PublishProductCommandHandler,
};
use crate::dev::ProductDevCommandHandler;
use lania_host::product_root_spec;
use crate::{
    DOCTOR_PRODUCT_HANDLER_ID, HANDLER_ID, INSPECT_PRODUCT_HANDLER_ID,
    PACK_PRODUCT_HANDLER_ID, PRODUCT_BUILD_HANDLER_ID, PRODUCT_DEV_HANDLER_ID,
    PUBLISH_PRODUCT_HANDLER_ID,
};

pub(crate) fn register_product_commands(ctx: &mut PluginSetupContext<'_>) -> Result<()> {
    let handlers: Vec<(&str, Box<dyn CommandHandler>)> = vec![
        (HANDLER_ID, Box::new(lania_host::ProductRootCommandHandler) as Box<dyn CommandHandler>),
        (PRODUCT_DEV_HANDLER_ID, Box::new(crate::dev::ProductDevCommandHandler) as Box<dyn CommandHandler>),
        (PRODUCT_BUILD_HANDLER_ID, Box::new(ProductBuildCommandHandler) as Box<dyn CommandHandler>),
        (PACK_PRODUCT_HANDLER_ID, Box::new(PackProductCommandHandler) as Box<dyn CommandHandler>),
        (PUBLISH_PRODUCT_HANDLER_ID, Box::new(PublishProductCommandHandler) as Box<dyn CommandHandler>),
        (INSPECT_PRODUCT_HANDLER_ID, Box::new(InspectProductCommandHandler) as Box<dyn CommandHandler>),
        (DOCTOR_PRODUCT_HANDLER_ID, Box::new(DoctorProductCommandHandler) as Box<dyn CommandHandler>),
    ];
    register_builtin_command_handlers(
        ctx,
        "command-product",
        crate::COMMAND_NAME,
        product_root_spec(),
        handlers,
    )?;
    Ok(())
}
