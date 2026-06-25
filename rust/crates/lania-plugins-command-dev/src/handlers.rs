//! `command-dev` 的 host handler 实现。
//!
//! 设计目标：
//! - handler 自身尽量“薄”，只负责：校验 capability -> 组装请求/参数 -> 执行 -> 包装输出
//! - 复杂逻辑拆到更合适的文件：
//!   - `request.rs`：标准 dev workflow 的 bridge request 映射
//!   - `watch.rs`：product dev 的 once/watch 运行时与文件变化检测
//!
//! 这样拆分后，当你在排查 bug 时可以很快定位：
//! - “命令长什么样”看 `spec.rs`
//! - “发给 bridge 的参数是什么”看 `request.rs`
//! - “watch 为什么会重启”看 `watch.rs`

use anyhow::{bail, Result};
use async_trait::async_trait;
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
};

use crate::watch::{resolve_product_dev_options, run_product_dev_once, run_product_dev_watch};
use crate::DevCommandPlugin;

// `command-dev` 的 host handler（保留“薄层”原则）。
//
// handler 只负责：
// - 校验需要的 capability（例如 NodeBridge）
// - 组装 bridge request / product-dev options
// - 执行并把结果包装成 `CommandExecution`

pub(crate) struct DevCommandHandler;
pub(crate) struct ProductDevCommandHandler;
pub(crate) struct ProductRootCommandHandler;

#[async_trait(?Send)]
impl CommandHandler for DevCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // 标准 `lan dev`：走 node-bridge 的 compiler.dev workflow。
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = DevCommandPlugin::build_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

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

#[async_trait(?Send)]
impl CommandHandler for ProductRootCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // `lan product` 根命令只是分组；不带子命令时直接提示正确用法。
        bail!(
            "{}",
            if ctx.locale() == "zh" {
                "缺少 product 子命令，请使用 `lan product <generate|dev|build|pack|publish|inspect|doctor>`"
            } else {
                "missing product subcommand, try `lan product <generate|dev|build|pack|publish|inspect|doctor>`"
            }
        );
    }
}
