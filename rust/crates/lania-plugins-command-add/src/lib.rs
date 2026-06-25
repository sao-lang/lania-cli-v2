//! 将 `add` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 主要导出：spec、build_input、AddCommandPlugin、HANDLER_ID。
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
use lania_workflows::{AddWorkflow, AddWorkflowInput};

pub const HANDLER_ID: &str = "command.add";

#[derive(Debug, Default)]
pub struct AddCommandPlugin;

struct AddCommandHandler;

impl AddCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new(
            "add",
            "Add generated content into an existing workspace",
            HANDLER_ID,
        )
        .with_options(vec![
            OptionSpec {
                long: "name".into(),
                short: Some('n'),
                help: "Base name when adding a generated component or module".into(),
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
                    "v2".into(),
                    "v3".into(),
                    "rcc".into(),
                    "rfc".into(),
                    "svelte".into(),
                    "astro".into(),
                    "prettier".into(),
                    "eslint".into(),
                    "stylelint".into(),
                    "editorconfig".into(),
                    "gitignore".into(),
                    "tsconfig".into(),
                    "commitizen".into(),
                ],
                negatable: false,
            },
            OptionSpec {
                long: "target".into(),
                short: Some('d'),
                help: "Relative target path or directory".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            // Legacy compatibility: accept the old option name for target paths.
            OptionSpec {
                long: "filepath".into(),
                short: None,
                help: "Legacy alias for target path or directory".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "force".into(),
                short: Some('f'),
                help: "Overwrite conflicting files".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
        ])
        .with_examples(vec![
            Example {
                command: "lan add --name button".into(),
                description: "Add a default toolkit module".into(),
            },
            Example {
                command: "lan add --name dashboard --template spa-react --target scaffolds".into(),
                description: "Add a scaffold into a custom directory".into(),
            },
        ])
    }

    pub fn build_input(context: &CommandContext) -> AddWorkflowInput {
        let legacy_filepath = context
            .argv
            .options
            .get("filepath")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        AddWorkflowInput {
            cwd: context.cwd.clone().into(),
            name: context
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
            target: context
                .argv
                .options
                .get("target")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .or(legacy_filepath),
            force: context
                .argv
                .options
                .get("force")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        }
    }
}

impl Plugin for AddCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-add".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Fs,
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
        register_builtin_command::<AddCommandHandler, _>(
            ctx,
            "command-add",
            "add",
            Self::spec(),
            &[HANDLER_ID],
            || AddCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for AddCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::Template)?;
        ctx.emit_workflow_start("add").await;
        let workflow = AddWorkflow;
        let execution = workflow
            .run(
                &ctx.workflow_services(),
                AddCommandPlugin::build_input(ctx.command()),
            )
            .await?;
        ctx.emit_workflow_complete("add").await;
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

    use super::{AddCommandPlugin, HANDLER_ID};

    #[test]
    fn registers_add_spec() {
        let plugin = AddCommandPlugin;
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
        assert_eq!(spec.name, "add");
        assert_eq!(spec.handler_id, HANDLER_ID);
        assert_eq!(spec.options.len(), 5);
        assert!(handlers.get(HANDLER_ID).is_some());
    }

    #[test]
    fn builds_add_input_from_context() {
        let context = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("name".into(), json!("button")),
                    ("template".into(), json!("spa-vue")),
                    ("target".into(), json!("packages")),
                    ("force".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace-add".into(),
        };

        let input = AddCommandPlugin::build_input(&context);
        assert_eq!(input.name.as_deref(), Some("button"));
        assert_eq!(input.template.as_deref(), Some("spa-vue"));
        assert_eq!(input.target.as_deref(), Some("packages"));
        assert!(input.force);
    }

    #[test]
    fn prefers_target_and_accepts_legacy_filepath() {
        let filepath_only = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("filepath".into(), json!("/src/components"))]
                    .into_iter()
                    .collect(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace-add-filepath".into(),
        };
        let filepath_input = AddCommandPlugin::build_input(&filepath_only);
        assert_eq!(filepath_input.target.as_deref(), Some("/src/components"));

        let target_wins = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("filepath".into(), json!("/src/legacy")),
                    ("target".into(), json!("src/current")),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace-add-target".into(),
        };
        let target_input = AddCommandPlugin::build_input(&target_wins);
        assert_eq!(target_input.target.as_deref(), Some("src/current"));
    }
}
