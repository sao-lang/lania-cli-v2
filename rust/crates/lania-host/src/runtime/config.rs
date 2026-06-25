//! HostRuntime 与项目配置系统之间的连接层。
//!
//! 现在按职责拆成 3 层：
//! - `summary`: 运行时快照与 manifest 视图
//! - `loading`: 配置读取与宿主 capability 引导
//! - `project_extensions`: 项目级动态命令、hook 与扩展 bootstrap

mod loading;
mod project_extensions;
mod summary;

use lania_command::CommandSpec;
use lania_config::{ConfigPluginRef, LanConfigSnapshot};
use lania_hooks::HookSnapshot;
use serde::Serialize;

use crate::plugin::{LifecyclePhase, NodePluginMeta};

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectExtensionBootstrapSummary {
    pub dynamic_commands: usize,
    pub dynamic_handlers: usize,
    pub lifecycle_hooks: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSummary {
    pub plugins: Vec<crate::PluginMeta>,
    pub commands: Vec<CommandSpec>,
    pub hooks: HookSnapshot,
    pub capabilities: Vec<crate::CapabilitySnapshot>,
    pub lifecycle: Vec<LifecyclePhase>,
    pub node_plugins: Vec<NodePluginMeta>,
    pub project_config: Option<LanConfigSnapshot>,
    pub project_node_plugins: Vec<NodePluginMeta>,
    pub project_plugin_report: ProjectPluginReport,
    pub manifest: PluginManifest,
    pub handshake: lania_node_bridge::HandshakeRequest,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginManifest {
    pub rust_plugins: Vec<crate::PluginMeta>,
    pub node_plugins: Vec<NodePluginMeta>,
    pub project_config: Option<LanConfigSnapshot>,
    pub project_node_plugins: Vec<NodePluginMeta>,
    pub project_plugin_report: ProjectPluginReport,
    pub hook_registrations: Vec<crate::HookRegistration>,
    pub supported_events: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectPluginReport {
    pub accepted_plugins: Vec<ConfigPluginRef>,
    pub review_required_plugins: Vec<ConfigPluginRef>,
    pub rejected_plugins: Vec<ConfigPluginRef>,
}

fn collect_command_names(commands: &[CommandSpec]) -> Vec<String> {
    fn visit(spec: &CommandSpec, names: &mut Vec<String>) {
        names.push(spec.name.clone());
        for child in &spec.subcommands {
            visit(child, names);
        }
    }

    let mut names = Vec::new();
    for command in commands {
        visit(command, &mut names);
    }
    names
}

pub(super) fn node_plugin_metas_from_config(plugins: &[ConfigPluginRef]) -> Vec<NodePluginMeta> {
    plugins
        .iter()
        .filter(|plugin| plugin.loadable)
        .map(|plugin| NodePluginMeta {
            name: plugin.name.clone(),
            package: plugin.package.clone(),
            methods: plugin.methods.clone(),
        })
        .collect()
}

pub(super) fn project_plugin_report_from_config(config: &LanConfigSnapshot) -> ProjectPluginReport {
    let mut report = ProjectPluginReport::default();
    for plugin in &config.plugins {
        if plugin.is_rejected() {
            report.rejected_plugins.push(plugin.clone());
        } else if plugin.requires_review() {
            report.review_required_plugins.push(plugin.clone());
            report.accepted_plugins.push(plugin.clone());
        } else {
            report.accepted_plugins.push(plugin.clone());
        }
    }
    report
}
