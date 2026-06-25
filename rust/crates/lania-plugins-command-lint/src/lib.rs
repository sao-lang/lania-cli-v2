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
};
use lania_logger::LogLevel;
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};
use serde_json::Value;

pub const HANDLER_ID: &str = "command.lint";
pub const CHECK_HANDLER_ID: &str = "command.lint.check";
pub const FIX_HANDLER_ID: &str = "command.lint.fix";

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
                help: "Comma-separated linter adaptors to run (oxlint,eslint,oxfmt,prettier,stylelint,textlint)".into(),
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
                            "oxlint" | "eslint" | "oxfmt" | "prettier" | "stylelint" | "textlint"
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
        let request = LintCommandPlugin::build_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        emit_grouped_output(ctx, &run.exchange.response.result);
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

    #[test]
    fn builds_lint_bridge_request_from_context() {
        let context = CommandContext {
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
    fn maps_lint_subcommand_modes() {
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        for (handler_id, expected_mode, expected_fix) in [
            (HANDLER_ID, "check", false),
            (CHECK_HANDLER_ID, "check", false),
            (FIX_HANDLER_ID, "fix", true),
        ] {
            let context = CommandContext {
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
