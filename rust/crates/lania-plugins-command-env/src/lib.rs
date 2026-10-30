//! `lan env` 命令：多语言工具链环境的查询与管理。
//!
//! 子命令：
//! - `list`       查看所有受支持工具及其当前版本（`--installed` 只看 lania 自管版本）
//! - `info`       查看单个工具详情
//! - `versions`   列出某个语言已安装的版本
//! - `install`    从上游下载并安装指定版本
//! - `use`        切换 current 指针（`--project` 写入项目版本文件）
//! - `remove`     卸载由 lania 管理的版本
//! - `which`      输出某个入口命令对应的真实路径（shim 使用）
//! - `init`       打印把 `~/.runex/shims` 加入 PATH 的 shell 片段
//! - `doctor`     体检：缺失工具、shim 与 PATH 一致性
//! - `ui`         终端 TUI
//!
//! 执行语义：默认 dry-run 打印计划；只有 `--yes` 才真正执行。
//! 安装/切换/卸载全部由 `runex` 完成，不转发给 nvm / volta 等外部管理器；
//! 本 crate 只保留 lania 宿主的命令注册、参数、门控与输出。

mod list;

use anyhow::Result;
use async_trait::async_trait;
use lania_command::{CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{
        CommandExecution, CommandExecutionContext, CommandHandler, ExecutionError,
        EXIT_RUNTIME_ERROR,
    },
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};
use runex::api::{EnvAction, EnvPlan, Runex, ShimHost};
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
pub const WHICH_HANDLER_ID: &str = "command.env.which";
pub const INIT_HANDLER_ID: &str = "command.env.init";

#[derive(Debug, Default)]
pub struct EnvCommandPlugin;

struct EnvCommandHandler;

impl EnvCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new(
            "env",
            "Install and switch language runtimes managed by lania itself (node/python/go/rust/deno/bun/zig/java/dart/kotlin/scala)",
            HANDLER_ID,
        )
        .with_subcommands(vec![
            list::list_spec(),
            list::info_spec(),
            list::versions_spec(),
            list::mutate_spec("install", "Download and install a runtime version", INSTALL_HANDLER_ID),
            list::mutate_spec("use", "Switch the current runtime version", USE_HANDLER_ID),
            list::mutate_spec("remove", "Delete a lania-managed runtime version", REMOVE_HANDLER_ID),
            list::doctor_spec(),
            which_spec(),
            init_spec(),
            ui_spec(),
        ])
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
                WHICH_HANDLER_ID,
                INIT_HANDLER_ID,
            ],
            || EnvCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for EnvCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let command = ctx.command();
        let cwd = std::path::PathBuf::from(&command.cwd);
        // 宿主只负责命令注册与输出；全部运行时能力都走 runex 的 Rust API。
        let api = runtime_api(&cwd)?;

        let result = match command.handler_id.as_str() {
            LIST_HANDLER_ID => Ok(list::execute_list(&api, ctx)),
            INFO_HANDLER_ID => list::execute_info(&api, ctx),
            VERSIONS_HANDLER_ID => list::execute_versions(&api, ctx),
            INSTALL_HANDLER_ID => execute_mutation(&api, ctx, EnvAction::Install),
            USE_HANDLER_ID => execute_mutation(&api, ctx, EnvAction::Use),
            REMOVE_HANDLER_ID => execute_mutation(&api, ctx, EnvAction::Remove),
            DOCTOR_HANDLER_ID => list::execute_doctor(&api),
            WHICH_HANDLER_ID => execute_which(&api, ctx),
            INIT_HANDLER_ID => execute_init(&api, ctx),
            UI_HANDLER_ID => execute_ui(&api, ctx),
            _ => Ok(json!({ "kind": "env_summary", "tools": api.scan() })),
        }?;

        Ok(ctx.complete_template_info(result, 0))
    }
}

fn which_spec() -> CommandSpec {
    CommandSpec::new(
        "which",
        "Print the binary a managed entrypoint currently resolves to",
        WHICH_HANDLER_ID,
    )
    .with_args(vec![lania_command::ArgSpec {
        name: "name".into(),
        required: true,
        multiple: false,
        help: "Entrypoint name, e.g. node / npm / python3 / go".into(),
    }])
    .with_options(vec![OptionSpec {
        long: "resolve".into(),
        short: None,
        help: "Print only the resolved absolute path (used by shims)".into(),
        value_kind: ValueKind::Bool,
        default_value: None,
        choices: vec![],
        negatable: false,
    }])
}

fn init_spec() -> CommandSpec {
    CommandSpec::new(
        "init",
        "Print the shell snippet that puts lania's shims directory on PATH",
        INIT_HANDLER_ID,
    )
    .with_options(vec![OptionSpec {
        long: "shell".into(),
        short: Some('s'),
        help: "Target shell: bash / zsh / fish / powershell (default: auto-detect)".into(),
        value_kind: ValueKind::OptionalString,
        default_value: None,
        choices: vec![],
        negatable: false,
    }])
}

/// `lan env which <name>`：给 shim 用的入口解析。
fn execute_which(api: &Runex, ctx: &CommandExecutionContext<'_>) -> Result<Value> {
    let name = ctx
        .command()
        .argv
        .args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("env which requires a name, e.g. `lan env which node`"))?;
    let path = api.which(name).map_err(|error| ExecutionError {
        exit_code: EXIT_RUNTIME_ERROR,
        message: error.to_string(),
    })?;
    let resolve_only = option_bool(ctx, "resolve").unwrap_or(false);
    let path_text = path.display().to_string();
    Ok(json!({
        "kind": "env_which",
        "name": name,
        "path": path_text,
        "resolve": resolve_only,
    }))
}

/// `lan env init`：只做一件事——把 shims 目录放进 PATH 的说明/片段。
fn execute_init(api: &Runex, ctx: &CommandExecutionContext<'_>) -> Result<Value> {
    let shell = option_string(ctx, "shell").unwrap_or_else(detect_shell);
    let snippet = api.init_snippet(&shell);
    Ok(json!({
        "kind": "env_init",
        "shell": shell,
        "shims": api.home().shims_dir().display().to_string(),
        "snippet": snippet,
    }))
}

/// 按运行环境猜测 shell；猜不到时给 POSIX 片段（最能覆盖多数场景）。
fn detect_shell() -> String {
    if cfg!(windows) {
        "powershell".to_string()
    } else if std::env::var("SHELL")
        .map(|value| value.contains("fish"))
        .unwrap_or(false)
    {
        "fish".to_string()
    } else {
        "bash".to_string()
    }
}

/// 启动终端 TUI 管理界面。阻塞直到用户退出（`q` / `Esc`）。
fn execute_ui(api: &Runex, ctx: &CommandExecutionContext<'_>) -> Result<Value> {
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
        let frame = api
            .tui_snapshot(cols, rows, down, wait_versions, ctx.locale())
            .map_err(|error| ExecutionError {
                exit_code: EXIT_RUNTIME_ERROR,
                message: error.to_string(),
            })?;
        return Ok(json!({ "kind": "env_ui_snapshot", "frame": frame, "down": down }));
    }

    api.run_tui(ctx.locale()).map_err(|error| ExecutionError {
        exit_code: EXIT_RUNTIME_ERROR,
        message: error.to_string(),
    })?;
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
            command: "lan env ui --snapshot".into(),
            description: "Render one frame without entering interactive mode".into(),
        },
    ])
}

fn execute_mutation(
    api: &Runex,
    ctx: &CommandExecutionContext<'_>,
    action: EnvAction,
) -> Result<Value> {
    let command = ctx.command();
    let args = &command.argv.args;

    let tool = args.get("tool").and_then(Value::as_str).ok_or_else(|| {
        anyhow::anyhow!(
            "{} requires a tool, e.g. `lan env {} node`",
            action.as_str(),
            action.as_str()
        )
    })?;
    let version = args.get("version").and_then(Value::as_str);

    let project = option_bool(ctx, "project").unwrap_or(false);
    let yes = option_bool(ctx, "yes").unwrap_or(false);

    let plan = api.plan(action, tool, version, project);
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
    let executed = api
        .apply(&plan, false, |_step| {})
        .map_err(|error| ExecutionError {
            exit_code: EXIT_RUNTIME_ERROR,
            message: error.to_string(),
        })?;

    let mut value = plan_value(&plan, "executed");
    value["executed"] = json!(executed
        .into_iter()
        .map(|step| json!({
            "label": step.label,
            "command": step.command,
            "status": step.status,
            "output": step.output,
        }))
        .collect::<Vec<_>>());
    Ok(value)
}

/// 构造 runex API：宿主只做一层「路径 + 错误文本」适配，不复制任何运行时逻辑。
pub(crate) fn runtime_api(cwd: &std::path::Path) -> Result<Runex> {
    Runex::resolve()
        // `lan env` 生成的 shim 必须继续调用 `lan env which`，而不是 `runex which`。
        .map(|api| api.with_cwd(cwd).with_shim_host(ShimHost::lan()))
        .map_err(|error| anyhow::anyhow!(error.to_string()))
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
            command: "lan env init".into(),
            description: "Print the snippet that puts ~/.runex/shims on PATH".into(),
        },
        Example {
            command: "lan env install node 20 --yes".into(),
            description: "Download node 20.x from nodejs.org and register it".into(),
        },
        Example {
            command: "lan env use node 20".into(),
            description: "Point the node current pointer at 20.x and regenerate shims".into(),
        },
        Example {
            command: "lan env list --installed".into(),
            description: "List only the versions lania itself installed".into(),
        },
        Example {
            command: "lan env info node".into(),
            description: "Inspect node: binary path, source, upstream, project pin".into(),
        },
        Example {
            command: "lan env which node".into(),
            description: "Print the binary the node shim currently resolves to".into(),
        },
        Example {
            command: "lan env use python 3.12 --project".into(),
            description: "Pin python 3.12 into .python-version for this project".into(),
        },
        Example {
            command: "lan env remove node 18 --yes".into(),
            description: "Delete the lania-managed node 18 directory".into(),
        },
        Example {
            command: "lan env doctor".into(),
            description: "Report missing runtimes, shim gaps and PATH problems".into(),
        },
        Example {
            command: "lan env ui".into(),
            description: "Open the terminal UI to manage runtimes visually".into(),
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
        EnvCommandPlugin, DOCTOR_HANDLER_ID, HANDLER_ID, INFO_HANDLER_ID, INIT_HANDLER_ID,
        INSTALL_HANDLER_ID, LIST_HANDLER_ID, REMOVE_HANDLER_ID, UI_HANDLER_ID, USE_HANDLER_ID,
        VERSIONS_HANDLER_ID, WHICH_HANDLER_ID,
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
        assert_eq!(spec.subcommands.len(), 10);

        let names: Vec<&str> = spec.subcommands.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "list", "info", "versions", "install", "use", "remove", "doctor", "which", "init",
                "ui"
            ]
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
            WHICH_HANDLER_ID,
            INIT_HANDLER_ID,
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
        // `--manager` 逃生舱已移除：lan env 只有自管一条路径。
        assert!(!install
            .options
            .iter()
            .any(|option| option.long == "manager"));
    }
}
