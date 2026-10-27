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
    RuntimeKind,
};
use lania_logger::LogLevel;

use crate::BuildCommandPlugin;

pub(crate) struct BuildCommandHandler;
pub(crate) struct ProductBuildCommandHandler;

/// 非 web runtime 的 adapter 会在 result 里带 `success` 字段。
/// 缺失该字段视为成功（保持 web 既有行为）。
pub(crate) fn resolve_build_exit_code(result: Option<&serde_json::Value>) -> i32 {
    let success = result
        .and_then(|value| value.get("success"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if success {
        lania_host::EXIT_SUCCESS
    } else {
        lania_host::EXIT_RUNTIME_ERROR
    }
}
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
        // §5.1 验收：双运行时项目要告诉用户我们选了哪个。
        let cli_runtime = ctx
            .command()
            .argv
            .options
            .get("runtime")
            .and_then(|value| value.as_str());
        ctx.log_runtime_decision(cli_runtime);
        // B9 / B10：非 JS 项目先尝试该运行时自己的 bridge（去掉 Node 中转），
        // 进程链从 `Rust → node → worker → go build` 降到 `Rust → go bridge → go build`。
        // 任何一步不可用时回落原来的 JS 簇路径（§5.3）。
        if let Some(execution) = run_runtime_build(ctx).await? {
            return Ok(execution);
        }
        let request = BuildCommandPlugin::build_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        let exit_code = resolve_build_exit_code(run.exchange.response.result.as_ref());
        Ok(ctx.complete_bridge(run, exit_code))
    }
}

/// 非 JS 项目的 build：交给该运行时自己的 bridge（§3.3 / §3.4 / §3.6）。
///
/// 返回 `Ok(None)` 表示「JS 项目 / watch 模式 / bridge 不可用 / 调用失败」，
/// 调用方继续走 node bridge —— 这就是 §5.3 要求的回落路径。
async fn run_runtime_build(ctx: &CommandExecutionContext<'_>) -> Result<Option<CommandExecution>> {
    // watch 需要长驻订阅与增量编译，语言 bridge 暂不承担（见 §4.2 的 compiler-worker 保留理由）。
    if ctx.bool_option("watch") {
        return Ok(None);
    }
    let cwd = ctx.command().cwd.clone();
    if cwd.is_empty() {
        return Ok(None);
    }
    let cli_runtime = ctx
        .command()
        .argv
        .options
        .get("runtime")
        .and_then(|value| value.as_str());
    let kind = lania_host::detect_runtime(
        std::path::Path::new(&cwd),
        ctx.project_config(),
        cli_runtime,
    );
    // JS 簇本来就该走 node bridge；其余非 JS 运行时都交给各自的 bridge。
    if !matches!(
        kind,
        RuntimeKind::Python | RuntimeKind::Go | RuntimeKind::Dart | RuntimeKind::Rust
    ) {
        return Ok(None);
    }
    if !ctx.has_bridge(kind) {
        return Ok(None);
    }
    let output_dir = ctx
        .command()
        .argv
        .options
        .get("output-dir")
        .and_then(|value| value.as_str())
        .unwrap_or("bin/app")
        .to_string();
    // 命令来自「配置覆盖 > toolchain.describe」（§5.2 单点）；拿不到就不路由。
    let Some(command) = ctx
        .resolve_runtime_command(kind, "build", Some(&output_dir))
        .await
    else {
        return Ok(None);
    };
    // 延迟解析的 bridge（Go 首次使用会编译一次）也可能在这里失败。
    let Ok(client) = ctx.bridge(kind) else {
        return Ok(None);
    };

    let request = client.request(
        format!("build.{}", kind.id()),
        serde_json::json!({
            "cwd": cwd,
            "outputDir": output_dir,
            "command": command,
            "watch": false,
            "mode": ctx.command().argv.options.get("mode").and_then(|value| value.as_str()),
        }),
    );
    // 保留一份原始请求，输出层会把它连同结果一起序列化。
    let executed = request.clone();
    match ctx.call_runtime_bridge(&client, request).await {
        Ok(exchange) => {
            let result = exchange.response.result.clone();
            let exit_code = resolve_build_exit_code(result.as_ref());
            Ok(Some(
                ctx.complete_runtime_bridge(executed, exchange, exit_code),
            ))
        }
        Err(error) => {
            ctx.workflow_services().logger.log(
                LogLevel::Warn,
                format!(
                    "build via {} failed, falling back to the JS cluster: {error}",
                    kind.id()
                ),
            );
            Ok(None)
        }
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
