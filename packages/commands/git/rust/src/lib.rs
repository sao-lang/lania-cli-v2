//! `lan git`：把常用 git 读写操作收敛成一个命令族，并接管原 `lan sync` 的四个动作。
//!
//! 分层约定：
//! - 本 crate 只负责 CLI 形状（子命令、选项、`--yes` 校验）与 workflow 调用
//! - git 语义在 `lania-workflows` 的 `GitWorkflow`（`sync` / `commit` / `push` 复用 sync 内核）
//!
//! 与旧 `lan sync` 的对应关系：
//! | 旧命令 | 新命令 |
//! | --- | --- |
//! | `lan sync` | `lan git sync` |
//! | `lan sync status` | `lan git status` |
//! | `lan sync commit` | `lan git commit` |
//! | `lan sync push` | `lan git push` |
use anyhow::Result;
use async_trait::async_trait;
use lania_command::{ArgSpec, CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{
        CommandExecution, CommandExecutionContext, CommandHandler, ExecutionError, EXIT_LINT_FAILED,
    },
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};
use lania_workflows::{GitWorkflow, GitWorkflowInput, GitWorkflowMode};

pub const HANDLER_ID: &str = "command.git";
pub const STATUS_HANDLER_ID: &str = "command.git.status";
pub const LOG_HANDLER_ID: &str = "command.git.log";
pub const DIFF_HANDLER_ID: &str = "command.git.diff";
pub const BRANCH_LIST_HANDLER_ID: &str = "command.git.branch.list";
pub const BRANCH_CREATE_HANDLER_ID: &str = "command.git.branch.create";
pub const BRANCH_SWITCH_HANDLER_ID: &str = "command.git.branch.switch";
pub const BRANCH_DELETE_HANDLER_ID: &str = "command.git.branch.delete";
pub const TAG_LIST_HANDLER_ID: &str = "command.git.tag.list";
pub const TAG_CREATE_HANDLER_ID: &str = "command.git.tag.create";
pub const TAG_DELETE_HANDLER_ID: &str = "command.git.tag.delete";
pub const ADD_HANDLER_ID: &str = "command.git.add";
pub const RESTORE_HANDLER_ID: &str = "command.git.restore";
pub const SYNC_HANDLER_ID: &str = "command.git.sync";
pub const COMMIT_HANDLER_ID: &str = "command.git.commit";
pub const PUSH_HANDLER_ID: &str = "command.git.push";

#[derive(Debug, Default)]
pub struct GitCommandPlugin;

struct GitCommandHandler;

impl GitCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new("git", "Inspect and sync git state", HANDLER_ID)
            .with_alias("gt")
            .with_examples(vec![
                Example {
                    command: "lan git status".into(),
                    description: "Show working tree status".into(),
                },
                Example {
                    command: "lan git sync -m \"feat: ship it\"".into(),
                    description: "Stage, commit, and push current changes".into(),
                },
                Example {
                    command: "lan git log --limit 5".into(),
                    description: "Show the last five commits".into(),
                },
            ])
            .with_subcommands(vec![
                Self::status_spec(),
                Self::log_spec(),
                Self::diff_spec(),
                Self::branch_spec(),
                Self::tag_spec(),
                Self::add_spec(),
                Self::restore_spec(),
                Self::sync_spec(),
                Self::commit_spec(),
                Self::push_spec(),
            ])
    }

    /// sync/commit/push 共享的选项，与原 `lan sync` 保持一致。
    fn sync_options(include_push_toggle: bool) -> Vec<OptionSpec> {
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

    fn sync_spec() -> CommandSpec {
        CommandSpec::new(
            "sync",
            "Stage, commit, and push current changes",
            SYNC_HANDLER_ID,
        )
        .with_options(Self::sync_options(true))
        .with_examples(vec![
            Example {
                command: "lan git sync -m \"chore: update workspace\"".into(),
                description: "Stage, commit, and push current changes".into(),
            },
            Example {
                command: "lan git sync --no-push -m \"feat: partial sync\"".into(),
                description: "Commit locally without pushing".into(),
            },
        ])
    }

    fn commit_spec() -> CommandSpec {
        CommandSpec::new("commit", "Stage and commit changes", COMMIT_HANDLER_ID)
            .with_options(Self::sync_options(true))
            .with_examples(vec![Example {
                command: "lan git commit -m \"chore: save work\" --no-push".into(),
                description: "Create a local commit only".into(),
            }])
    }

    fn push_spec() -> CommandSpec {
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

    fn status_spec() -> CommandSpec {
        CommandSpec::new(
            "status",
            "Show working tree status",
            STATUS_HANDLER_ID,
        )
    }

    fn log_spec() -> CommandSpec {
        CommandSpec::new("log", "Show recent commits", LOG_HANDLER_ID).with_options(vec![
            limit_option("Number of commits to show"),
        ])
    }

    fn diff_spec() -> CommandSpec {
        CommandSpec::new("diff", "Show working tree or staged diff", DIFF_HANDLER_ID)
            .with_options(vec![
                OptionSpec {
                    long: "staged".into(),
                    short: Some('s'),
                    help: "Show staged changes only".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "path".into(),
                    short: Some('P'),
                    help: "Limit diff to a path".into(),
                    value_kind: ValueKind::OptionalString,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
            ])
    }

    fn branch_spec() -> CommandSpec {
        CommandSpec::new("branch", "Manage branches", BRANCH_LIST_HANDLER_ID)
            .with_alias("br")
            .with_subcommands(vec![
                CommandSpec::new("list", "List local and remote branches", BRANCH_LIST_HANDLER_ID)
                    .with_examples(vec![Example {
                        command: "lan git branch list".into(),
                        description: "List local and remote branches".into(),
                    }]),
                CommandSpec::new("create", "Create a branch", BRANCH_CREATE_HANDLER_ID)
                    .with_args(vec![branch_arg()]),
                CommandSpec::new("switch", "Switch to a branch", BRANCH_SWITCH_HANDLER_ID)
                    .with_args(vec![branch_arg()]),
                CommandSpec::new("delete", "Delete a branch", BRANCH_DELETE_HANDLER_ID)
                    .with_args(vec![branch_arg()])
                    .with_options(vec![
                        yes_option("Confirm the deletion"),
                        OptionSpec {
                            long: "force".into(),
                            short: Some('f'),
                            help: "Delete even if the branch is not merged (-D)".into(),
                            value_kind: ValueKind::Bool,
                            default_value: None,
                            choices: vec![],
                            negatable: false,
                        },
                    ]),
            ])
    }

    fn tag_spec() -> CommandSpec {
        CommandSpec::new("tag", "Manage tags", TAG_LIST_HANDLER_ID).with_subcommands(vec![
            CommandSpec::new("list", "List tags", TAG_LIST_HANDLER_ID),
            CommandSpec::new("create", "Create a tag", TAG_CREATE_HANDLER_ID)
                .with_args(vec![tag_arg()])
                .with_options(vec![OptionSpec {
                    long: "message".into(),
                    short: Some('m'),
                    help: "Annotated tag message".into(),
                    value_kind: ValueKind::OptionalString,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                }]),
            CommandSpec::new("delete", "Delete a tag", TAG_DELETE_HANDLER_ID)
                .with_args(vec![tag_arg()])
                .with_options(vec![yes_option("Confirm the deletion")]),
        ])
    }

    fn add_spec() -> CommandSpec {
        CommandSpec::new("add", "Stage every change in the working tree", ADD_HANDLER_ID)
            .with_options(vec![OptionSpec {
                long: "dry-run".into(),
                short: Some('d'),
                help: "Plan git commands without executing them".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: false,
            }])
    }

    fn restore_spec() -> CommandSpec {
        CommandSpec::new(
            "restore",
            "Discard working tree or staged changes",
            RESTORE_HANDLER_ID,
        )
        .with_args(vec![ArgSpec {
            name: "paths".into(),
            required: false,
            multiple: true,
            help: "Paths to restore (defaults to the whole worktree)".into(),
        }])
        .with_options(vec![
            OptionSpec {
                long: "staged".into(),
                short: Some('s'),
                help: "Unstage instead of discarding working tree changes".into(),
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
        ])
    }

    pub fn build_input(context: &CommandContext) -> GitWorkflowInput {
        let options = &context.argv.options;
        let paths = context
            .argv
            .args
            .get("paths")
            .map(|value| match value {
                serde_json::Value::Array(items) => items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect(),
                serde_json::Value::String(text) => vec![text.clone()],
                _ => Vec::new(),
            })
            .unwrap_or_default();
        let tag = context.argv.args.get("tag").and_then(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| value.as_array().and_then(|items| items.first()?.as_str().map(Into::into)))
        });
        let message = option_string(options, "message").or_else(|| {
            context
                .argv
                .args
                .get("message")
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
        });

        GitWorkflowInput {
            cwd: context.cwd.clone().into(),
            mode: mode_for_handler(&context.handler_id),
            remote: option_string(options, "remote"),
            branch: positional_branch(context).or_else(|| option_string(options, "branch")),
            tag,
            message,
            push: option_bool(options, "push"),
            amend: option_bool(options, "amend").unwrap_or(false),
            force: option_bool(options, "force").unwrap_or(false),
            force_with_lease: option_bool(options, "force-with-lease").unwrap_or(false),
            dry_run: option_bool(options, "dry-run").unwrap_or(false),
            interactive: option_bool(options, "interactive").unwrap_or(false),
            staged: option_bool(options, "staged").unwrap_or(false),
            path: option_string(options, "path"),
            paths,
            limit: option_number(options, "limit").map(|value| value as usize),
        }
    }
}

/// `--yes` 是所有破坏性动作的统一确认开关。
fn yes_option(help: &str) -> OptionSpec {
    OptionSpec {
        long: "yes".into(),
        short: Some('y'),
        help: help.into(),
        value_kind: ValueKind::Bool,
        default_value: None,
        choices: vec![],
        negatable: false,
    }
}

fn limit_option(help: &str) -> OptionSpec {
    OptionSpec {
        long: "limit".into(),
        short: Some('n'),
        help: help.into(),
        value_kind: ValueKind::OptionalString,
        default_value: Some("10".into()),
        choices: vec![],
        negatable: false,
    }
}

fn branch_arg() -> ArgSpec {
    ArgSpec {
        name: "branch".into(),
        required: true,
        multiple: false,
        help: "Branch name".into(),
    }
}

fn tag_arg() -> ArgSpec {
    ArgSpec {
        name: "tag".into(),
        required: true,
        multiple: false,
        help: "Tag name".into(),
    }
}

fn mode_for_handler(handler_id: &str) -> GitWorkflowMode {
    match handler_id {
        STATUS_HANDLER_ID => GitWorkflowMode::Status,
        LOG_HANDLER_ID => GitWorkflowMode::Log,
        DIFF_HANDLER_ID => GitWorkflowMode::Diff,
        BRANCH_CREATE_HANDLER_ID => GitWorkflowMode::BranchCreate,
        BRANCH_SWITCH_HANDLER_ID => GitWorkflowMode::BranchSwitch,
        BRANCH_DELETE_HANDLER_ID => GitWorkflowMode::BranchDelete,
        BRANCH_LIST_HANDLER_ID => GitWorkflowMode::BranchList,
        TAG_CREATE_HANDLER_ID => GitWorkflowMode::TagCreate,
        TAG_DELETE_HANDLER_ID => GitWorkflowMode::TagDelete,
        TAG_LIST_HANDLER_ID => GitWorkflowMode::TagList,
        ADD_HANDLER_ID => GitWorkflowMode::Add,
        RESTORE_HANDLER_ID => GitWorkflowMode::Restore,
        COMMIT_HANDLER_ID => GitWorkflowMode::Commit,
        PUSH_HANDLER_ID => GitWorkflowMode::Push,
        _ => GitWorkflowMode::Sync,
    }
}

fn positional_branch(context: &CommandContext) -> Option<String> {
    context
        .argv
        .args
        .get("branch")
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
}

fn option_string(
    options: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    options.get(key).and_then(|value| match value {
        serde_json::Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    })
}

fn option_bool(
    options: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<bool> {
    options.get(key).and_then(serde_json::Value::as_bool)
}

fn option_number(
    options: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<i64> {
    options.get(key).and_then(|value| match value {
        serde_json::Value::Number(number) => number.as_i64(),
        serde_json::Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    })
}

impl Plugin for GitCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-git".into(),
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
        register_builtin_command::<GitCommandHandler, _>(
            ctx,
            "command-git",
            "git",
            Self::spec(),
            &[
                HANDLER_ID,
                STATUS_HANDLER_ID,
                LOG_HANDLER_ID,
                DIFF_HANDLER_ID,
                BRANCH_LIST_HANDLER_ID,
                BRANCH_CREATE_HANDLER_ID,
                BRANCH_SWITCH_HANDLER_ID,
                BRANCH_DELETE_HANDLER_ID,
                TAG_LIST_HANDLER_ID,
                TAG_CREATE_HANDLER_ID,
                TAG_DELETE_HANDLER_ID,
                ADD_HANDLER_ID,
                RESTORE_HANDLER_ID,
                SYNC_HANDLER_ID,
                COMMIT_HANDLER_ID,
                PUSH_HANDLER_ID,
            ],
            || GitCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for GitCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::Git)?;
        let mut input = GitCommandPlugin::build_input(ctx.command());

        // 破坏性操作必须显式确认，否则只输出计划，避免误删分支/标签。
        if lania_workflows::is_destructive(input.mode)
            && !option_bool(&ctx.command().argv.options, "yes").unwrap_or(false)
            && !input.dry_run
        {
            input.dry_run = true;
        }

        // `restore` 会丢弃未提交的改动，风险同样高，默认也只出计划。
        if matches!(input.mode, GitWorkflowMode::Restore)
            && !option_bool(&ctx.command().argv.options, "yes").unwrap_or(false)
            && !input.dry_run
        {
            input.dry_run = true;
        }

        ctx.emit_workflow_start("git").await;
        let execution = match ctx
            .run_workflow_with_tasks("git", "Git workflow", move |services| async move {
                let workflow = GitWorkflow;
                workflow.run(&services, input).await
            })
            .await
        {
            Ok(execution) => execution,
            Err(error) if error.to_string().contains("commitlint rejected commit message") => {
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
        ctx.emit_workflow_complete("git").await;
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

    use lania_workflows::GitWorkflowMode;

    use super::{
        GitCommandPlugin, ADD_HANDLER_ID, BRANCH_CREATE_HANDLER_ID, BRANCH_LIST_HANDLER_ID,
        DIFF_HANDLER_ID, HANDLER_ID, LOG_HANDLER_ID, PUSH_HANDLER_ID, RESTORE_HANDLER_ID,
        STATUS_HANDLER_ID, SYNC_HANDLER_ID, TAG_CREATE_HANDLER_ID,
    };

    fn setup_plugin() -> (CommandRegistryImpl, CommandHandlerRegistryImpl) {
        let plugin = GitCommandPlugin;
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
        (commands, handlers)
    }

    #[test]
    fn registers_git_spec_with_sync_folded_in() {
        let (commands, handlers) = setup_plugin();
        let spec = &commands.commands()[0];
        assert_eq!(spec.name, "git");
        assert_eq!(spec.handler_id, HANDLER_ID);

        let names: Vec<&str> = spec
            .subcommands
            .iter()
            .map(|child| child.name.as_str())
            .collect();
        // 原 lan sync 的四个动作必须完整保留在 git 下。
        for expected in ["sync", "commit", "push", "status"] {
            assert!(names.contains(&expected), "missing `git {expected}`");
        }
        for expected in ["log", "diff", "branch", "tag", "add", "restore"] {
            assert!(names.contains(&expected), "missing `git {expected}`");
        }

        for handler in [
            HANDLER_ID,
            STATUS_HANDLER_ID,
            LOG_HANDLER_ID,
            DIFF_HANDLER_ID,
            BRANCH_LIST_HANDLER_ID,
            BRANCH_CREATE_HANDLER_ID,
            TAG_CREATE_HANDLER_ID,
            ADD_HANDLER_ID,
            RESTORE_HANDLER_ID,
            SYNC_HANDLER_ID,
            PUSH_HANDLER_ID,
        ] {
            assert!(handlers.get(handler).is_some(), "handler {handler} missing");
        }
    }

    #[test]
    fn sync_subcommand_keeps_legacy_options() {
        let (commands, _) = setup_plugin();
        let sync = commands.commands()[0]
            .subcommands
            .iter()
            .find(|child| child.name == "sync")
            .expect("sync subcommand exists");
        let longs: Vec<&str> = sync.options.iter().map(|o| o.long.as_str()).collect();
        for expected in [
            "remote",
            "branch",
            "message",
            "amend",
            "force-with-lease",
            "dry-run",
            "interactive",
            "push",
        ] {
            assert!(longs.contains(&expected), "missing --{expected}");
        }
    }

    fn context(handler_id: &str, options: serde_json::Value, args: serde_json::Value) -> lania_command::CommandContext {
        lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: serde_json::from_value(args).expect("args object"),
                options: serde_json::from_value(options).expect("options object"),
            },
            handler_id: handler_id.into(),
            trace_id: "trace-git".into(),
        }
    }

    #[test]
    fn builds_sync_input_from_options() {
        let input = GitCommandPlugin::build_input(&context(
            SYNC_HANDLER_ID,
            json!({
                "remote": "upstream",
                "branch": "develop",
                "message": "feat(sync): ship",
                "push": false,
                "amend": true,
                "force-with-lease": true,
                "dry-run": true,
                "interactive": true,
            }),
            json!({}),
        ));
        assert_eq!(input.mode, GitWorkflowMode::Sync);
        assert_eq!(input.remote.as_deref(), Some("upstream"));
        assert_eq!(input.branch.as_deref(), Some("develop"));
        assert_eq!(input.message.as_deref(), Some("feat(sync): ship"));
        assert_eq!(input.push, Some(false));
        assert!(input.amend);
        assert!(input.force_with_lease);
        assert!(input.dry_run);
        assert!(input.interactive);
    }

    #[test]
    fn maps_handlers_to_modes_and_reads_positional_args() {
        let branch_input = GitCommandPlugin::build_input(&context(
            BRANCH_CREATE_HANDLER_ID,
            json!({}),
            json!({ "branch": "feature/x" }),
        ));
        assert_eq!(branch_input.mode, GitWorkflowMode::BranchCreate);
        assert_eq!(branch_input.branch.as_deref(), Some("feature/x"));

        let tag_input = GitCommandPlugin::build_input(&context(
            TAG_CREATE_HANDLER_ID,
            json!({ "message": "release 1.0" }),
            json!({ "tag": "v1.0.0" }),
        ));
        assert_eq!(tag_input.mode, GitWorkflowMode::TagCreate);
        assert_eq!(tag_input.tag.as_deref(), Some("v1.0.0"));
        assert_eq!(tag_input.message.as_deref(), Some("release 1.0"));

        let restore_input = GitCommandPlugin::build_input(&context(
            RESTORE_HANDLER_ID,
            json!({ "staged": true }),
            json!({ "paths": ["src", "README.md"] }),
        ));
        assert_eq!(restore_input.mode, GitWorkflowMode::Restore);
        assert!(restore_input.staged);
        assert_eq!(restore_input.paths, vec!["src", "README.md"]);

        let log_input = GitCommandPlugin::build_input(&context(
            LOG_HANDLER_ID,
            json!({ "limit": "5" }),
            json!({}),
        ));
        assert_eq!(log_input.mode, GitWorkflowMode::Log);
        assert_eq!(log_input.limit, Some(5));
    }
}
