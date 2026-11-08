//! `command-product` 的 build 部分（从 workspace 包迁入；
//! `BuildCommandHandler` 为完整副本，仅 Product* 五个 handler 被注册使用）。
//!
//! 内容：
//! - `BuildCommandHandler`：标准 `lan workspace build`（compiler.build workflow）
//! - product build/pack/publish/inspect/doctor 五个 handler（`lan product ...`）
//! - bridge request 构建（原 request.rs）：compiler.build 与 product.* 方法
//!
//! 设计原则：
//! - 每个 handler 只做三件事：校验 capability -> 构造 request -> 调用 bridge 并包装结果
//! - `inspect`/`doctor` 顶层命令不允许直接执行，返回更友好的引导文案

use anyhow::Result;
use async_trait::async_trait;
use lania_command::CommandContext;
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
    RuntimeKind,
};
use lania_logger::LogLevel;
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};
use serde_json::{Map, Value};
use std::path::PathBuf;

pub(crate) struct BuildCommandHandler;
pub(crate) struct ProductBuildCommandHandler;
pub(crate) struct PackProductCommandHandler;
pub(crate) struct PublishProductCommandHandler;
pub(crate) struct InspectProductCommandHandler;
pub(crate) struct DoctorProductCommandHandler;

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

// 每个 handler 都刻意保持很薄：
// - 校验需要的 capability
// - 构造 bridge request
// - 调用 bridge 并把结果包装成 host `CommandExecution`

#[async_trait(?Send)]
impl CommandHandler for BuildCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // 标准 `lan workspace build`：走 node-bridge 的 compiler.build workflow。
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
        let request = build_request(ctx.command(), ctx.node_bridge());
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
        let request = build_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for PackProductCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // `lan product pack` -> `product.pack`。
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = pack_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for PublishProductCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // `lan product publish` -> `product.publish`。
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = publish_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for InspectProductCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = inspect_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

#[async_trait(?Send)]
impl CommandHandler for DoctorProductCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let request = doctor_product_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
    }
}

// ─────────────────────────────────────────────────────────────
// bridge request 构建（原 request.rs 迁入）
//
// 目标：
// - 把 CLI 的 argv/options 转成 node-bridge 能理解的 `method + params`
// - 统一处理 cwd/path 兼容规则（legacy 选项）
// - 保持 product distribution 的参数命名与 JS 侧实现一致（camelCase 等）
// ─────────────────────────────────────────────────────────────

pub(crate) fn build_request(context: &CommandContext, bridge: &NodeBridgeClient) -> BridgeRequest {
    // Legacy 兼容字段：
    // - `config` / `path` 可能被某些旧 bridge 使用（或仅作为 hint），即使新实现忽略也需要保留。
    let config_path = string_option_value(context, "config");
    let path = string_option_value(context, "path");
    let cwd = resolve_cwd_from_path_option(&context.cwd, path.as_deref());
    let watch = bool_option_value(context, "watch", false);
    let mode = string_option_value(context, "mode");
    let output_dir = string_option_value(context, "output-dir");

    let mut request = bridge.compiler_build_with_options_request(cwd, watch, mode, output_dir);
    let runtime = string_option_value(context, "runtime");
    request.params["config"] = serde_json::json!(config_path);
    request.params["path"] = serde_json::json!(path);
    request.params["runtime"] = serde_json::json!(runtime);
    request
}

pub(crate) fn build_product_request(
    context: &CommandContext,
    bridge: &NodeBridgeClient,
) -> BridgeRequest {
    product_distribution_request(
        bridge,
        "product.build",
        context,
        vec![(
            "outputDir",
            Value::String(
                string_option_value(context, "output-dir")
                    .unwrap_or_else(|| ".lania/build/product".into()),
            ),
        )],
    )
}

pub(crate) fn pack_product_request(
    context: &CommandContext,
    bridge: &NodeBridgeClient,
) -> BridgeRequest {
    product_distribution_request(
        bridge,
        "product.pack",
        context,
        vec![
            (
                "buildDir",
                Value::String(
                    string_option_value(context, "build-dir")
                        .unwrap_or_else(|| ".lania/build/product".into()),
                ),
            ),
            (
                "outputDir",
                Value::String(
                    string_option_value(context, "output-dir")
                        .unwrap_or_else(|| ".lania/pack/product/install-root".into()),
                ),
            ),
        ],
    )
}

pub(crate) fn publish_product_request(
    context: &CommandContext,
    bridge: &NodeBridgeClient,
) -> BridgeRequest {
    // publish 的 params 相对复杂：它既包含“产物目录参数”，也包含“执行策略参数”。
    // 注意：这里的 key 必须和 TS 侧 `product.publish` 处理器约定一致（例如 `npmBin`）。
    let mut extra = vec![
        (
            "packDir",
            Value::String(
                string_option_value(context, "pack-dir")
                    .unwrap_or_else(|| ".lania/pack/product/install-root".into()),
            ),
        ),
        (
            "outputDir",
            Value::String(
                string_option_value(context, "output-dir")
                    .unwrap_or_else(|| ".lania/publish/product/npm-package".into()),
            ),
        ),
    ];
    if let Some(dist_tag) = string_option_value(context, "dist-tag") {
        extra.push(("distTag", Value::String(dist_tag)));
    }
    if let Some(channel) = string_option_value(context, "channel") {
        extra.push(("channel", Value::String(channel)));
    }
    if let Some(registry) = string_option_value(context, "registry") {
        extra.push(("registry", Value::String(registry)));
    }
    if let Some(platform_binaries_dir) = string_option_value(context, "platform-binaries-dir") {
        extra.push(("platformBinariesDir", Value::String(platform_binaries_dir)));
    }
    if let Some(platform_binary_paths) = string_option_value(context, "platform-binary-paths") {
        extra.push(("platformBinaryPaths", Value::String(platform_binary_paths)));
    }
    if bool_option_value(context, "execute", false) {
        extra.push(("execute", Value::Bool(true)));
    }
    if bool_option_value(context, "dry-run", false) {
        extra.push(("dryRun", Value::Bool(true)));
    }
    if bool_option_value(context, "yes", false) {
        extra.push(("yes", Value::Bool(true)));
    }
    if bool_option_value(context, "resume", false) {
        extra.push(("resume", Value::Bool(true)));
    }
    if let Some(otp) = string_option_value(context, "otp") {
        extra.push(("otp", Value::String(otp)));
    }
    if let Some(npm_bin) = string_option_value(context, "npm-bin") {
        extra.push(("npmBin", Value::String(npm_bin)));
    }
    if let Some(max_retries) = string_option_value(context, "max-retries") {
        extra.push(("maxRetries", Value::String(max_retries)));
    }
    if let Some(retry_delay_ms) = string_option_value(context, "retry-delay-ms") {
        extra.push(("retryDelayMs", Value::String(retry_delay_ms)));
    }
    if bool_option_value(context, "rollback-on-failure", false) {
        extra.push(("rollbackOnFailure", Value::Bool(true)));
    }
    product_distribution_request(bridge, "product.publish", context, extra)
}

pub(crate) fn inspect_product_request(
    context: &CommandContext,
    bridge: &NodeBridgeClient,
) -> BridgeRequest {
    inspect_like_product_request(context, bridge, false)
}

pub(crate) fn doctor_product_request(
    context: &CommandContext,
    bridge: &NodeBridgeClient,
) -> BridgeRequest {
    inspect_like_product_request(context, bridge, true)
}

fn inspect_like_product_request(
    context: &CommandContext,
    bridge: &NodeBridgeClient,
    doctor: bool,
) -> BridgeRequest {
    let mut extra = vec![("hostVersion", Value::String(env!("CARGO_PKG_VERSION").to_string()))];
    if bool_option_value(context, "compat", false) {
        extra.push(("compat", Value::Bool(true)));
    }
    if doctor {
        extra.push(("doctor", Value::Bool(true)));
        extra.push(("compat", Value::Bool(true)));
    }
    product_distribution_request(bridge, "product.inspect", context, extra)
}

fn string_option_value(context: &CommandContext, key: &str) -> Option<String> {
    context
        .argv
        .options
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn bool_option_value(context: &CommandContext, key: &str, default: bool) -> bool {
    context
        .argv
        .options
        .get(key)
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

fn product_distribution_request(
    bridge: &NodeBridgeClient,
    method: &str,
    context: &CommandContext,
    extra: Vec<(&str, Value)>,
) -> BridgeRequest {
    // 所有 product.* 方法都接受：
    // - cwd: 绝对/基于当前 cwd 的路径
    // - path: 原始参数（可空），用于调试/兼容
    // - clean: 清理开关（默认 true）
    let path = string_option_value(context, "path");
    let cwd = resolve_cwd_from_path_option(&context.cwd, path.as_deref());
    let mut params = Map::new();
    params.insert("cwd".into(), Value::String(cwd));
    params.insert("path".into(), serde_json::json!(path));
    params.insert(
        "clean".into(),
        Value::Bool(bool_option_value(context, "clean", true)),
    );
    for (key, value) in extra {
        params.insert(key.into(), value);
    }
    bridge.request(method, Value::Object(params))
}

fn resolve_cwd_from_path_option(base_cwd: &str, raw_path: Option<&str>) -> String {
    // cwd 解析规则保持和旧 CLI 一致：
    // - 传了 `--path` 时：相对路径基于 base_cwd 拼接；绝对路径直接使用
    // - 未传 `--path` 时：使用 base_cwd
    match raw_path.map(str::trim) {
        Some(raw) if !raw.is_empty() => {
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                path.display().to_string()
            } else {
                PathBuf::from(base_cwd).join(raw).display().to_string()
            }
        }
        _ => base_cwd.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bool_option_value, build_product_request, build_request, doctor_product_request,
        inspect_product_request, pack_product_request, publish_product_request,
        resolve_build_exit_code,
    };
    use lania_command::CommandContext;
    use lania_node_bridge::{BridgeClientConfig, NodeBridgeClient};
    use serde_json::json;

    fn joined_cwd(path: &str) -> String {
        std::path::Path::new("/repo")
            .join(path)
            .display()
            .to_string()
    }

    #[test]
    fn builds_build_bridge_request_from_context() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("watch".into(), json!(true)),
                    ("path".into(), json!("apps/demo")),
                    ("mode".into(), json!("development")),
                    ("output-dir".into(), json!("dist-custom")),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.build".into(),
            trace_id: "trace-2".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = build_request(&context, &bridge);

        assert_eq!(request.method, "compiler.build");
        assert_eq!(request.params["cwd"], joined_cwd("apps/demo"));
        assert_eq!(request.params["watch"], true);
        assert_eq!(request.params["mode"], "development");
        assert_eq!(request.params["outputDir"], "dist-custom");
    }

    #[test]
    fn build_request_forwards_runtime() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("runtime".into(), json!("rust"))].into_iter().collect(),
            },
            handler_id: "command.build".into(),
            trace_id: "trace-runtime".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = build_request(&context, &bridge);

        assert_eq!(request.params["runtime"], json!("rust"));
    }

    #[test]
    fn build_exit_code_maps_failure_to_runtime_error() {
        use lania_host::{EXIT_RUNTIME_ERROR, EXIT_SUCCESS};

        assert_eq!(
            resolve_build_exit_code(Some(&json!({ "success": false }))),
            EXIT_RUNTIME_ERROR
        );
        assert_eq!(
            resolve_build_exit_code(Some(&json!({ "success": true }))),
            EXIT_SUCCESS
        );
        assert_eq!(resolve_build_exit_code(Some(&json!({}))), EXIT_SUCCESS);
        assert_eq!(resolve_build_exit_code(None), EXIT_SUCCESS);
    }

    #[test]
    fn builds_product_build_bridge_request_from_context() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("path".into(), json!("products/acme")),
                    ("output-dir".into(), json!(".lania/build/product")),
                    ("clean".into(), json!(false)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.build.product".into(),
            trace_id: "trace-3".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = build_product_request(&context, &bridge);

        assert_eq!(request.method, "product.build");
        assert_eq!(request.params["cwd"], joined_cwd("products/acme"));
        assert_eq!(request.params["outputDir"], ".lania/build/product");
        assert_eq!(request.params["clean"], false);
    }

    #[test]
    fn builds_product_pack_bridge_request_from_context() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("path".into(), json!("products/acme")),
                    ("build-dir".into(), json!(".lania/build/product")),
                    (
                        "output-dir".into(),
                        json!(".lania/pack/product/install-root"),
                    ),
                    ("clean".into(), json!(false)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.pack.product".into(),
            trace_id: "trace-4".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = pack_product_request(&context, &bridge);

        assert_eq!(request.method, "product.pack");
        assert_eq!(request.params["cwd"], joined_cwd("products/acme"));
        assert_eq!(request.params["buildDir"], ".lania/build/product");
        assert_eq!(
            request.params["outputDir"],
            ".lania/pack/product/install-root"
        );
        assert_eq!(request.params["clean"], false);
    }

    #[test]
    fn builds_product_publish_bridge_request_from_context() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("path".into(), json!("products/acme")),
                    ("pack-dir".into(), json!(".lania/pack/product/install-root")),
                    (
                        "output-dir".into(),
                        json!(".lania/publish/product/npm-package"),
                    ),
                    ("dist-tag".into(), json!("next")),
                    ("channel".into(), json!("beta")),
                    ("registry".into(), json!("http://localhost:4873")),
                    (
                        "platform-binaries-dir".into(),
                        json!("/tmp/lania-cli-platforms"),
                    ),
                    (
                        "platform-binary-paths".into(),
                        json!(r#"{"linux-x64":"/tmp/lania-cli-linux-x64"}"#),
                    ),
                    ("execute".into(), json!(true)),
                    ("dry-run".into(), json!(true)),
                    ("yes".into(), json!(true)),
                    ("resume".into(), json!(true)),
                    ("otp".into(), json!("123456")),
                    ("npm-bin".into(), json!("/tmp/fake-npm")),
                    ("max-retries".into(), json!("3")),
                    ("retry-delay-ms".into(), json!("1500")),
                    ("rollback-on-failure".into(), json!(true)),
                    ("clean".into(), json!(false)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.publish.product".into(),
            trace_id: "trace-5".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = publish_product_request(&context, &bridge);

        assert_eq!(request.method, "product.publish");
        assert_eq!(request.params["cwd"], joined_cwd("products/acme"));
        assert_eq!(
            request.params["packDir"],
            ".lania/pack/product/install-root"
        );
        assert_eq!(
            request.params["outputDir"],
            ".lania/publish/product/npm-package"
        );
        assert_eq!(request.params["distTag"], "next");
        assert_eq!(request.params["channel"], "beta");
        assert_eq!(request.params["registry"], "http://localhost:4873");
        assert_eq!(
            request.params["platformBinariesDir"],
            "/tmp/lania-cli-platforms"
        );
        assert_eq!(
            request.params["platformBinaryPaths"],
            r#"{"linux-x64":"/tmp/lania-cli-linux-x64"}"#
        );
        assert_eq!(request.params["execute"], true);
        assert_eq!(request.params["dryRun"], true);
        assert_eq!(request.params["yes"], true);
        assert_eq!(request.params["resume"], true);
        assert_eq!(request.params["otp"], "123456");
        assert_eq!(request.params["npmBin"], "/tmp/fake-npm");
        assert_eq!(request.params["maxRetries"], "3");
        assert_eq!(request.params["retryDelayMs"], "1500");
        assert_eq!(request.params["rollbackOnFailure"], true);
        assert_eq!(request.params["clean"], false);
    }

    #[test]
    fn builds_product_inspect_bridge_request_from_context() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("path".into(), json!("products/acme")),
                    ("compat".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.inspect.product".into(),
            trace_id: "trace-6".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = inspect_product_request(&context, &bridge);

        assert_eq!(request.method, "product.inspect");
        assert_eq!(request.params["cwd"], joined_cwd("products/acme"));
        assert_eq!(request.params["clean"], true);
        assert_eq!(request.params["compat"], true);
        assert_eq!(request.params["hostVersion"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn builds_product_doctor_bridge_request_from_context() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("path".into(), json!("products/acme"))]
                    .into_iter()
                    .collect(),
            },
            handler_id: "command.doctor.product".into(),
            trace_id: "trace-7".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = doctor_product_request(&context, &bridge);

        assert_eq!(request.method, "product.inspect");
        assert_eq!(request.params["cwd"], joined_cwd("products/acme"));
        assert_eq!(request.params["clean"], true);
        assert_eq!(request.params["doctor"], true);
        assert_eq!(request.params["compat"], true);
        assert_eq!(request.params["hostVersion"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn bool_option_value_defaults_when_missing_or_not_bool() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv::default(),
            handler_id: "command.build".into(),
            trace_id: "trace-bool".into(),
        };
        assert!(bool_option_value(&context, "watch", true));
        assert!(!bool_option_value(&context, "watch", false));
    }
}
