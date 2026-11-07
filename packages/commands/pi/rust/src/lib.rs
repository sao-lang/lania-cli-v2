//! `lan pi`：把独立产品 `lania-agent` 接进 lan CLI 的**适配层**。
//!
//! 边界（与 `lania-plugins-command-env` 对 `runex` 的关系一致）：
//! - `lania-agent` 是完整产品，不认识 lan 的命令框架，可独立运行；
//! - 本 crate 只做三件事：声明命令 → 解析宿主上下文 → 注入传输与审批通道；
//! - 反向依赖禁止：`lania-agent` 里不出现本 crate 的名字。

mod config;
mod bridge;
mod handler;
mod options;

use anyhow::Result;
use lania_command::{ArgSpec, CommandSpec, Example, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};

pub const HANDLER_ID: &str = "command.pi";

#[derive(Debug, Default)]
pub struct PiCommandPlugin;

/// 命令双形态的唯一契约（§5.4 D.1）：总包 `lan` 与独立二进制 `lan-pi`
/// 都用它装配同一个插件。目标态返回 `Box<dyn lania_command::CommandPlugin>`；
/// 当前沿用宿主 `Plugin` trait，待类型完全下沉后平移签名。
pub fn factory() -> Box<dyn lania_host::plugin::Plugin> {
    Box::new(PiCommandPlugin)
}

/// 冻结导出面：`spec()` 作为自由函数，供 `--describe`、独立 bin 与
/// known-commands.json 派生使用；内部委托 `{plugin}::spec()`。
pub fn spec() -> lania_command::CommandSpec {
    PiCommandPlugin::spec()
}

impl PiCommandPlugin {
    pub fn spec() -> CommandSpec {
        let flag = |long: &str, short: Option<char>, help: &str| OptionSpec {
            long: long.into(),
            short,
            help: help.into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        };
        let value = |long: &str, short: Option<char>, help: &str, choices: Vec<&str>| OptionSpec {
            long: long.into(),
            short,
            help: help.into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: choices.into_iter().map(ToOwned::to_owned).collect(),
            negatable: false,
        };

        CommandSpec::new(
            "pi",
            "Run the coding agent (interactive REPL or one-shot)",
            HANDLER_ID,
        )
        .with_alias("agent")
        .with_args(vec![ArgSpec {
            name: "prompt".into(),
            required: false,
            multiple: true,
            help: "Prompt text; when present the agent runs one-shot".into(),
        }])
        .with_options(vec![
            value(
                "model",
                Some('m'),
                "Model id (e.g. claude-sonnet-4-5); defaults to lan.config agent.model",
                vec![],
            ),
            value("provider", None, "Provider id (anthropic/openai/...)", vec![]),
            value(
                "thinking",
                None,
                "Reasoning level",
                vec!["minimal", "low", "medium", "high", "xhigh", "max"],
            ),
            value(
                "approval-mode",
                None,
                "Tool approval mode: always-ask | write | yolo",
                vec!["always-ask", "write", "yolo"],
            ),
            flag(
                "print",
                Some('p'),
                "One-shot mode: read the prompt from stdin when no positional argument is given",
            ),
            flag("continue", Some('c'), "Resume the most recent session"),
            value(
                "resume",
                None,
                "Resume the session with this id (see --sessions)",
                vec![],
            ),
            flag(
                "new",
                None,
                "Start a new session even if --continue is configured",
            ),
            flag("no-session", None, "Do not persist the conversation to .lan/agent"),
            flag("sessions", None, "List sessions in this workspace and exit"),
            flag(
                "json",
                None,
                "Emit one JSON object per event (machine readable)",
            ),
            flag(
                "no-tui",
                None,
                "Force the line-based REPL instead of the full-screen TUI",
            ),
            flag(
                "snapshot",
                None,
                "Render TUI frames to stdout and exit (for CI / no-TTY review)",
            ),
            flag("verbose", None, "Show extra diagnostics"),
        ])
        .with_subcommand(config::config_spec())
        .with_examples(vec![
            Example {
                command: "lan pi".into(),
                description: "Start an interactive session".into(),
            },
            Example {
                command: "lan pi \"explain the rust crate layout\"".into(),
                description: "One-shot prompt".into(),
            },
            Example {
                command: "lan pi -c".into(),
                description: "Continue the latest session".into(),
            },
            Example {
                command: "lan pi --sessions".into(),
                description: "List saved sessions".into(),
            },
            Example {
                command: "lan pi --approval-mode yolo \"run the test suite\"".into(),
                description: "One-shot without approval prompts".into(),
            },
        ])
    }
}

impl Plugin for PiCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-pi".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Exec,
                CapabilityName::Fs,
                CapabilityName::NodeBridge,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        let config_ids = lania_command_config::handler_ids(COMMAND_NAME);
        let mut ids: Vec<&str> = vec![HANDLER_ID];
        ids.extend(config_ids.iter().map(String::as_str));
        register_builtin_command::<handler::PiCommandHandler, _>(
            ctx,
            "command-pi",
            "pi",
            Self::spec(),
            &ids,
            || handler::PiCommandHandler,
        )
    }
}

/// 命令名（供文档生成与测试复用）。
pub const COMMAND_NAME: &str = "pi";
/// 兼容旧入口：`lan agent` 仍然可用。
pub const LEGACY_ALIAS: &str = "agent";


#[cfg(test)]
mod frozen_export_surface {
    //! 冻结导出面守卫（§5.4 D.1 / §10.3 CI 断言）。
    use super::*;

    #[test]
    fn command_name_matches_spec() {
        assert_eq!(spec().name, COMMAND_NAME);
    }

    #[test]
    fn handler_id_is_frozen() {
        assert_eq!(spec().handler_id, "command.pi");
    }

    #[test]
    fn factory_builds_the_plugin() {
        // 只验证可构造与元信息非空，不触发宿主装配。
        let _plugin = factory();
    }
}
