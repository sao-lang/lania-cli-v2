//! 将 `add` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 主要导出：spec、build_input、AddCommandPlugin、HANDLER_ID。
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
use lania_workflows::{generator_catalog, AddWorkflow, AddWorkflowInput};

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
        .with_args(vec![
            ArgSpec {
                name: "generator".into(),
                required: false,
                multiple: false,
                help: "Generator name, e.g. `resource` (NestJS) or `component`".into(),
            },
            // clap 要求 option 名与位置参数名唯一，而 `--name` 已经占用 `name`，
            // 因此位置参数用 `generator-name`，在 `build_input` 里映射回名称。
            ArgSpec {
                name: "generator-name".into(),
                required: false,
                multiple: false,
                help: "Base name used to derive class names and file paths".into(),
            },
        ])
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
                help: "Generator name or a legacy template name (rfc/v3/eslint/...)".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
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
                long: "list".into(),
                short: Some('l'),
                help: "List available generators grouped by framework, then exit".into(),
                value_kind: ValueKind::Bool,
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
                command: "lan add resource users".into(),
                description: "Generate a multi-file NestJS resource (module/controller/service/dto/entity/spec)"
                    .into(),
            },
            Example {
                command: "lan add component Button --target src/components".into(),
                description: "Add a framework-agnostic UI component".into(),
            },
            Example {
                command: "lan add --list".into(),
                description: "Show generators available for the detected framework".into(),
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
        let positional_generator = context
            .argv
            .args
            .get("generator")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let positional_name = context
            .argv
            .args
            .get("generator-name")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        AddWorkflowInput {
            cwd: context.cwd.clone().into(),
            name: context
                .argv
                .options
                .get("name")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .or(positional_name),
            template: context
                .argv
                .options
                .get("template")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .or(positional_generator),
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

    /// `lan add --list`：按框架分组输出生成器清单。
    ///
    /// 框架探测失败时**只列通用生成器**（§4.5.3），不猜框架。
    pub fn list_catalog(cwd: &std::path::Path, locale: &str) -> serde_json::Value {
        let catalog = generator_catalog(cwd, None);
        let framework = catalog["framework"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let generic = catalog["generic"].as_bool().unwrap_or(true);
        let generators = catalog["generators"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|generator| {
                let entry_framework = generator["framework"].as_str().unwrap_or_default();
                entry_framework.is_empty() || (!generic && entry_framework == framework)
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "kind": "template_info",
            "framework": if generic { serde_json::Value::Null } else { serde_json::json!(framework) },
            "frameworkReason": catalog["frameworkReason"],
            "locale": locale,
            "generators": generators,
        })
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
        let input = AddCommandPlugin::build_input(ctx.command());
        if ctx.bool_option("list") {
            let catalog = AddCommandPlugin::list_catalog(&input.cwd, ctx.locale());
            return Ok(ctx.complete_template_info(catalog, lania_host::EXIT_SUCCESS));
        }
        ctx.emit_workflow_start("add").await;
        let workflow = AddWorkflow;
        let execution = workflow.run(&ctx.workflow_services(), input).await?;
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
        assert_eq!(spec.options.len(), 6);
        assert_eq!(spec.args.len(), 2);
        let arg_names: Vec<&str> = spec.args.iter().map(|arg| arg.name.as_str()).collect();
        assert_eq!(arg_names, vec!["generator", "generator-name"]);
        // clap 要求 option 与 positional 名唯一（`--name` 与位置参数不能同名）。
        let mut all_names: Vec<&str> = spec
            .options
            .iter()
            .map(|option| option.long.as_str())
            .collect();
        all_names.extend(arg_names);
        let unique: std::collections::HashSet<&&str> = all_names.iter().collect();
        assert_eq!(all_names.len(), unique.len());
        assert!(handlers.get(HANDLER_ID).is_some());
    }

    #[test]
    fn add_spec_accepts_positional_generator_and_name() {
        let spec = AddCommandPlugin::spec();
        let names: Vec<&str> = spec.args.iter().map(|arg| arg.name.as_str()).collect();
        assert_eq!(names, vec!["generator", "generator-name"]);
    }

    #[test]
    fn builds_add_input_from_positional_args() {
        let context = lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: [
                    ("generator".into(), json!("resource")),
                    ("generator-name".into(), json!("users")),
                ]
                .into_iter()
                .collect(),
                options: Default::default(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace-add-positional".into(),
        };

        let input = AddCommandPlugin::build_input(&context);
        assert_eq!(input.template.as_deref(), Some("resource"));
        assert_eq!(input.name.as_deref(), Some("users"));
    }

    #[test]
    fn list_catalog_only_shows_generic_generators_without_a_framework() {
        let root = std::env::temp_dir().join(format!("lania-add-list-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temp dir");
        let catalog = AddCommandPlugin::list_catalog(&root, "en");
        let generators = catalog["generators"].as_array().expect("generators");
        let names: Vec<&str> = generators
            .iter()
            .filter_map(|generator| generator["name"].as_str())
            .collect();
        assert_eq!(names, vec!["component", "config"]);
        assert!(catalog["framework"].is_null());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn list_catalog_includes_framework_generators_when_detected() {
        let root = std::env::temp_dir().join(format!("lania-add-list-nest-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temp dir");
        std::fs::write(root.join("nest-cli.json"), "{}").expect("marker");
        let catalog = AddCommandPlugin::list_catalog(&root, "en");
        assert_eq!(catalog["framework"], "nestjs");
        let names: Vec<&str> = catalog["generators"]
            .as_array()
            .expect("generators")
            .iter()
            .filter_map(|generator| generator["name"].as_str())
            .collect();
        assert!(names.contains(&"resource"));
        assert!(names.contains(&"component"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn builds_add_input_from_context() {
        let context = lania_command::CommandContext {
            name: String::new(),
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
            name: String::new(),
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
            name: String::new(),
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
