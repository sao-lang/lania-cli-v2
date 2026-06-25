//! 插件元数据、生命周期阶段与 setup 上下文定义。
//!
//! 主要导出：PluginMeta、NodePluginMeta、PluginSetupContext、EmptyPlugin、LifecyclePhase、PluginKind。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use anyhow::Result;
use lania_command::CommandSpec;
use serde::{Deserialize, Serialize};

use crate::{
    CapabilityRegistrar, CommandHandler, CommandRegistry, HandlerRegistry, HookRuntime,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMeta {
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    pub requires: Vec<crate::CapabilityName>,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhase {
    Discover,
    Resolve,
    Load,
    Setup,
    RuntimeStart,
    CommandExecute,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePluginMeta {
    pub name: String,
    pub package: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Rust,
    Node,
}

pub struct PluginSetupContext<'a> {
    pub commands: &'a mut dyn CommandRegistry,
    pub hooks: &'a mut dyn HookRuntime,
    pub capabilities: &'a mut dyn CapabilityRegistrar,
    pub handlers: &'a mut dyn HandlerRegistry,
}

pub trait Plugin: Send + Sync {
    fn meta(&self) -> PluginMeta;
    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()>;
}

pub fn register_builtin_command<H, F>(
    ctx: &mut PluginSetupContext<'_>,
    plugin_name: &str,
    command_name: &str,
    spec: CommandSpec,
    handler_ids: &[&str],
    make_handler: F,
) -> Result<()>
where
    H: CommandHandler + 'static,
    F: Fn() -> H,
{
    let handlers = handler_ids
        .iter()
        .map(|handler_id| (*handler_id, Box::new(make_handler()) as Box<dyn CommandHandler>))
        .collect::<Vec<_>>();
    register_builtin_command_handlers(ctx, plugin_name, command_name, spec, handlers)
}

pub fn record_builtin_command_registration(
    ctx: &mut PluginSetupContext<'_>,
    plugin_name: &str,
    command_name: &str,
) {
    ctx.hooks.record_event(
        plugin_name.to_string(),
        crate::hook_keys::ON_COMMAND_REGISTER.to_string(),
        serde_json::json!({
            "cwd": "",
            "traceId": "",
            "command": { "name": "", "handlerId": null },
            "registry": { "command": command_name, "source": "builtin" }
        }),
    );
}

pub fn register_builtin_command_handlers(
    ctx: &mut PluginSetupContext<'_>,
    plugin_name: &str,
    command_name: &str,
    spec: CommandSpec,
    handlers: Vec<(&str, Box<dyn CommandHandler>)>,
) -> Result<()> {
    ctx.commands.register(spec)?;
    record_builtin_command_registration(ctx, plugin_name, command_name);
    for (handler_id, handler) in handlers {
        ctx.handlers.register(handler_id, handler)?;
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct EmptyPlugin;

impl Plugin for EmptyPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "empty".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, _ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        Ok(())
    }
}
