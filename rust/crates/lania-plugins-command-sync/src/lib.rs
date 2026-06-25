//! 将 `sync` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 主要导出：spec、status_spec、commit_spec、push_spec、build_input、SyncCommandPlugin。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use anyhow::Result;
use async_trait::async_trait;
use lania_command::{CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{
        CommandExecution, CommandExecutionContext, CommandHandler, ExecutionError, EXIT_LINT_FAILED,
    },
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};
use lania_workflows::{SyncMode, SyncWorkflow, SyncWorkflowInput};

pub const HANDLER_ID: &str = "command.sync";
pub const STATUS_HANDLER_ID: &str = "command.sync.status";
pub const COMMIT_HANDLER_ID: &str = "command.sync.commit";
pub const PUSH_HANDLER_ID: &str = "command.sync.push";

#[derive(Debug, Default)]
pub struct SyncCommandPlugin;

struct SyncCommandHandler;

fn option_value<'a>(
    options: &'a std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<&'a serde_json::Value> {
    options.get(key)
}

fn option_string(
    options: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    option_value(options, key)
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

fn option_bool(
    options: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<bool> {
    option_value(options, key).and_then(serde_json::Value::as_bool)
}

impl SyncCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new("sync", "Quickly sync local code to git", HANDLER_ID)
            .with_options(Self::shared_options(true))
            .with_examples(vec![
                Example {
                    command: "lan sync --message \"chore(sync): update workspace\"".into(),
                    description: "Stage, commit, and push current changes".into(),
                },
                Example {
                    command: "lan sync --no-push --message \"feat: partial sync\"".into(),
                    description: "Commit locally without pushing".into(),
                },
            ])
            .with_subcommands(vec![
                Self::status_spec(),
                Self::commit_spec(),
                Self::push_spec(),
            ])
    }

    fn shared_options(include_push_toggle: bool) -> Vec<OptionSpec> {
        let mut options = vec![
            OptionSpec {
                long: "remote".into(),
                short: Some('r'),
                help: "Remote name".into(),
                value_kind: ValueKind::String,
                default_value: Some("origin".into()),
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "branch".into(),
                short: Some('b'),
                help: "Branch name".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "message".into(),
                short: Some('m'),
                help: "Commit message override".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "amend".into(),
                short: Some('a'),
                help: "Amend the latest commit".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "force-with-lease".into(),
                short: Some('f'),
                help: "Force push with lease protection".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "dry-run".into(),
                short: Some('d'),
                help: "Plan git commands without executing them".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "interactive".into(),
                short: Some('i'),
                help: "Generate commit message via commitizen".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
        ];
        if include_push_toggle {
            options.push(OptionSpec {
                long: "push".into(),
                short: Some('p'),
                help: "Push after commit".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            });
        }
        options
    }

    pub fn status_spec() -> CommandSpec {
        CommandSpec::new("status", "Show git sync status", STATUS_HANDLER_ID).with_options(vec![
            OptionSpec {
                long: "remote".into(),
                short: Some('r'),
                help: "Remote name".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "branch".into(),
                short: Some('b'),
                help: "Branch name".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
        ])
    }

    pub fn commit_spec() -> CommandSpec {
        CommandSpec::new("commit", "Stage and commit changes", COMMIT_HANDLER_ID)
            .with_options(Self::shared_options(true))
            .with_examples(vec![Example {
                command: "lan sync commit --message \"chore: save work\" --no-push".into(),
                description: "Create a local commit only".into(),
            }])
    }

    pub fn push_spec() -> CommandSpec {
        CommandSpec::new("push", "Push current branch to remote", PUSH_HANDLER_ID).with_options(
            vec![
                OptionSpec {
                    long: "remote".into(),
                    short: Some('r'),
                    help: "Remote name".into(),
                    value_kind: ValueKind::OptionalString,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "branch".into(),
                    short: Some('b'),
                    help: "Branch name".into(),
                    value_kind: ValueKind::OptionalString,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "force-with-lease".into(),
                    short: Some('f'),
                    help: "Force push with lease protection".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "dry-run".into(),
                    short: Some('d'),
                    help: "Plan git commands without executing them".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
            ],
        )
    }

    pub fn build_input(context: &CommandContext) -> SyncWorkflowInput {
        let options: &std::collections::BTreeMap<String, serde_json::Value> = &context.argv.options;
        SyncWorkflowInput {
            cwd: context.cwd.clone().into(),
            remote: option_string(options, "remote"),
            branch: option_string(options, "branch"),
            message: option_string(options, "message"),
            push: option_bool(options, "push"),
            amend: option_bool(options, "amend").unwrap_or(false),
            force_with_lease: option_bool(options, "force-with-lease").unwrap_or(false),
            dry_run: option_bool(options, "dry-run").unwrap_or(false),
            interactive: option_bool(options, "interactive").unwrap_or(false),
            mode: match context.handler_id.as_str() {
                STATUS_HANDLER_ID => SyncMode::Status,
                COMMIT_HANDLER_ID => SyncMode::Commit,
                PUSH_HANDLER_ID => SyncMode::Push,
                _ => SyncMode::Sync,
            },
        }
    }
}

impl Plugin for SyncCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-sync".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Git,
                CapabilityName::Exec,
                CapabilityName::Task,
                CapabilityName::Progress,
                CapabilityName::NodeBridge,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<SyncCommandHandler, _>(
            ctx,
            "command-sync",
            "sync",
            Self::spec(),
            &[HANDLER_ID, COMMIT_HANDLER_ID, STATUS_HANDLER_ID, PUSH_HANDLER_ID],
            || SyncCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for SyncCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::Git)?;
        ctx.emit_workflow_start("sync").await;
        let input = SyncCommandPlugin::build_input(ctx.command());
        let execution = match ctx
            .run_workflow_with_tasks("sync", "Sync workflow", move |services| async move {
                let workflow = SyncWorkflow;
                workflow.run(&services, input).await
            })
            .await
        {
            Ok(execution) => execution,
            Err(error)
                if error
                    .to_string()
                    .contains("commitlint rejected commit message") =>
            {
                return Err(ExecutionError {
                    exit_code: EXIT_LINT_FAILED,
                    message: if ctx.locale() == "zh" {
                        format!("提交信息未通过 commitlint 校验：{error}")
                    } else {
                        error.to_string()
                    },
                }
                .into());
            }
            Err(error) => return Err(error),
        };
        ctx.emit_workflow_complete("sync").await;
        Ok(ctx.complete_workflow(execution, lania_host::EXIT_SUCCESS))
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
    use serde_json::json;

    use lania_workflows::SyncMode;

    use super::{
        SyncCommandPlugin, COMMIT_HANDLER_ID, HANDLER_ID, PUSH_HANDLER_ID, STATUS_HANDLER_ID,
    };

    #[test]
    fn registers_sync_spec() {
        let plugin = SyncCommandPlugin;
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
        assert_eq!(spec.name, "sync");
        assert_eq!(spec.handler_id, HANDLER_ID);
        assert_eq!(spec.subcommands.len(), 3);
        assert!(handlers.get(HANDLER_ID).is_some());
        assert!(handlers.get(COMMIT_HANDLER_ID).is_some());
        assert!(handlers.get(STATUS_HANDLER_ID).is_some());
        assert!(handlers.get(PUSH_HANDLER_ID).is_some());
    }

    #[test]
    fn builds_sync_input_from_context() {
        let context = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("remote".into(), json!("upstream")),
                    ("branch".into(), json!("develop")),
                    ("message".into(), json!("feat(sync): ship changes")),
                    ("push".into(), json!(false)),
                    ("amend".into(), json!(true)),
                    ("force-with-lease".into(), json!(true)),
                    ("dry-run".into(), json!(true)),
                    ("interactive".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace-sync".into(),
        };

        let input = SyncCommandPlugin::build_input(&context);
        assert_eq!(input.remote.as_deref(), Some("upstream"));
        assert_eq!(input.branch.as_deref(), Some("develop"));
        assert_eq!(input.message.as_deref(), Some("feat(sync): ship changes"));
        assert_eq!(input.push, Some(false));
        assert!(input.amend);
        assert!(input.force_with_lease);
        assert!(input.dry_run);
        assert!(input.interactive);
        assert_eq!(input.mode, SyncMode::Sync);
    }

    #[test]
    fn builds_sync_subcommand_modes() {
        for (handler_id, expected_mode) in [
            (HANDLER_ID, SyncMode::Sync),
            (STATUS_HANDLER_ID, SyncMode::Status),
            (COMMIT_HANDLER_ID, SyncMode::Commit),
            (PUSH_HANDLER_ID, SyncMode::Push),
        ] {
            let context = lania_command::CommandContext {
                cwd: "/repo".into(),
                argv: lania_command::ParsedArgv::default(),
                handler_id: handler_id.to_string(),
                trace_id: "trace-1".into(),
            };
            let input = SyncCommandPlugin::build_input(&context);
            assert_eq!(input.mode, expected_mode);
        }
    }
}
