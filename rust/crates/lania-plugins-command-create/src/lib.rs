//! 将 `create` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 主要导出：spec、build_input、CreateCommandPlugin、HANDLER_ID。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use anyhow::Result;
use async_trait::async_trait;
use lania_command::{ArgSpec, CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};
use lania_logger::{render_ascii_banner, LogLevel};
use lania_workflows::{CreateWorkflow, CreateWorkflowInput};
use std::io::IsTerminal;

pub const HANDLER_ID: &str = "command.create";

#[derive(Debug, Default)]
pub struct CreateCommandPlugin;

struct CreateCommandHandler;

impl CreateCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new("create", "Create a new project from a template", HANDLER_ID)
            .with_args(vec![ArgSpec {
                name: "path".into(),
                required: false,
                multiple: false,
                help: "Project path (use \".\" for current directory)".into(),
            }])
            .with_options(vec![
                OptionSpec {
                    long: "name".into(),
                    short: Some('n'),
                    help: "Project name".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "template".into(),
                    short: Some('t'),
                    help: "Template name".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![
                        "spa-react".into(),
                        "spa-vue".into(),
                        "toolkit".into(),
                        "toolkit-monorepo".into(),
                    ],
                    negatable: false,
                },
                OptionSpec {
                    long: "package-manager".into(),
                    short: Some('p'),
                    help: "Package manager".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec!["npm".into(), "pnpm".into(), "yarn".into(), "bun".into()],
                    negatable: false,
                },
                // Legacy compatibility: create in a child directory (maps to v2 path semantics).
                OptionSpec {
                    long: "directory".into(),
                    short: None,
                    help: "Legacy alias for project path (create inside the given directory)"
                        .into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "git".into(),
                    short: Some('g'),
                    help: "Initialize git repository".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: true,
                },
                // Legacy compatibility: invert of --git (matches legacy create behavior).
                OptionSpec {
                    long: "skip-git".into(),
                    short: None,
                    help: "Legacy alias for --no-git".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                // Legacy compatibility: skip package manager steps.
                OptionSpec {
                    long: "skip-install".into(),
                    short: None,
                    help: "Skip package manager init/install steps".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                // Legacy compatibility: allow templates to branch by language (e.g. js/ts).
                OptionSpec {
                    long: "language".into(),
                    short: None,
                    help: "Preferred project language (forwarded to template context)".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "dry-run".into(),
                    short: Some('d'),
                    help: "Plan file writes and install commands without executing".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "preview".into(),
                    short: Some('P'),
                    help: "Preview rendered template files and imply dry-run".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
            ])
            .with_examples(vec![
                Example {
                    command: "lan create --name demo-app".into(),
                    description: "Create a default react application".into(),
                },
                Example {
                    command: "lan create .".into(),
                    description: "Create a project in current directory".into(),
                },
                Example {
                    command: "lan create --template toolkit --package-manager pnpm".into(),
                    description: "Create a toolkit template with pnpm".into(),
                },
                Example {
                    command: "lan create --template spa-vue --preview".into(),
                    description: "Preview the rendered template files without writing".into(),
                },
            ])
    }

    pub fn build_input(context: &CommandContext) -> CreateWorkflowInput {
        let directory = context
            .argv
            .options
            .get("directory")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let path = directory.or_else(|| {
            context
                .argv
                .args
                .get("path")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
        });
        let skip_git = context
            .argv
            .options
            .get("skip-git")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let skip_install = context
            .argv
            .options
            .get("skip-install")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let init_git = if skip_git {
            false
        } else {
            // Legacy default: initialize git unless explicitly disabled.
            context
                .argv
                .options
                .get("git")
                .and_then(|value| value.as_bool())
                .unwrap_or(true)
        };

        CreateWorkflowInput {
            cwd: context.cwd.clone().into(),
            path,
            project_name: context
                .argv
                .options
                .get("name")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            template: context
                .argv
                .options
                .get("template")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            package_manager: context
                .argv
                .options
                .get("package-manager")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            language: context
                .argv
                .options
                .get("language")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            init_git,
            skip_install,
            skip_install_specified: skip_install,
            dry_run: context
                .argv
                .options
                .get("dry-run")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            preview: context
                .argv
                .options
                .get("preview")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        }
    }
}

impl Plugin for CreateCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-create".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Prompt,
                CapabilityName::Fs,
                CapabilityName::PackageManager,
                CapabilityName::Git,
                CapabilityName::Task,
                CapabilityName::Progress,
                CapabilityName::Template,
                CapabilityName::NodeBridge,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<CreateCommandHandler, _>(
            ctx,
            "command-create",
            "create",
            Self::spec(),
            &[HANDLER_ID],
            || CreateCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for CreateCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::Template)?;
        ctx.emit_workflow_start("create").await;
        let input = CreateCommandPlugin::build_input(ctx.command());
        let mut execution = ctx
            .run_workflow_with_tasks("create", "Create workflow", move |services| async move {
                let workflow = CreateWorkflow;
                workflow.run(&services, input).await
            })
            .await?;

        let logger = ctx.workflow_services().logger.scoped("command.create");
        let package_manager = execution
            .prompts
            .get("packageManager")
            .and_then(|value| value.as_str())
            .unwrap_or("npm");
        match execution.state {
            lania_workflows::WorkflowState::Completed => {
                let interactive_rendered =
                    std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
                execution.interactive_rendered = interactive_rendered;
                let banner_text = execution
                    .prompts
                    .get("projectName")
                    .and_then(|value| value.as_str())
                    .unwrap_or("lania");
                if interactive_rendered {
                    let banner = render_ascii_banner(banner_text).join("\n");
                    logger.log(LogLevel::Info, format!("\n\x1b[35;1m{banner}\x1b[0m\n"));
                }
                logger.log(
                    LogLevel::Info,
                    format!("project created in {}", execution.target_dir),
                );
                logger.log(LogLevel::Info, "next steps:".to_string());
                logger.log(LogLevel::Info, format!("  cd {}", execution.target_dir));
                logger.log(LogLevel::Info, format!("  {package_manager} run dev"));
            }
            lania_workflows::WorkflowState::Planned => {
                logger.log(
                    LogLevel::Info,
                    format!("create plan prepared for {}", execution.target_dir),
                );
            }
            _ => {}
        }
        ctx.emit_workflow_complete("create").await;
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

    use super::{CreateCommandPlugin, HANDLER_ID};

    #[test]
    fn registers_create_spec() {
        let plugin = CreateCommandPlugin;
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
        assert_eq!(spec.name, "create");
        assert_eq!(spec.handler_id, HANDLER_ID);
        assert_eq!(spec.options.len(), 10);
        assert!(handlers.get(HANDLER_ID).is_some());
    }

    #[test]
    fn builds_create_input_from_context() {
        let context = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("name".into(), json!("demo-app")),
                    ("template".into(), json!("toolkit")),
                    ("package-manager".into(), json!("pnpm")),
                    ("git".into(), json!(true)),
                    ("language".into(), json!("ts")),
                    ("skip-install".into(), json!(true)),
                    ("dry-run".into(), json!(true)),
                    ("preview".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace-create".into(),
        };

        let input = CreateCommandPlugin::build_input(&context);
        assert_eq!(input.project_name.as_deref(), Some("demo-app"));
        assert_eq!(input.template.as_deref(), Some("toolkit"));
        assert_eq!(input.package_manager.as_deref(), Some("pnpm"));
        assert_eq!(input.language.as_deref(), Some("ts"));
        assert!(input.init_git);
        assert!(input.skip_install);
        assert!(input.skip_install_specified);
        assert!(input.dry_run);
        assert!(input.preview);
    }

    #[test]
    fn prefers_directory_option_over_dot_path_and_skip_git() {
        let context = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: [("path".into(), json!("."))].into_iter().collect(),
                options: [
                    ("directory".into(), json!("foo")),
                    ("skip-git".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace-create-directory".into(),
        };

        let input = CreateCommandPlugin::build_input(&context);
        assert_eq!(input.path.as_deref(), Some("foo"));
        assert!(!input.init_git);
        assert!(!input.skip_install);
        assert!(!input.skip_install_specified);
    }
}
