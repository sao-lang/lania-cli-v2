//! 将 `lint` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 主要导出：spec、build_request、LintCommandPlugin、HANDLER_ID、CHECK_HANDLER_ID、FIX_HANDLER_ID。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use anyhow::Result;
use async_trait::async_trait;
use lania_command::{CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
    RuntimeKind,
};
use lania_logger::LogLevel;
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};
use serde_json::Value;

pub const HANDLER_ID: &str = "command.lint";
pub const CHECK_HANDLER_ID: &str = "command.lint.check";
pub const FIX_HANDLER_ID: &str = "command.lint.fix";

/// 「lint adaptor → 独立语言 bridge」的唯一映射。
///
/// B8 先落地了 Python（`lan-py-bridge`），B9 / B10 / B11 扩到 Dart / Go / Rust。
/// 新增一个语言 bridge 的 lint 工具时**只改这里**：
/// 工具的参数表仍在 `toolchain.ts`（TS 侧唯一来源），两者职责不同 ——
/// 这里只回答「这个 adaptor 归哪个进程」。
const RUNTIME_LINT_TOOLS: &[(&str, RuntimeKind)] = &[
    // Python（§3.3）：mypy / flake8 / black 有真 API，ruff 是二进制分发壳。
    ("mypy", RuntimeKind::Python),
    ("ruff", RuntimeKind::Python),
    ("black", RuntimeKind::Python),
    ("flake8", RuntimeKind::Python),
    // Go（§3.4）：go vet 随工具链自带，golangci-lint 需要单独安装。
    ("go-vet", RuntimeKind::Go),
    ("golangci-lint", RuntimeKind::Go),
    // Dart（§3.6）：`dart analyze --format machine`。
    ("dart-analyze", RuntimeKind::Dart),
    // Rust（§3.5 / B11）：由 `lan` 自身的 bridge 模式委托 `cargo clippy`。
    ("clippy", RuntimeKind::Rust),
];

/// 某个运行时由独立 bridge 承担的 adaptor 列表。
fn runtime_lint_tools(kind: RuntimeKind) -> Vec<&'static str> {
    RUNTIME_LINT_TOOLS
        .iter()
        .filter(|(_, owner)| *owner == kind)
        .map(|(adaptor, _)| *adaptor)
        .collect()
}

/// 该运行时的默认 lint 工具（没显式配置时用哪一个）。
///
/// 选「随工具链自带」的那个：`go vet` 跟着 `go`、`dart analyze` 跟着 `dart`、
/// `clippy` 跟着 `cargo`，这样新项目 `lan lint` 不会立刻因为「没装第三方 linter」
/// 而变成噪声。
fn default_lint_tool(kind: RuntimeKind) -> Option<&'static str> {
    match kind {
        RuntimeKind::Python => Some("mypy"),
        RuntimeKind::Go => Some("go-vet"),
        RuntimeKind::Dart => Some("dart-analyze"),
        RuntimeKind::Rust => Some("clippy"),
        _ => None,
    }
}

fn runtime_bridge_label(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Python => "lan-py-bridge",
        RuntimeKind::Go => "lan-go-bridge",
        RuntimeKind::Dart => "lan-dart-bridge",
        RuntimeKind::Rust => "lan-rs-bridge",
        _ => "unknown",
    }
}

/// 该运行时是否已落地独立 lint bridge。
fn has_lint_bridge(kind: RuntimeKind) -> bool {
    !runtime_lint_tools(kind).is_empty()
}

#[derive(Debug, Default)]
pub struct LintCommandPlugin;

struct LintCommandHandler;

impl LintCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new("lint", "Run project lint checks", HANDLER_ID)
            .with_options(Self::shared_options())
            .with_subcommands(vec![Self::check_spec(), Self::fix_spec()])
            .with_examples(vec![
                Example {
                    command: "lan lint".into(),
                    description: "Run lint checks".into(),
                },
                Example {
                    command: "lan lint fix --concurrency 2".into(),
                    description: "Run lint in fix mode with a custom worker limit".into(),
                },
            ])
    }

    fn shared_options() -> Vec<OptionSpec> {
        vec![
            OptionSpec {
                long: "fix".into(),
                short: Some('f'),
                help: "Run lint in fix mode".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            // Legacy compatibility: allow selecting adaptors by name (comma-separated).
            OptionSpec {
                long: "linters".into(),
                short: None,
                help: "Comma-separated linter adaptors to run (eslint,oxlint,prettier,oxfmt,stylelint,textlint,ruff,black,mypy,flake8,golangci-lint,go-vet,clippy,dart-analyze)".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "concurrency".into(),
                short: Some('c'),
                help: "Run lint adaptors with a bounded concurrency level".into(),
                value_kind: ValueKind::Number,
                default_value: Some("4".into()),
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "grouped-output".into(),
                short: None,
                help: "Print lint results grouped by adaptor".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
        ]
    }

    fn check_spec() -> CommandSpec {
        CommandSpec::new("check", "Run lint in check mode", CHECK_HANDLER_ID)
            .with_options(Self::shared_options())
    }

    fn fix_spec() -> CommandSpec {
        CommandSpec::new("fix", "Run lint in fix mode", FIX_HANDLER_ID)
            .with_options(Self::shared_options())
    }

    pub fn build_request(context: &CommandContext, bridge: &NodeBridgeClient) -> BridgeRequest {
        let requested_fix = context
            .argv
            .options
            .get("fix")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let linters = context
            .argv
            .options
            .get("linters")
            .and_then(|value| value.as_str())
            .map(|value| {
                value
                    .split(',')
                    .map(|item| item.trim())
                    .filter(|item| {
                        matches!(
                            *item,
                            "oxlint"
                                | "eslint"
                                | "oxfmt"
                                | "prettier"
                                | "stylelint"
                                | "textlint"
                                | "ruff"
                                | "black"
                                | "mypy"
                                | "flake8"
                                | "golangci-lint"
                                | "go-vet"
                                | "clippy"
                                | "dart-analyze"
                                | "deno-lint"
                                | "deno-fmt"
                        )
                    })
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mode = match context.handler_id.as_str() {
            FIX_HANDLER_ID => "fix",
            CHECK_HANDLER_ID => "check",
            _ if requested_fix => "fix",
            _ => "check",
        };
        let fix = mode == "fix";
        let concurrency = context
            .argv
            .options
            .get("concurrency")
            .and_then(|value| value.as_u64())
            .and_then(|value| usize::try_from(value).ok());
        let grouped_output = context
            .argv
            .options
            .get("grouped-output")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let mut request = bridge.lint_run_request(context.cwd.clone(), fix, concurrency);
        request.params["mode"] = serde_json::json!(mode);
        request.params["groupedOutput"] = serde_json::json!(grouped_output);
        if !linters.is_empty() {
            request.params["linters"] = serde_json::json!(linters);
        }
        request
    }
}

impl Plugin for LintCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-lint".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Logger,
                CapabilityName::NodeBridge,
                CapabilityName::Task,
                CapabilityName::Progress,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<LintCommandHandler, _>(
            ctx,
            "command-lint",
            "lint",
            Self::spec(),
            &[HANDLER_ID, CHECK_HANDLER_ID, FIX_HANDLER_ID],
            || LintCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for LintCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::NodeBridge)?;
        let started = std::time::Instant::now();

        // §5.1 验收：双运行时项目要告诉用户我们选了哪个（`deno.json` + `package.json` 等）。
        ctx.log_runtime_decision(None);

        // B8 / B9 / B10：非 JS 项目优先走各自的运行时 bridge
        // （Python → lan-py-bridge、Go → lan-go-bridge、Dart → lan-dart-bridge）。
        // bridge 不可用时回落到 JS 簇 bridge 的 spawn 适配器（§5.3 回落矩阵）。
        // 两条路径产出同一份 `Diagnostic`，因此渲染层不需要分支。
        if let Some(execution) = run_runtime_bridge(ctx, started).await? {
            return Ok(execution);
        }

        let request = LintCommandPlugin::build_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        emit_lint_report(ctx, &run.exchange.response.result, started.elapsed());
        let exit_code = run
            .exchange
            .response
            .result
            .as_ref()
            .and_then(|result| result["exitCode"].as_i64())
            .unwrap_or(lania_host::EXIT_LINT_FAILED as i64) as i32;
        Ok(ctx.complete_bridge(run, exit_code))
    }
}

/// 非 JS 项目的 lint：交给该运行时自己的 bridge（§3.3 / §3.4 / §3.6）。
///
/// 返回 `Ok(None)` 表示「项目是 JS 簇 / bridge 不可用 / 没有可用工具 / 调用失败」，
/// 调用方继续走原来的 JS 簇路径 —— 这就是 §5.3 要求的「移除 bridge 后仍可用」。
///
/// 结果形状与 `lint.run` 保持一致（`diagnostics` / `counts` / `resultsByAdaptor` / `exitCode`），
/// 因此 `diagnostic_views` 与 `LintReportRenderer` 不需要知道结果来自哪个 bridge。
async fn run_runtime_bridge(
    ctx: &CommandExecutionContext<'_>,
    started: std::time::Instant,
) -> Result<Option<CommandExecution>> {
    let cwd = ctx.command().cwd.clone();
    if cwd.is_empty() {
        return Ok(None);
    }
    // 运行时判定复用 `lania_host::detect_runtime`（§5.1 唯一入口），
    // 不再各命令自己数特征文件。
    let kind = lania_host::detect_runtime(std::path::Path::new(&cwd), ctx.project_config(), None);
    if !has_lint_bridge(kind) || !ctx.has_bridge(kind) {
        return Ok(None);
    }
    let adaptors = runtime_lint_adaptors(ctx, kind);
    if adaptors.is_empty() {
        return Ok(None);
    }
    // 可能是延迟解析的 bridge（Go 首次使用会编译一次）；失败 ⇒ 回落。
    let Ok(client) = ctx.bridge(kind) else {
        return Ok(None);
    };
    let bridge_label = runtime_bridge_label(kind);
    let mode = lint_mode(ctx.command());

    let mut results = Vec::new();
    for adaptor in adaptors {
        let request = client.request(
            format!("lint.{adaptor}"),
            serde_json::json!({
                "cwd": cwd,
                "mode": mode,
                "fix": mode == "fix",
            }),
        );
        match ctx.call_runtime_bridge(&client, request).await {
            Ok(exchange) => {
                if let Some(result) = exchange.response.result {
                    results.push(result);
                }
            }
            Err(error) => {
                ctx.workflow_services().logger.log(
                    LogLevel::Warn,
                    format!("{adaptor} via {bridge_label} failed, falling back: {error}"),
                );
                return Ok(None);
            }
        }
    }

    let payload = runtime_lint_payload(mode, bridge_label, &results);
    emit_lint_report(ctx, &Some(payload.clone()), started.elapsed());
    let exit_code = payload["exitCode"].as_i64().unwrap_or(0) as i32;
    Ok(Some(ctx.complete_runtime_bridge(
        client.request("lint.run", serde_json::json!({ "cwd": cwd })),
        runtime_lint_exchange(payload),
        exit_code,
    )))
}

/// `lint.run` 的 `mode` 推导（与 `build_request` 保持同一份规则）。
fn lint_mode(context: &CommandContext) -> &'static str {
    let requested_fix = context
        .argv
        .options
        .get("fix")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    match context.handler_id.as_str() {
        FIX_HANDLER_ID => "fix",
        CHECK_HANDLER_ID => "check",
        _ if requested_fix => "fix",
        _ => "check",
    }
}

/// `--linters` 过滤（只接受该运行时拥有的工具名；其它语言的工具名交给 JS 簇处理）。
fn requested_linters(context: &CommandContext, kind: RuntimeKind) -> Vec<String> {
    let own = runtime_lint_tools(kind);
    context
        .argv
        .options
        .get("linters")
        .and_then(|value| value.as_str())
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| own.contains(item))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// 该运行时的 lint 工具集：显式 `--linters` > `lan.config` 的 `lintTools` > 内置默认。
///
/// 只保留属于该运行时的工具：`lan.config` 里残留的 `eslint` 之类由 JS 簇处理，
/// 硬塞给语言 bridge 只会得到「工具不存在」的噪声。
fn runtime_lint_adaptors(ctx: &CommandExecutionContext<'_>, kind: RuntimeKind) -> Vec<String> {
    let requested = requested_linters(ctx.command(), kind);
    if !requested.is_empty() {
        return requested;
    }
    let configured = configured_lint_tools(ctx, kind);
    if !configured.is_empty() {
        return configured;
    }
    default_lint_tool(kind)
        .map(|tool| vec![tool.to_string()])
        .unwrap_or_default()
}

/// 读 `lan.config` 的 `lintTools`，只保留当前运行时的工具。
///
/// 优先用已经求值过的配置快照（B2.5 的强类型字段）；快照缺失时才回落到
/// 文本粗解析 —— 后者只为「bridge 起不来、快照拿不到」的极端场景兜底。
fn configured_lint_tools(ctx: &CommandExecutionContext<'_>, kind: RuntimeKind) -> Vec<String> {
    let own = runtime_lint_tools(kind);
    let from_snapshot = ctx
        .project_config()
        .map(|config| config.lint_tools.clone())
        .unwrap_or_default();
    if !from_snapshot.is_empty() {
        return from_snapshot
            .into_iter()
            .filter(|item| own.contains(&item.as_str()))
            .collect();
    }
    let cwd = ctx.command().cwd.clone();
    if cwd.is_empty() {
        return Vec::new();
    }
    config_text_lint_tools(&cwd, kind)
}

/// 从 `lan.config.*` 文本里粗取 `lintTools` 列表（快照不可用时的兜底）。
///
/// 刻意不做完整 TS/JS 解析：这里只需要「是不是这门语言 + 用哪些工具」，
/// 配置解析的权威实现仍在 bridge 侧（§6.5「候选列表只有一处」）。
fn config_text_lint_tools(cwd: &str, kind: RuntimeKind) -> Vec<String> {
    let own = runtime_lint_tools(kind);
    let config_files = [
        "lan.config.ts",
        "lan.config.js",
        "lan.config.mjs",
        "lan.config.cjs",
    ];
    for file in config_files {
        let path = std::path::Path::new(cwd).join(file);
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let tools = extract_lint_tools(&contents, &own);
        if !tools.is_empty() {
            return tools;
        }
    }
    Vec::new()
}

/// 从 `lan.config.*` 文本里粗取 `lintTools` 列表。
fn extract_lint_tools(contents: &str, own: &[&str]) -> Vec<String> {
    let Some(index) = contents.find("lintTools") else {
        return Vec::new();
    };
    let rest = &contents[index..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let Some(close) = rest[open..].find(']') else {
        return Vec::new();
    };
    rest[open + 1..open + close]
        .split(',')
        .map(|item| item.trim().trim_matches(|ch| ch == '\'' || ch == '"'))
        .filter(|item| own.contains(item))
        .map(ToOwned::to_owned)
        .collect()
}

/// 合并各工具结果（与 `plugin.ts` 的 `summarizeResults` / `countDiagnostics` 同语义）。
fn runtime_lint_payload(mode: &str, bridge: &str, results: &[Value]) -> Value {
    let mut diagnostics: Vec<Value> = Vec::new();
    let mut results_by_adaptor = serde_json::Map::new();
    let mut errors = 0u64;
    let mut warnings = 0u64;
    let mut files = 0u64;
    for result in results {
        let adaptor = result["adaptor"].as_str().unwrap_or("unknown").to_string();
        errors += result["errors"].as_u64().unwrap_or(0);
        warnings += result["warnings"].as_u64().unwrap_or(0);
        files += result["files"].as_array().map(Vec::len).unwrap_or(0) as u64;
        if let Some(items) = result["diagnostics"].as_array() {
            diagnostics.extend(items.iter().cloned());
        }
        results_by_adaptor.insert(adaptor, result.clone());
    }
    let counts = serde_json::json!({
        "error": diagnostics.iter().filter(|item| item["severity"] == "error").count(),
        "warning": diagnostics.iter().filter(|item| item["severity"] == "warning").count(),
        "info": diagnostics.iter().filter(|item| item["severity"] == "info").count(),
        "hint": diagnostics.iter().filter(|item| item["severity"] == "hint").count(),
    });
    let exit_code = if errors > 0 { 1 } else { 0 };
    serde_json::json!({
        "accepted": true,
        "mode": mode,
        "fix": mode == "fix",
        "bridge": bridge,
        "summary": {
            "errors": errors,
            "warnings": warnings,
            "files": files,
            "adaptors": results.len(),
        },
        "resultsByAdaptor": results_by_adaptor,
        "diagnostics": diagnostics,
        "counts": counts,
        "exitCode": exit_code,
    })
}

/// 把已聚合的结果包装成 `BridgeExchange`（输出层只看 `response.result`）。
fn runtime_lint_exchange(payload: Value) -> lania_node_bridge::BridgeExchange {
    lania_node_bridge::BridgeExchange {
        response: lania_node_bridge::BridgeResponse {
            id: "runtime-lint".into(),
            result: Some(payload),
            error: None,
        },
        events: Vec::new(),
    }
}

/// 把 bridge 的 `diagnostics` 数组映射成展示模型。
fn diagnostic_views(result: &Value) -> Vec<lania_presentation::DiagnosticView> {
    let Some(items) = result["diagnostics"].as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let severity = match item["severity"].as_str().unwrap_or("error") {
                "warning" => lania_presentation::ReportSeverity::Warning,
                "info" => lania_presentation::ReportSeverity::Info,
                "hint" => lania_presentation::ReportSeverity::Hint,
                _ => lania_presentation::ReportSeverity::Error,
            };
            lania_presentation::DiagnosticView {
                tool: item["tool"].as_str().unwrap_or("unknown").to_string(),
                id: item["id"].as_str().map(ToOwned::to_owned),
                severity,
                message: item["message"].as_str().unwrap_or_default().to_string(),
                file: item["file"].as_str().map(ToOwned::to_owned),
                line: item["range"]["start"]["line"].as_u64(),
                col: item["range"]["start"]["col"].as_u64(),
                fix_available: item["fix"]["available"].as_bool(),
            }
        })
        .collect()
}

/// 统一渲染 lint 结果（§7.4）。
///
/// - 有 `diagnostics` 时走新的 `LintReportRenderer`：按文件分组、列对齐、severity 高亮。
/// - 只有旧字段（`summaryByAdaptor`）时才回落到旧的按 adaptor 输出，保证兼容。
fn emit_lint_report(
    ctx: &CommandExecutionContext<'_>,
    result: &Option<Value>,
    elapsed: std::time::Duration,
) {
    let Some(result) = result.as_ref() else {
        return;
    };
    let views = diagnostic_views(result);
    if views.is_empty() {
        emit_grouped_output(ctx, &Some(result.clone()));
        return;
    }
    let logger = ctx.workflow_services().logger.scoped("command.lint");
    let report =
        lania_presentation::LintReport::from_diagnostics(views, Some(elapsed.as_millis() as u64));
    let renderer = lania_presentation::LintReportRenderer::for_terminal(
        std::io::IsTerminal::is_terminal(&std::io::stdout()),
        ctx.locale(),
    );
    for line in renderer.render(&report) {
        logger.log(LogLevel::Info, line);
    }
}

fn emit_grouped_output(ctx: &CommandExecutionContext<'_>, result: &Option<Value>) {
    let Some(result) = result.as_ref() else {
        return;
    };
    if result["groupedOutput"].as_bool() != Some(true) {
        return;
    }

    let logger = ctx.workflow_services().logger.scoped("command.lint");
    logger.log(LogLevel::Info, "lint grouped output:".to_string());

    let summary_by_adaptor = result["summaryByAdaptor"].as_object();
    let results_by_adaptor = result["resultsByAdaptor"].as_object();
    let Some(summary_by_adaptor) = summary_by_adaptor else {
        return;
    };

    for (adaptor, summary) in summary_by_adaptor {
        let implementation = summary["implementation"].as_str().unwrap_or("unknown");
        let errors = summary["errors"].as_u64().unwrap_or_default();
        let warnings = summary["warnings"].as_u64().unwrap_or_default();
        let files = summary["files"].as_u64().unwrap_or_default();
        logger.log(
            LogLevel::Info,
            format!(
                "[{adaptor}] {errors} errors, {warnings} warnings, {files} files ({implementation})"
            ),
        );
        if let Some(entries) = results_by_adaptor
            .and_then(|results| results.get(adaptor))
            .and_then(|result| result["files"].as_array())
        {
            for entry in entries {
                let file_path = entry["filePath"].as_str().unwrap_or(".");
                let file_errors = entry["errors"].as_u64().unwrap_or_default();
                let file_warnings = entry["warnings"].as_u64().unwrap_or_default();
                logger.log(
                    LogLevel::Info,
                    format!("  - {file_path}: {file_errors} errors, {file_warnings} warnings"),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lania_host::{
        capability::CapabilityContainer,
        plugin::{Plugin, PluginSetupContext},
        registry::{
            CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl, HandlerRegistry,
        },
        HookBusImpl,
    };

    use super::{LintCommandPlugin, CHECK_HANDLER_ID, FIX_HANDLER_ID, HANDLER_ID};
    use lania_command::CommandContext;
    use lania_node_bridge::{BridgeClientConfig, NodeBridgeClient};
    use serde_json::json;

    #[test]
    fn registers_lint_command_spec() {
        let plugin = LintCommandPlugin;
        let mut commands = CommandRegistryImpl::new();
        let mut hooks = HookBusImpl::new();
        let mut capabilities = CapabilityContainer::new();
        let mut handlers = CommandHandlerRegistryImpl::new();

        plugin
            .setup(&mut PluginSetupContext {
                commands: &mut commands,
                hooks: &mut hooks,
                capabilities: &mut capabilities,
                handlers: &mut handlers,
            })
            .expect("plugin setup succeeds");

        let spec = &commands.commands()[0];
        assert_eq!(spec.name, "lint");
        assert_eq!(spec.handler_id, "command.lint");
        assert_eq!(spec.subcommands.len(), 2);
        assert!(handlers.get(HANDLER_ID).is_some());
        assert!(handlers.get(CHECK_HANDLER_ID).is_some());
        assert!(handlers.get(FIX_HANDLER_ID).is_some());
    }

    // ---- B8 / B9 / B10：语言 bridge 的路由判定与结果整形 ----

    fn temp_project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "lania-lint-{name}-{}-{}",
            std::process::id(),
            name.len()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp project");
        for (file, contents) in files {
            std::fs::write(root.join(file), contents).expect("seed file");
        }
        root
    }

    #[test]
    fn runtime_tool_table_covers_all_language_bridges() {
        // 「adaptor → bridge」映射是唯一的一处；把预期写全，避免新增工具时漏登记。
        assert_eq!(
            super::runtime_lint_tools(lania_host::RuntimeKind::Python),
            vec!["mypy", "ruff", "black", "flake8"]
        );
        assert_eq!(
            super::runtime_lint_tools(lania_host::RuntimeKind::Go),
            vec!["go-vet", "golangci-lint"]
        );
        assert_eq!(
            super::runtime_lint_tools(lania_host::RuntimeKind::Dart),
            vec!["dart-analyze"]
        );
        assert_eq!(
            super::runtime_lint_tools(lania_host::RuntimeKind::Rust),
            vec!["clippy"]
        );
        // JS 簇没有独立 bridge：这些运行时永远走 node bridge。
        for kind in [
            lania_host::RuntimeKind::Node,
            lania_host::RuntimeKind::Bun,
            lania_host::RuntimeKind::Deno,
        ] {
            assert!(!super::has_lint_bridge(kind), "{kind:?}");
        }
        // 每个有 bridge 的运行时都必须有默认工具，否则 `lan lint` 会静默什么都不跑。
        for kind in [
            lania_host::RuntimeKind::Python,
            lania_host::RuntimeKind::Go,
            lania_host::RuntimeKind::Dart,
            lania_host::RuntimeKind::Rust,
        ] {
            assert!(super::has_lint_bridge(kind), "{kind:?}");
            assert!(super::default_lint_tool(kind).is_some(), "{kind:?}");
            assert_ne!(super::runtime_bridge_label(kind), "unknown");
        }
    }

    #[test]
    fn lint_tool_filtering_is_per_runtime() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("linters".into(), json!("mypy,go-vet,dart-analyze,eslint"))]
                    .into_iter()
                    .collect(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace".into(),
        };
        // 只保留属于该运行时的工具：不得把 go-vet 塞给 Python bridge。
        assert_eq!(
            super::requested_linters(&context, lania_host::RuntimeKind::Python),
            vec!["mypy".to_string()]
        );
        assert_eq!(
            super::requested_linters(&context, lania_host::RuntimeKind::Go),
            vec!["go-vet".to_string()]
        );
        assert_eq!(
            super::requested_linters(&context, lania_host::RuntimeKind::Dart),
            vec!["dart-analyze".to_string()]
        );
    }

    #[test]
    fn config_text_lint_tools_only_keeps_tools_of_the_runtime() {
        let root = temp_project(
            "tools",
            &[(
                "lan.config.js",
                "export default { lintTools: ['ruff', 'mypy', 'eslint', 'go-vet'] };\n",
            )],
        );
        let cwd = root.to_string_lossy().to_string();
        assert_eq!(
            super::config_text_lint_tools(&cwd, lania_host::RuntimeKind::Python),
            vec!["ruff".to_string(), "mypy".to_string()]
        );
        assert_eq!(
            super::config_text_lint_tools(&cwd, lania_host::RuntimeKind::Go),
            vec!["go-vet".to_string()]
        );
        assert!(super::config_text_lint_tools(&cwd, lania_host::RuntimeKind::Dart).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_lint_payload_matches_the_js_cluster_shape() {
        let payload = super::runtime_lint_payload(
            "check",
            "lan-go-bridge",
            &[json!({
                "adaptor": "go-vet",
                "implementation": "runtime",
                "errors": 1,
                "warnings": 1,
                "files": [{ "filePath": "main.go", "errors": 1, "warnings": 1 }],
                "diagnostics": [
                    { "tool": "go-vet", "severity": "error",
                      "message": "wrong type", "file": "main.go",
                      "range": { "start": { "line": 5, "col": 1 } } },
                    { "tool": "go-vet", "severity": "warning", "message": "warn",
                      "file": "main.go" }
                ]
            })],
        );
        assert_eq!(payload["bridge"], "lan-go-bridge");
        assert_eq!(payload["mode"], "check");
        assert_eq!(payload["exitCode"], 1);
        assert_eq!(payload["summary"]["errors"], 1);
        assert_eq!(payload["summary"]["adaptors"], 1);
        assert_eq!(payload["counts"]["error"], 1);
        assert_eq!(payload["counts"]["warning"], 1);
        assert_eq!(payload["resultsByAdaptor"]["go-vet"]["adaptor"], "go-vet");
        // 渲染层只认 `diagnostics`；bridge 自己的 result 形状必须和 `lint.run` 一致。
        assert_eq!(payload["diagnostics"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn lint_mode_honours_subcommands_and_the_fix_flag() {
        let make = |handler: &str, fix: bool| CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("fix".to_string(), json!(fix))].into_iter().collect(),
            },
            handler_id: handler.into(),
            trace_id: "trace".into(),
        };
        assert_eq!(super::lint_mode(&make(FIX_HANDLER_ID, false)), "fix");
        assert_eq!(super::lint_mode(&make(CHECK_HANDLER_ID, true)), "check");
        assert_eq!(super::lint_mode(&make(HANDLER_ID, true)), "fix");
        assert_eq!(super::lint_mode(&make(HANDLER_ID, false)), "check");
    }

    #[test]
    fn builds_lint_bridge_request_from_context() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("fix".into(), json!(true)),
                    (
                        "linters".into(),
                        json!("oxfmt,prettier,oxlint,eslint,unknown"),
                    ),
                    ("concurrency".into(), json!(2)),
                    ("grouped-output".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.lint".into(),
            trace_id: "trace-3".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = LintCommandPlugin::build_request(&context, &bridge);

        assert_eq!(request.method, "lint.run");
        assert_eq!(request.params["mode"], "fix");
        assert_eq!(request.params["fix"], true);
        assert_eq!(
            request.params["linters"],
            serde_json::json!(["oxfmt", "prettier", "oxlint", "eslint"])
        );
        assert_eq!(request.params["concurrency"], 2);
    }

    #[test]
    fn accepts_language_linters_in_request() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [(
                    "linters".into(),
                    json!("ruff,clippy,dart-analyze,golangci-lint,unknown"),
                )]
                .into_iter()
                .collect(),
            },
            handler_id: "command.lint".into(),
            trace_id: "trace-lang".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = LintCommandPlugin::build_request(&context, &bridge);

        assert_eq!(
            request.params["linters"],
            serde_json::json!(["ruff", "clippy", "dart-analyze", "golangci-lint"])
        );
    }

    #[test]
    fn accepts_deno_adaptors_and_drops_unknown_linters() {
        let context = lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("linters".into(), json!("deno-lint, deno-fmt,bun,unknown"))]
                    .into_iter()
                    .collect(),
            },
            handler_id: "command.lint".into(),
            trace_id: "trace-deno".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = LintCommandPlugin::build_request(&context, &bridge);

        // `bun` 是 build 诊断的 tool 名，不是 lint adaptor，必须被过滤掉。
        assert_eq!(
            request.params["linters"],
            serde_json::json!(["deno-lint", "deno-fmt"])
        );
    }

    #[test]
    fn maps_lint_subcommand_modes() {
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        for (handler_id, expected_mode, expected_fix) in [
            (HANDLER_ID, "check", false),
            (CHECK_HANDLER_ID, "check", false),
            (FIX_HANDLER_ID, "fix", true),
        ] {
            let context = CommandContext {
                name: String::new(),
                cwd: "/repo".into(),
                argv: lania_command::ParsedArgv::default(),
                handler_id: handler_id.to_string(),
                trace_id: "trace".into(),
            };
            let request = LintCommandPlugin::build_request(&context, &bridge);
            assert_eq!(request.params["mode"], expected_mode);
            assert_eq!(request.params["fix"], expected_fix);
        }
    }
}
