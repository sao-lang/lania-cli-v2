//! `command-product` 的 dev 部分。
//!
//! `lan product dev` 通过 re-exec 当前 CLI + 注入 env 来实现「开发态 product 命令」，
//! 由 `ProductDevCommandHandler` 承担，并在 `registry.rs` 注册。
//!
//! 历史背景：本文件早期从 workspace 包迁入时还带了一份 `DevCommandHandler` /
//! `build_request` / `resolve_dev_exit_code` 的完整副本（对应标准 `lan workspace dev`
//! 走 node-bridge 的路径），但那份 handler 从未被注册，属于死代码，已删除。
//! 标准 dev 能力的唯一实现在 workspace 命令子包中。

use anyhow::Result;
use async_trait::async_trait;
use lania_host::execution::{CommandExecution, CommandExecutionContext, CommandHandler};

use crate::watch::{resolve_product_dev_options, run_product_dev_once, run_product_dev_watch};

/// `lan product dev` 的 handler：当前进程以 product 环境变量 re-exec CLI 实现开发态。
pub(crate) struct ProductDevCommandHandler;

#[async_trait(?Send)]
impl CommandHandler for ProductDevCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // `lan product dev`：通过 re-exec 当前 CLI + 注入 env 来实现“开发态 product 命令”。
        let options = resolve_product_dev_options(ctx.command(), ctx.locale())?;
        if options.watch {
            return run_product_dev_watch(ctx, options).await;
        }
        run_product_dev_once(ctx, options)
    }
}
