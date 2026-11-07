use std::env;

use anyhow::Result;
use lania_config::LanConfigSnapshot;
use lania_hooks::HookRuntime;

use crate::{
    capability::CapabilityResolver,
    registry::{CommandRegistry, PluginRegistry},
};

use super::{
    super::HostRuntime, node_plugin_metas_from_config, project_plugin_report_from_config,
    PluginManifest, RuntimeSummary,
};

impl HostRuntime {
    pub fn summary(&self) -> RuntimeSummary {
        let project_config = env::current_dir().ok().and_then(|cwd| {
            self.load_lan_config_snapshot_from_cwd(cwd.display().to_string())
                .ok()
        });
        self.summary_from_project_config(project_config)
    }

    pub fn summary_for_cwd(&self, cwd: impl Into<String>) -> Result<RuntimeSummary> {
        let project_config = self.load_lan_config_snapshot_from_cwd(cwd).ok();
        Ok(self.summary_from_project_config(project_config))
    }

    fn summary_from_project_config(
        &self,
        project_config: Option<LanConfigSnapshot>,
    ) -> RuntimeSummary {
        let project_node_plugins = project_config
            .as_ref()
            .map(|config| node_plugin_metas_from_config(&config.plugins))
            .unwrap_or_default();
        let project_plugin_report = project_config
            .as_ref()
            .map(project_plugin_report_from_config)
            .unwrap_or_default();
        RuntimeSummary {
            plugins: self.registries.plugin_registry.metas(),
            commands: self.registries.commands.commands().to_vec(),
            hooks: self.state.hooks.snapshot(),
            capabilities: self.state.capabilities.all(),
            lifecycle: self.lifecycle_phases(),
            node_plugins: self.node_plugin_metas(),
            project_config: project_config.clone(),
            project_node_plugins: project_node_plugins.clone(),
            project_plugin_report: project_plugin_report.clone(),
            manifest: PluginManifest {
                rust_plugins: self.registries.plugin_registry.metas(),
                node_plugins: self.node_plugin_metas(),
                project_config,
                project_node_plugins,
                project_plugin_report,
                hook_registrations: self.state.hooks.snapshot().registrations,
                supported_events: self
                    .services
                    .node_bridge
                    .supported_events()
                    .into_iter()
                    .map(|event| format!("{:?}", event).to_lowercase())
                    .collect(),
            },
            handshake: self.handshake_preview(),
        }
    }
}
