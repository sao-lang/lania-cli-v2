//! 将 `release` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 主要导出：spec、build_input、ReleaseCommandPlugin、HANDLER_ID、PLAN_HANDLER_ID、RUN_HANDLER_ID。
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
use lania_workflows::{ReleaseMode, ReleaseWorkflow, ReleaseWorkflowInput, WorkflowState};

pub const HANDLER_ID: &str = "command.release";
pub const PLAN_HANDLER_ID: &str = "command.release.plan";
pub const RUN_HANDLER_ID: &str = "command.release.run";
pub const RESUME_HANDLER_ID: &str = "command.release.resume";
pub const STATUS_HANDLER_ID: &str = "command.release.status";

#[derive(Debug, Default)]
pub struct ReleaseCommandPlugin;

struct ReleaseCommandHandler;

impl ReleaseCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new("release", "Project release orchestration", HANDLER_ID)
            .with_options(Self::shared_options())
            .with_examples(vec![
                Example {
                    command: "lan release --profile web-app --env prod".into(),
                    description: "Preview a release plan without executing".into(),
                },
                Example {
                    command: "lan release run --apply --yes --from verify --to finalize".into(),
                    description: "Execute the selected release stage range".into(),
                },
            ])
            .with_subcommands(vec![
                Self::plan_spec(),
                Self::run_spec(),
                Self::resume_spec(),
                Self::status_spec(),
            ])
    }

    fn shared_options() -> Vec<OptionSpec> {
        vec![
            OptionSpec {
                long: "version".into(),
                short: Some('v'),
                help: "Release version".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "tag".into(),
                short: Some('t'),
                help: "Package dist-tag override".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "profile".into(),
                short: Some('P'),
                help: "Release profile: package, web-app, service, custom".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "env".into(),
                short: Some('e'),
                help: "Target release environment".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "channel".into(),
                short: Some('C'),
                help: "Release channel or deploy channel".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "from".into(),
                short: Some('f'),
                help: "Start release from the given stage".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "to".into(),
                short: Some('T'),
                help: "Stop release after the given stage".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "skip".into(),
                short: Some('s'),
                help: "Comma-separated stages to skip".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "state-file".into(),
                short: Some('S'),
                help: "Release state file path".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "publish".into(),
                short: Some('p'),
                help: "Enable package publish stage".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "changelog".into(),
                short: Some('c'),
                help: "Enable changelog stage".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "skip-git".into(),
                short: None,
                help: "Disable finalize git commit/tag/push behavior".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "apply".into(),
                short: Some('a'),
                help: "Apply release actions instead of plan-only output".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "dry-run".into(),
                short: Some('d'),
                help: "Force plan-only mode even for run/resume".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "yes".into(),
                short: Some('y'),
                help: "Confirm release execution without extra prompts".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
        ]
    }

    fn plan_spec() -> CommandSpec {
        CommandSpec::new(
            "plan",
            "Generate a release plan and persist release state",
            PLAN_HANDLER_ID,
        )
        .with_options(Self::shared_options())
        .with_examples(vec![
            Example {
                command: "lan release plan --profile package --version 1.2.3".into(),
                description: "Preview package release stages".into(),
            },
            Example {
                command: "lan release plan --profile web-app --env prod --skip version".into(),
                description: "Preview a web deployment plan".into(),
            },
        ])
    }

    fn run_spec() -> CommandSpec {
        CommandSpec::new(
            "run",
            "Execute a release plan with state persistence",
            RUN_HANDLER_ID,
        )
        .with_options(Self::shared_options())
    }

    fn resume_spec() -> CommandSpec {
        CommandSpec::new(
            "resume",
            "Resume a previously failed or partial release",
            RESUME_HANDLER_ID,
        )
        .with_options(Self::shared_options())
    }

    fn status_spec() -> CommandSpec {
        CommandSpec::new(
            "status",
            "Inspect the latest persisted release state",
            STATUS_HANDLER_ID,
        )
        .with_options(vec![OptionSpec {
            long: "state-file".into(),
            short: Some('S'),
            help: "Release state file path".into(),
            value_kind: ValueKind::OptionalString,
            default_value: None,
            choices: vec![],
            negatable: false,
        }])
    }

    pub fn build_input(context: &CommandContext) -> ReleaseWorkflowInput {
        ReleaseWorkflowInput {
            cwd: context.cwd.clone().into(),
            mode: match context.handler_id.as_str() {
                RUN_HANDLER_ID => ReleaseMode::Run,
                RESUME_HANDLER_ID => ReleaseMode::Resume,
                STATUS_HANDLER_ID => ReleaseMode::Status,
                _ => ReleaseMode::Plan,
            },
            version: context
                .argv
                .options
                .get("version")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            tag: context
                .argv
                .options
                .get("tag")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            profile: context
                .argv
                .options
                .get("profile")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            env: context
                .argv
                .options
                .get("env")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            channel: context
                .argv
                .options
                .get("channel")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            from_stage: context
                .argv
                .options
                .get("from")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            to_stage: context
                .argv
                .options
                .get("to")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            skip_stages: context
                .argv
                .options
                .get("skip")
                .and_then(|value| value.as_str())
                .into_iter()
                .flat_map(|value| value.split(','))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
            state_file: context
                .argv
                .options
                .get("state-file")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            apply: context
                .argv
                .options
                .get("apply")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            dry_run: context
                .argv
                .options
                .get("dry-run")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            yes: context
                .argv
                .options
                .get("yes")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            publish: context
                .argv
                .options
                .get("publish")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            changelog: context
                .argv
                .options
                .get("changelog")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            skip_git: context
                .argv
                .options
                .get("skip-git")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        }
    }
}

impl Plugin for ReleaseCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-release".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Git,
                CapabilityName::Exec,
                CapabilityName::Task,
                CapabilityName::Progress,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<ReleaseCommandHandler, _>(
            ctx,
            "command-release",
            "release",
            Self::spec(),
            &[
                HANDLER_ID,
                PLAN_HANDLER_ID,
                RUN_HANDLER_ID,
                RESUME_HANDLER_ID,
                STATUS_HANDLER_ID,
            ],
            || ReleaseCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for ReleaseCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::Git)?;
        ctx.emit_workflow_start("release").await;
        let input = ReleaseCommandPlugin::build_input(ctx.command());
        let execution = ctx
            .run_workflow_with_tasks("release", "Release workflow", move |services| async move {
                let workflow = ReleaseWorkflow;
                workflow.run(&services, input).await
            })
            .await?;
        ctx.emit_workflow_complete("release").await;
        let exit_code = if execution.state == WorkflowState::Failed {
            lania_host::EXIT_RUNTIME_ERROR
        } else {
            lania_host::EXIT_SUCCESS
        };
        Ok(ctx.complete_workflow(execution, exit_code))
    }
}

#[cfg(test)]
mod tests;
