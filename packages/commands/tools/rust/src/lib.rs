//! 注册 `tools` 命令，并将 `list` / `run` / `view` 子能力组织在同一插件下。

mod config;
mod list;
mod run;
mod shared;
mod view;

use anyhow::Result;
use async_trait::async_trait;
use lania_command::{CommandContext, CommandSpec, Example};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};

/// `tools` 根命令处理器标识。
pub const HANDLER_ID: &str = "command.tools";

/// 命令名，必须等于 `spec().name`（§5.4 D.1 冻结导出面 / §10.3 CI 断言）。
pub const COMMAND_NAME: &str = "tools";
/// `tools list` 子命令处理器标识。
pub const LIST_HANDLER_ID: &str = "command.tools.list";
/// `tools run` 子命令处理器标识。
pub const RUN_HANDLER_ID: &str = "command.tools.run";
/// `tools view` 子命令处理器标识。
pub const VIEW_HANDLER_ID: &str = "command.tools.view";

/// 将 `tools` 命令及其子命令注册到宿主命令系统。
#[derive(Debug, Default)]
pub struct ToolsCommandPlugin;

/// 命令双形态的唯一契约（§5.4 D.1）：总包 `lan` 与独立二进制 `lan-tools`
/// 都用它装配同一个插件。目标态返回 `Box<dyn lania_command::CommandPlugin>`；
/// 当前沿用宿主 `Plugin` trait，待类型完全下沉后平移签名。
pub fn factory() -> Box<dyn lania_host::plugin::Plugin> {
    Box::new(ToolsCommandPlugin)
}

/// 冻结导出面：`spec()` 作为自由函数，供 `--describe`、独立 bin 与
/// known-commands.json 派生使用；内部委托 `{plugin}::spec()`。
pub fn spec() -> lania_command::CommandSpec {
    ToolsCommandPlugin::spec()
}

struct ToolsCommandHandler;

impl ToolsCommandPlugin {
    /// 返回 `tools` 根命令规范，并保持原有子命令与选项结构不变。
    pub fn spec() -> CommandSpec {
        CommandSpec::new(
            "tools",
            "List commands, run files by type, or view local files",
            HANDLER_ID,
        )
        .with_options(list::options())
        .with_subcommands(vec![list::spec(), run::spec(), view::spec(), config::config_spec()])
        .with_examples(examples())
    }

    /// 基于解析后的命令上下文构造 `system.listCommands` bridge 请求。
    pub fn build_request(context: &CommandContext, bridge: &NodeBridgeClient) -> BridgeRequest {
        list::build_request(context, bridge)
    }
}

impl Plugin for ToolsCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-tools".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::NodeBridge,
                CapabilityName::Exec,
                CapabilityName::Fs,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        let config_ids = lania_command_config::handler_ids(COMMAND_NAME);
        let mut ids: Vec<&str> = vec![HANDLER_ID, LIST_HANDLER_ID, RUN_HANDLER_ID, VIEW_HANDLER_ID];
        ids.extend(config_ids.iter().map(String::as_str));
        register_builtin_command::<ToolsCommandHandler, _>(
            ctx,
            "command-tools",
            "tools",
            Self::spec(),
            &ids,
            || ToolsCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for ToolsCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let command = ctx.command();
        if lania_command_config::is_config_handler_id(COMMAND_NAME, &command.handler_id) {
            let value = lania_command_config::execute_config(
                COMMAND_NAME,
                &config::config_keys(),
                command,
            )?;
            return Ok(ctx.complete_template_info(value, 0));
        }
        match ctx.command().handler_id.as_str() {
            RUN_HANDLER_ID => run::execute(ctx),
            VIEW_HANDLER_ID => view::execute(ctx),
            _ => list::execute(ctx).await,
        }
    }
}

fn examples() -> Vec<Example> {
    vec![
        Example {
            command: "lan tools".into(),
            description: "List PATH commands plus shell builtins, aliases, and functions".into(),
        },
        Example {
            command: "lan tools --filter ts".into(),
            description: "List terminal commands whose names contain `ts`".into(),
        },
        Example {
            command: "lan tools --all-matches --filter node".into(),
            description: "Show all PATH matches for command names containing `node`".into(),
        },
        Example {
            command: "lan tools --names-only --no-shell".into(),
            description: "Return only PATH command names in a compact list".into(),
        },
        Example {
            command: "lan tools --group-by-source".into(),
            description: "Group commands by PATH, shell builtin, alias, and function".into(),
        },
        Example {
            command: "lan tools --plain --group-by-source".into(),
            description: "Render grouped command names as plain text".into(),
        },
        Example {
            command: "lan tools --plain --names-only --unique".into(),
            description: "Render one unique command name per line".into(),
        },
        Example {
            command: "lan tools run ./scripts/demo.ts -- --port 3000".into(),
            description: "Detect the file type and execute it with the matching runtime".into(),
        },
        Example {
            command: "lan tools view ./src/index.ts".into(),
            description: "Show file contents with line numbers or open media with the system app"
                .into(),
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

    use super::{ToolsCommandPlugin, HANDLER_ID, RUN_HANDLER_ID, VIEW_HANDLER_ID};

    #[test]
    fn registers_tools_command_spec() {
        let plugin = ToolsCommandPlugin;
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
        assert_eq!(spec.name, "tools");
        assert_eq!(spec.handler_id, HANDLER_ID);
        assert_eq!(spec.subcommands.len(), 3);
        assert_eq!(spec.subcommands[1].handler_id, RUN_HANDLER_ID);
        assert_eq!(spec.subcommands[2].handler_id, VIEW_HANDLER_ID);
        assert!(handlers.get(HANDLER_ID).is_some());
    }
}


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
        assert_eq!(spec().handler_id, "command.tools");
    }

    #[test]
    fn factory_builds_the_plugin() {
        // 只验证可构造与元信息非空，不触发宿主装配。
        let _plugin = factory();
    }
}
