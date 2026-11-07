//! `lan product` 根命令的共享定义（`dev` / `build` 等命令子包共同消费）。
//!
//! product 生命周期命令分布在多个子包：`dev` 注册 root 并挂 `product dev`，
//! `build` 挂 `product build/pack/publish/inspect/doctor`。独立二进制形态
//! （`lan-build` 等）没有 dev 包预先注册 root，`mount_subcommand("product", …)`
//! 会因父命令缺失而失败。[`ensure_product_root`] 在缺失时补齐 root；
//! 总包/单测形态已注册时是 no-op——spec 与 handler 只写一份。

use anyhow::{bail, Result};
use async_trait::async_trait;
use lania_command::{CommandSpec, Example};

use crate::execution::{CommandExecution, CommandExecutionContext, CommandHandler};
use crate::plugin::PluginSetupContext;

/// product 根命令的 handler id（与 dev 包历史值保持一致，冻结导出面）。
pub const PRODUCT_ROOT_HANDLER_ID: &str = "command.product";

/// product 根命令 spec（内容与 dev 包原实现逐字段一致）。
pub fn product_root_spec() -> CommandSpec {
    // `product` 根命令：用来分组 product 生命周期命令。
    // 它本身不可执行；缺少子命令时由 [`ProductRootCommandHandler`] 给出提示。
    CommandSpec::new(
        "product",
        "Product-oriented CLI workflows and distribution commands",
        PRODUCT_ROOT_HANDLER_ID,
    )
    .with_examples(vec![
        Example {
            command: "lan product dev hello --path ./products/acme-cli".into(),
            description: "Run a local product command in development mode".into(),
        },
        Example {
            command: "lan product inspect --path ./products/acme-cli --compat".into(),
            description: "Inspect product compatibility and local distribution state".into(),
        },
    ])
}

/// `lan product`（bare）：只是分组；不带子命令时直接提示正确用法。
#[derive(Debug, Default)]
pub struct ProductRootCommandHandler;

#[async_trait(?Send)]
impl CommandHandler for ProductRootCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        bail!(
            "{}",
            if ctx.locale() == "zh" {
                "缺少 product 子命令，请使用 `lan product <dev|build|pack|publish|inspect|doctor>`"
            } else {
                "missing product subcommand, try `lan product <dev|build|pack|publish|inspect|doctor>`"
            }
        );
    }
}

/// 幂等注册 product 根命令（spec + handler）：缺失时补齐，已存在时不动。
pub fn ensure_product_root(ctx: &mut PluginSetupContext<'_>) -> Result<()> {
    let exists = ctx
        .commands
        .commands()
        .iter()
        .any(|spec| spec.name == "product");
    if exists {
        return Ok(());
    }
    ctx.commands.register(product_root_spec())?;
    ctx.handlers.register(
        PRODUCT_ROOT_HANDLER_ID,
        Box::new(ProductRootCommandHandler) as Box<dyn CommandHandler>,
    )?;
    Ok(())
}
