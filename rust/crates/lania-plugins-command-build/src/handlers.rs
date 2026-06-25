//! `command-build` 的 host handler 实现。
//!
//! 设计原则：
//! - 每个 handler 只做三件事：校验 capability -> 构造 request -> 调用 bridge 并包装结果
//! - 复杂参数解析与 request 组装放在 `request.rs`（避免 handler 膨胀）
//! - `inspect`/`doctor` 顶层命令不允许直接执行，返回更友好的引导文案

use anyhow::Result;
use async_trait::async_trait;
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
};

use crate::BuildCommandPlugin;

pub(crate) struct BuildCommandHandler;
pub(crate) struct ProductBuildCommandHandler;
pub(crate) struct PackProductCommandHandler;
pub(crate) struct PublishProductCommandHandler;
pub(crate) struct InspectProductCommandHandler;
pub(crate) struct DoctorProductCommandHandler;

// 每个 handler 都刻意保持很薄：
// - 校验需要的 capability
// - 构造 bridge request
// - 调用 bridge 并把结果包装成 host `CommandExecution`

#[async_trait(?Send)]
impl CommandHandler for BuildCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // 标准 `lan build`：走 node-bridge 的 compiler.build workflow。
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = BuildCommandPlugin::build_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for ProductBuildCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // `lan product build` -> `product.build`（TS 侧实现）。
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = BuildCommandPlugin::build_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for PackProductCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // `lan product pack` -> `product.pack`。
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = BuildCommandPlugin::pack_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for PublishProductCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // `lan product publish` -> `product.publish`。
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = BuildCommandPlugin::publish_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for InspectProductCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = BuildCommandPlugin::inspect_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for DoctorProductCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = BuildCommandPlugin::doctor_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}
