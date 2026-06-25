//! `command-dev` 的注册胶水（commands + handlers）。
//!
//! 这个文件解决的问题：
//! - `DevCommandPlugin::setup()` 需要尽量简短，像“流程描述”而不是一堆注册细节。
//! - 命令 spec 的注册、handler 的注册、以及 hook 事件上报是高频样板代码，集中放置可读性更好。
//!
//! 为什么你在 IDE 里可能看到“灰色”：
//! - 这里的函数是 `pub(crate)`，属于 crate 内部 wiring，不对外暴露很正常。
//! - 是否“灰色”更多取决于 rust-analyzer 的索引状态，不代表不参与编译。
//! - 实际上本文件被 `lib.rs` 通过 `mod registry;` 引入，并在 `setup()` 中调用。

use anyhow::Result;
use lania_host::plugin::{register_builtin_command_handlers, PluginSetupContext};
use lania_host::CommandHandler;

use crate::handlers::{DevCommandHandler, ProductDevCommandHandler, ProductRootCommandHandler};
use crate::{DevCommandPlugin, HANDLER_ID, PRODUCT_HANDLER_ID, PRODUCT_ROOT_HANDLER_ID};

// `command-dev` 的注册胶水（命令 spec + handler）。
//
// 为什么要单独放在这里：
// - `DevCommandPlugin::setup()` 需要像“步骤列表”，不希望被大量 register/mount 细节淹没。
// - “注册命令”“注册 handler”属于高频样板代码，集中放置更便于复用和查找。
//
// 可见性说明：
// - 这里的函数都用 `pub(crate)`：它们是 crate 内部 wiring，不是公共 API。
// - 虽然不对外暴露，但确实会被 `lib.rs` 引用并在 setup 时执行。

pub(crate) fn register_dev_commands(ctx: &mut PluginSetupContext<'_>) -> Result<()> {
    // 注册根命令：
    // - `dev`：标准开发流程（走 node-bridge 的 compiler.dev）
    // - `product`：产品命令分组根（自身不可执行，只承载子命令）
    register_builtin_command_handlers(
        ctx,
        "command-dev",
        "dev",
        DevCommandPlugin::spec(),
        vec![
            (HANDLER_ID, Box::new(DevCommandHandler) as Box<dyn CommandHandler>),
            (
                PRODUCT_ROOT_HANDLER_ID,
                Box::new(ProductRootCommandHandler) as Box<dyn CommandHandler>,
            ),
            (
                PRODUCT_HANDLER_ID,
                Box::new(ProductDevCommandHandler) as Box<dyn CommandHandler>,
            ),
        ],
    )?;
    ctx.commands
        .register(DevCommandPlugin::product_root_spec())?;
    // 把 `dev` 挂到 `lan product dev` 下。
    // 这是 product 开发态命令的唯一入口，不再保留 `lan dev product`。
    // 这样用户在 `lan product ...` 里能集中发现 product 生命周期相关命令。
    ctx.commands
        .mount_subcommand("product", DevCommandPlugin::product_dev_spec())?;
    Ok(())
}
