//! `generate` 命令插件的命令规格定义。
//!
//! 这个文件只回答一个问题：CLI 长什么样。
//! - 顶层命令有哪些子命令
//! - 每个子命令支持哪些 option / example
//! - 解析后的参数应该路由到哪个 handler id
//!
//! 真正的业务执行在 workflow 层；这里更像“命令行协议声明”，告诉 `lania-command`
//! 如何把用户输入的文本命令解析成结构化 `CommandContext`。

use lania_command::{CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_workflows::{
    GenerateApiMode, GenerateApiWorkflowInput, GenerateModuleMode, GenerateModuleWorkflowInput,
};

use crate::{
    split_csv, API_DIFF_HANDLER_ID, API_HANDLER_ID, API_INIT_HANDLER_ID, API_PLAN_HANDLER_ID,
    HANDLER_ID, MODULE_APPLY_HANDLER_ID, MODULE_DIFF_HANDLER_ID, MODULE_HANDLER_ID,
    MODULE_INIT_HANDLER_ID, MODULE_PLAN_HANDLER_ID, PRODUCT_HANDLER_ID,
};

use super::GenerateCommandPlugin;

impl GenerateCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new("generate", "Generate project artifacts", HANDLER_ID).with_examples(vec![
            Example {
                command: "lan product generate --name \"Acme CLI\" --binary-name acme".into(),
                description: "Scaffold a new CLI product workspace".into(),
            },
            Example {
                command: "lan generate api".into(),
                description: "Generate contract and transport artifacts from lania.contract.yaml"
                    .into(),
            },
            Example {
                command: "lan g api --source proto --target grpc,http --dry-run".into(),
                description: "Preview protobuf grpc/http generation without writing files".into(),
            },
            Example {
                command: "lan generate module".into(),
                description: "Generate lania-g module files from lania.module.yaml".into(),
            },
        ])
    }

    pub fn product_spec() -> CommandSpec {
        CommandSpec::new(
            "product",
            "Generate a scaffolded CLI product workspace",
            PRODUCT_HANDLER_ID,
        )
        .with_options(vec![
            OptionSpec {
                long: "preset".into(),
                short: None,
                help: "Scaffold preset (demo or minimal)".into(),
                value_kind: ValueKind::OptionalString,
                default_value: Some("demo".into()),
                choices: vec!["demo".into(), "minimal".into()],
                negatable: false,
            },
            OptionSpec {
                long: "interactive".into(),
                short: Some('i'),
                help: "Run interactive prompts when generating a product scaffold".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "name".into(),
                short: Some('n'),
                help: "Display name for the generated product".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "binary-name".into(),
                short: Some('b'),
                help: "Binary name used by the generated product".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "package-name".into(),
                short: Some('p'),
                help: "Package name written to the generated package.json".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "output-dir".into(),
                short: Some('o'),
                help: "Output directory for the generated product scaffold".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "force".into(),
                short: Some('f'),
                help: "Overwrite generated files if the destination already exists".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
        ])
        .with_examples(vec![Example {
            command: "lan product generate --preset demo --interactive".into(),
            description: "Create a product workspace scaffold under products/acme-cli".into(),
        }])
    }

    pub fn product_generate_spec() -> CommandSpec {
        let mut spec = Self::product_spec();
        spec.name = "generate".into();
        spec.about = "Generate a scaffolded CLI product workspace".into();
        spec.examples = vec![Example {
            command: "lan product generate --preset demo --interactive".into(),
            description: "Create a product workspace scaffold under products/acme-cli".into(),
        }];
        spec
    }

    pub fn api_spec() -> CommandSpec {
        CommandSpec::new(
            "api",
            "Generate contract DTOs and transport wrappers",
            API_HANDLER_ID,
        )
        .with_options(vec![
            OptionSpec {
                long: "config".into(),
                short: Some('c'),
                help: "Contract config path".into(),
                value_kind: ValueKind::String,
                default_value: Some("lania.contract.yaml".into()),
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "source".into(),
                short: Some('s'),
                help: "Source kind filter, e.g. proto or thrift".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "target".into(),
                short: Some('t'),
                help: "Target filter, e.g. grpc,http".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "entry".into(),
                short: Some('e'),
                help: "Entry filter, supports comma separated names".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "manifest".into(),
                short: Some('m'),
                help: "Manifest path override".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "dry-run".into(),
                short: Some('d'),
                help: "Plan generation without writing files".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "check".into(),
                short: Some('k'),
                help: "Validate generated outputs without writing files".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "clean".into(),
                short: None,
                help: "Remove stale generated files tracked by the manifest".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "force".into(),
                short: Some('f'),
                help: "Overwrite unmanaged conflicting files".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
        ])
        .with_examples(vec![
            Example {
                command: "lan generate api".into(),
                description: "Generate all configured contract outputs".into(),
            },
            Example {
                command: "lan generate api --entry user-service --target http".into(),
                description: "Generate only the selected entry and target".into(),
            },
            Example {
                command: "lan generate api check".into(),
                description: "Fail when generated outputs are out of date".into(),
            },
        ])
        .with_subcommands(vec![
            Self::api_plan_spec(),
            Self::api_diff_spec(),
            Self::api_init_spec(),
        ])
    }

    pub fn api_plan_spec() -> CommandSpec {
        CommandSpec::new(
            "plan",
            "Preview generated files, conflicts, and cleanup actions",
            API_PLAN_HANDLER_ID,
        )
    }

    pub fn api_diff_spec() -> CommandSpec {
        CommandSpec::new(
            "diff",
            "Compare the current manifest with the planned outputs",
            API_DIFF_HANDLER_ID,
        )
    }

    pub fn api_init_spec() -> CommandSpec {
        CommandSpec::new(
            "init",
            "Bootstrap lania.contract.yaml and example schema files",
            API_INIT_HANDLER_ID,
        )
        .with_options(vec![OptionSpec {
            long: "force".into(),
            short: Some('f'),
            help: "Overwrite existing bootstrap files".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        }])
    }

    pub fn module_spec() -> CommandSpec {
        CommandSpec::new(
            "module",
            "Generate lania-g modules and main.go injection assets",
            MODULE_HANDLER_ID,
        )
        .with_options(vec![
            OptionSpec {
                long: "config".into(),
                short: Some('c'),
                help: "Module config path".into(),
                value_kind: ValueKind::String,
                default_value: Some("lania.module.yaml".into()),
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "input".into(),
                short: Some('i'),
                help: "Limit generation to one input directory or file".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "source".into(),
                short: Some('s'),
                help: "Source kind filter, e.g. proto or thrift".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "target".into(),
                short: Some('t'),
                help: "Target filter, e.g. grpc,http".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "framework".into(),
                short: None,
                help: "Framework backend name".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "entry".into(),
                short: Some('e'),
                help: "Entry filter, supports comma separated names".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "main".into(),
                short: None,
                help: "Override main.go injection target".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "module-name".into(),
                short: None,
                help: "Override the generated module name for a single entry".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "package".into(),
                short: None,
                help: "Override the Go import path used by generated main.go helpers".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "manifest".into(),
                short: Some('m'),
                help: "Manifest path override".into(),
                value_kind: ValueKind::OptionalString,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "dry-run".into(),
                short: Some('d'),
                help: "Plan generation without writing files".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "check".into(),
                short: Some('k'),
                help: "Validate generated outputs without writing files".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "clean".into(),
                short: None,
                help: "Remove stale generated files tracked by the manifest".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "force".into(),
                short: Some('f'),
                help: "Overwrite unmanaged conflicting files".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "no-inject".into(),
                short: None,
                help: "Skip main.go injection and helper generation".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
        ])
        .with_examples(vec![
            Example {
                command: "lan generate module".into(),
                description: "Generate all configured lania-g module outputs".into(),
            },
            Example {
                command: "lan generate module --entry user --target grpc --check".into(),
                description: "Check module generation drift for one entry".into(),
            },
            Example {
                command: "lan generate module apply --no-inject".into(),
                description: "Write module outputs without touching main.go".into(),
            },
        ])
        .with_subcommands(vec![
            Self::module_plan_spec(),
            Self::module_diff_spec(),
            Self::module_init_spec(),
            Self::module_apply_spec(),
        ])
    }

    pub fn module_plan_spec() -> CommandSpec {
        CommandSpec::new(
            "plan",
            "Preview generated module files, injection changes, and cleanup actions",
            MODULE_PLAN_HANDLER_ID,
        )
    }

    pub fn module_diff_spec() -> CommandSpec {
        CommandSpec::new(
            "diff",
            "Compare the current module manifest with the planned outputs",
            MODULE_DIFF_HANDLER_ID,
        )
    }

    pub fn module_init_spec() -> CommandSpec {
        CommandSpec::new(
            "init",
            "Bootstrap lania.module.yaml and example schema files",
            MODULE_INIT_HANDLER_ID,
        )
        .with_options(vec![OptionSpec {
            long: "force".into(),
            short: Some('f'),
            help: "Overwrite existing bootstrap files".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        }])
    }

    pub fn module_apply_spec() -> CommandSpec {
        CommandSpec::new(
            "apply",
            "Explicitly write module outputs and main.go injection assets",
            MODULE_APPLY_HANDLER_ID,
        )
    }

    pub fn build_api_input(context: &CommandContext) -> GenerateApiWorkflowInput {
        GenerateApiWorkflowInput {
            cwd: context.cwd.clone().into(),
            config_path: context
                .argv
                .options
                .get("config")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            manifest_path: context
                .argv
                .options
                .get("manifest")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            source_filter: split_csv(
                context
                    .argv
                    .options
                    .get("source")
                    .and_then(|value| value.as_str()),
            ),
            target_filter: split_csv(
                context
                    .argv
                    .options
                    .get("target")
                    .and_then(|value| value.as_str()),
            ),
            entry_filter: split_csv(
                context
                    .argv
                    .options
                    .get("entry")
                    .and_then(|value| value.as_str()),
            ),
            dry_run: context
                .argv
                .options
                .get("dry-run")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            check: context
                .argv
                .options
                .get("check")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            clean: context
                .argv
                .options
                .get("clean")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            force: context
                .argv
                .options
                .get("force")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            mode: match context.handler_id.as_str() {
                API_PLAN_HANDLER_ID => GenerateApiMode::Plan,
                API_DIFF_HANDLER_ID => GenerateApiMode::Diff,
                API_INIT_HANDLER_ID => GenerateApiMode::Init,
                _ => GenerateApiMode::Apply,
            },
        }
    }

    pub fn build_module_input(context: &CommandContext) -> GenerateModuleWorkflowInput {
        GenerateModuleWorkflowInput {
            cwd: context.cwd.clone().into(),
            config_path: context
                .argv
                .options
                .get("config")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            manifest_path: context
                .argv
                .options
                .get("manifest")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            input_path: context
                .argv
                .options
                .get("input")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            source_filter: split_csv(
                context
                    .argv
                    .options
                    .get("source")
                    .and_then(|value| value.as_str()),
            ),
            target_filter: split_csv(
                context
                    .argv
                    .options
                    .get("target")
                    .and_then(|value| value.as_str()),
            ),
            entry_filter: split_csv(
                context
                    .argv
                    .options
                    .get("entry")
                    .and_then(|value| value.as_str()),
            ),
            framework: context
                .argv
                .options
                .get("framework")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            main_path: context
                .argv
                .options
                .get("main")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            module_name: context
                .argv
                .options
                .get("module-name")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            package_name: context
                .argv
                .options
                .get("package")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            dry_run: context
                .argv
                .options
                .get("dry-run")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            check: context
                .argv
                .options
                .get("check")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            clean: context
                .argv
                .options
                .get("clean")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            force: context
                .argv
                .options
                .get("force")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            no_inject: context
                .argv
                .options
                .get("no-inject")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            mode: match context.handler_id.as_str() {
                MODULE_PLAN_HANDLER_ID => GenerateModuleMode::Plan,
                MODULE_DIFF_HANDLER_ID => GenerateModuleMode::Diff,
                MODULE_INIT_HANDLER_ID => GenerateModuleMode::Init,
                MODULE_APPLY_HANDLER_ID => GenerateModuleMode::Apply,
                _ => GenerateModuleMode::Apply,
            },
        }
    }
}
