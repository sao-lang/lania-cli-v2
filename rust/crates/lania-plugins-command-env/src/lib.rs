//! `lan env` 命令：多语言工具链环境的查询与管理。
//!
//! 子命令：
//! - `list`      查看所有受支持工具及其当前版本
//! - `info`      查看单个工具详情
//! - `versions`  列出某个管理器已安装的版本
//! - `install`   安装指定版本
//! - `use`       切换 / 固定版本（`--project` 写入项目版本文件）
//! - `remove`    卸载指定版本
//! - `doctor`    体检：缺失工具、PATH 冲突
//!
//! 执行语义：默认 dry-run 打印计划；只有 `--yes` 才真正执行。

mod list;
mod tui;

use anyhow::Result;
use async_trait::async_trait;
use lania_command::{CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_env::{build_plan, EnvAction, EnvPlan, EnvService, ManagerKind};
use lania_host::{
    capability::CapabilityName,
    execution::{
        CommandExecution, CommandExecutionContext, CommandHandler, ExecutionError,
        EXIT_RUNTIME_ERROR,
    },
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};
use serde_json::{json, Value};

pub const HANDLER_ID: &str = "command.env";
pub const LIST_HANDLER_ID: &str = "command.env.list";
pub const INFO_HANDLER_ID: &str = "command.env.info";
pub const VERSIONS_HANDLER_ID: &str = "command.env.versions";
pub const INSTALL_HANDLER_ID: &str = "command.env.install";
pub const USE_HANDLER_ID: &str = "command.env.use";
pub const REMOVE_HANDLER_ID: &str = "command.env.remove";
pub const DOCTOR_HANDLER_ID: &str = "command.env.doctor";
pub const UI_HANDLER_ID: &str = "command.env.ui";

#[derive(Debug, Default)]
pub struct EnvCommandPlugin;

struct EnvCommandHandler;

impl EnvCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new(
            "env",
            "Inspect and manage language toolchains (node/python/rust/go/dart/kotlin/java/c)",
            HANDLER_ID,
        )
        .with_subcommands(vec![
            list::list_spec(),
            list::info_spec(),
            list::versions_spec(),
            list::mutate_spec("install", "Install a toolchain version", INSTALL_HANDLER_ID),
            list::mutate_spec("use", "Switch or pin a toolchain version", USE_HANDLER_ID),
            list::mutate_spec("remove", "Uninstall a toolchain version", REMOVE_HANDLER_ID),
            list::doctor_spec(),
            ui_spec(),
        ])
        .with_options(vec![OptionSpec {
            long: "manager".into(),
            short: Some('m'),
            help: "Force a specific version manager (nvm/fnm/volta/pyenv/rustup/...)".into(),
            value_kind: ValueKind::OptionalString,
            default_value: None,
            choices: vec![],
            negatable: false,
        }])
        .with_examples(examples())
    }
}

impl Plugin for EnvCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-env".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![CapabilityName::Exec, CapabilityName::Fs],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<EnvCommandHandler, _>(
            ctx,
            "command-env",
            "env",
            Self::spec(),
            &[
                HANDLER_ID,
                LIST_HANDLER_ID,
                INFO_HANDLER_ID,
                VERSIONS_HANDLER_ID,
                INSTALL_HANDLER_ID,
                USE_HANDLER_ID,
                REMOVE_HANDLER_ID,
                DOCTOR_HANDLER_ID,
                UI_HANDLER_ID,
            ],
            || EnvCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for EnvCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let service = EnvService::new();
        let command = ctx.command();
        let cwd = std::path::PathBuf::from(&command.cwd);

        let result = match command.handler_id.as_str() {
            LIST_HANDLER_ID => Ok(list::execute_list(&service, &cwd, ctx)),
            INFO_HANDLER_ID => list::execute_info(&service, &cwd, ctx),
            VERSIONS_HANDLER_ID => list::execute_versions(&service, ctx),
            INSTALL_HANDLER_ID => execute_mutation(ctx, EnvAction::Install),
            USE_HANDLER_ID => execute_mutation(ctx, EnvAction::Use),
            REMOVE_HANDLER_ID => execute_mutation(ctx, EnvAction::Remove),
            DOCTOR_HANDLER_ID => Ok(list::execute_doctor(&service, &cwd)),
            UI_HANDLER_ID => execute_ui(ctx, &cwd),
            _ => Ok(json!({ "kind": "env_summary", "tools": service.scan(&cwd) })),
        }?;

        Ok(ctx.complete_template_info(result, 0))
    }
}

/// 启动终端 TUI 管理界面。阻塞直到用户退出（`q` / `Esc`）。
fn execute_ui(ctx: &CommandExecutionContext<'_>, cwd: &std::path::Path) -> Result<Value> {
    // `--snapshot` 输出单帧纯文本，便于在非 TTY 环境（CI / 测试）验证布局。
    if option_bool(ctx, "snapshot").unwrap_or(false) {
        let cols = option_string(ctx, "cols")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(100);
        let rows = option_string(ctx, "rows")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(28);
        // `--down N` 模拟按 N 次方向键，用于验证滚动窗口。
        let down = option_string(ctx, "down")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        // `--no-versions` 会被命令行解析器规范化成 `versions: false`，
        // 这里要按规范化后的键读取。
        let wait_versions = option_bool(ctx, "versions").unwrap_or(true);
        let frame = tui::snapshot_with(cwd.to_path_buf(), cols, rows, down, wait_versions)?;
        return Ok(json!({ "kind": "env_ui_snapshot", "frame": frame, "down": down }));
    }

    tui::run(cwd.to_path_buf())?;
    Ok(json!({ "kind": "env_ui", "status": "closed" }))
}

fn ui_spec() -> CommandSpec {
    CommandSpec::new(
        "ui",
        "Open the terminal UI for managing toolchains",
        UI_HANDLER_ID,
    )
    .with_options(vec![
        OptionSpec {
            long: "snapshot".into(),
            short: None,
            help: "Print one non-interactive frame instead of starting the TUI".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "cols".into(),
            short: None,
            help: "Snapshot width (default 100)".into(),
            value_kind: ValueKind::OptionalString,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "rows".into(),
            short: None,
            help: "Snapshot height (default 28)".into(),
            value_kind: ValueKind::OptionalString,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "down".into(),
            short: None,
            help: "Snapshot only: simulate pressing Down N times (scroll check)".into(),
            value_kind: ValueKind::OptionalString,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "no-versions".into(),
            short: None,
            help: "Snapshot only: skip version probing for a fast, layout-only frame".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
    ])
    .with_examples(vec![
        Example {
            command: "lan env ui".into(),
            description: "Open the two-pane terminal UI (arrows to move, i/u/d to act)".into(),
        },
        Example {
            command: "lan env ui --snapshot --plain".into(),
            description: "Render one frame without entering interactive mode".into(),
        },
    ])
}

fn execute_mutation(ctx: &CommandExecutionContext<'_>, action: EnvAction) -> Result<Value> {
    let command = ctx.command();
    let cwd = std::path::PathBuf::from(&command.cwd);
    let args = &command.argv.args;

    let tool = args.get("tool").and_then(Value::as_str).ok_or_else(|| {
        anyhow::anyhow!(
            "{} requires a tool, e.g. `lan env {} node`",
            action.as_str(),
            action.as_str()
        )
    })?;
    let version = args.get("version").and_then(Value::as_str);

    let manager = option_string(ctx, "manager")
        .map(|value| parse_manager(&value))
        .transpose()?;
    let project = option_bool(ctx, "project").unwrap_or(false);
    let yes = option_bool(ctx, "yes").unwrap_or(false);

    let plan = build_plan(action, tool, version, manager, &cwd, project);
    if let Some(error) = plan.error.as_deref() {
        return Err(ExecutionError {
            exit_code: EXIT_RUNTIME_ERROR,
            message: error.to_string(),
        }
        .into());
    }

    if !yes {
        return Ok(plan_value(&plan, "dry_run"));
    }

    ctx.require_capability(CapabilityName::Exec)?;
    let service = EnvService::new();
    let mut executed = Vec::new();
    for step in &plan.steps {
        if step.program == "write-file" {
            let path = &step.args[0];
            let content = &step.args[1];
            if content.is_empty() {
                let _ = std::fs::remove_file(path);
            } else {
                if let Some(parent) = std::path::Path::new(path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(path, format!("{content}\n"))?;
            }
            executed.push(json!({ "label": step.label, "command": step.command, "status": "ok" }));
            continue;
        }
        let output = service.run_step(ctx.exec(), &cwd, step)?;
        executed.push(json!({
            "label": step.label,
            "command": step.command,
            "status": "ok",
            "output": output.trim(),
        }));
    }

    let mut value = plan_value(&plan, "executed");
    value["executed"] = json!(executed);
    Ok(value)
}

fn plan_value(plan: &EnvPlan, status: &str) -> Value {
    let mut value = serde_json::to_value(plan).unwrap_or_else(|_| json!({}));
    if let Some(map) = value.as_object_mut() {
        map.insert("kind".into(), json!("env_plan"));
        map.insert("status".into(), json!(status));
        map.insert("dryRun".into(), json!(status == "dry_run"));
    }
    value
}

fn parse_manager(value: &str) -> Result<ManagerKind> {
    let normalized = value.trim().to_ascii_lowercase();
    let parsed = match normalized.as_str() {
        "nvm" => ManagerKind::Nvm,
        "fnm" => ManagerKind::Fnm,
        "volta" => ManagerKind::Volta,
        "n" => ManagerKind::N,
        "asdf" => ManagerKind::Asdf,
        "mise" => ManagerKind::Mise,
        "pyenv" => ManagerKind::Pyenv,
        "uv" => ManagerKind::Uv,
        "rustup" => ManagerKind::Rustup,
        "go" | "go-install" => ManagerKind::GoInstall,
        "fvm" => ManagerKind::Fvm,
        "sdkman" => ManagerKind::Sdkman,
        "scoop" => ManagerKind::Scoop,
        "winget" => ManagerKind::Winget,
        "brew" => ManagerKind::Brew,
        "apt" => ManagerKind::Apt,
        other => anyhow::bail!("unknown version manager `{other}`"),
    };
    Ok(parsed)
}

pub(crate) fn option_string(ctx: &CommandExecutionContext<'_>, key: &str) -> Option<String> {
    ctx.command()
        .argv
        .options
        .get(key)
        .and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
}

pub(crate) fn option_bool(ctx: &CommandExecutionContext<'_>, key: &str) -> Option<bool> {
    ctx.command().argv.options.get(key).and_then(Value::as_bool)
}

/// 供 `list` 子模块复用：从命令选项读取 manager 过滤。
pub(crate) fn manager_filter(ctx: &CommandExecutionContext<'_>) -> Option<ManagerKind> {
    option_string(ctx, "manager").and_then(|value| parse_manager(&value).ok())
}

/// 供 `list` 子模块复用：语言/工具名过滤。
pub(crate) fn tool_filter(command: &CommandContext) -> Option<String> {
    command
        .argv
        .args
        .get("tool")
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase())
}

fn examples() -> Vec<Example> {
    vec![
        Example {
            command: "lan env list".into(),
            description: "Show every supported toolchain with its detected version".into(),
        },
        Example {
            command: "lan env info node".into(),
            description: "Inspect node: binary path, manager, project pin, candidates".into(),
        },
        Example {
            command: "lan env versions node".into(),
            description: "List installed node versions managed by nvm/fnm/...".into(),
        },
        Example {
            command: "lan env install node 20 --yes".into(),
            description: "Install node 20 via the detected manager".into(),
        },
        Example {
            command: "lan env use python 3.12 --project".into(),
            description: "Pin python 3.12 into .python-version for this project".into(),
        },
        Example {
            command: "lan env remove node 18 --yes".into(),
            description: "Uninstall node 18".into(),
        },
        Example {
            command: "lan env doctor".into(),
            description: "Report missing toolchains and PATH conflicts".into(),
        },
        Example {
            command: "lan env ui".into(),
            description: "Open a local GUI to manage toolchains visually".into(),
        },
    ]
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

    use super::{
        EnvCommandPlugin, DOCTOR_HANDLER_ID, HANDLER_ID, INFO_HANDLER_ID, INSTALL_HANDLER_ID,
        LIST_HANDLER_ID, REMOVE_HANDLER_ID, UI_HANDLER_ID, USE_HANDLER_ID, VERSIONS_HANDLER_ID,
    };

    #[test]
    fn registers_env_command_spec() {
        let plugin = EnvCommandPlugin;
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
        assert_eq!(spec.name, "env");
        assert_eq!(spec.handler_id, HANDLER_ID);
        assert_eq!(spec.subcommands.len(), 8);

        let names: Vec<&str> = spec.subcommands.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["list", "info", "versions", "install", "use", "remove", "doctor", "ui"]
        );

        for handler in [
            HANDLER_ID,
            LIST_HANDLER_ID,
            INFO_HANDLER_ID,
            VERSIONS_HANDLER_ID,
            INSTALL_HANDLER_ID,
            USE_HANDLER_ID,
            REMOVE_HANDLER_ID,
            DOCTOR_HANDLER_ID,
            UI_HANDLER_ID,
        ] {
            assert!(handlers.get(handler).is_some(), "missing handler {handler}");
        }
    }

    #[test]
    fn mutate_subcommands_declare_args_and_flags() {
        let spec = EnvCommandPlugin::spec();
        let install = spec
            .subcommands
            .iter()
            .find(|sub| sub.name == "install")
            .expect("install subcommand");
        assert!(install
            .args
            .iter()
            .any(|arg| arg.name == "tool" && arg.required));
        assert!(install
            .args
            .iter()
            .any(|arg| arg.name == "version" && !arg.required));
        assert!(install.options.iter().any(|option| option.long == "yes"));
        assert!(install
            .options
            .iter()
            .any(|option| option.long == "manager"));
    }
}
