use std::env;

use anyhow::Result;

#[path = "project_extensions_dynamic_commands.rs"]
mod dynamic_commands;
#[path = "project_extensions_hook_bindings.rs"]
mod hook_bindings;

use super::ProjectExtensionBootstrapSummary;
use super::super::HostRuntime;

impl HostRuntime {
    pub async fn bootstrap_project_extensions_from_cwd_async(
        &mut self,
        cwd: impl Into<String>,
    ) -> Result<ProjectExtensionBootstrapSummary> {
        let cwd = cwd.into();
        let runtime_mode = env::var("LANIA_RUNTIME_MODE").ok();
        let is_installed_mode = matches!(runtime_mode.as_deref(), Some("installed"));
        let product_root = env::var("LANIA_PRODUCT_ROOT")
            .ok()
            .filter(|value| !value.trim().is_empty() && (is_installed_mode || value != &cwd));
        let snapshot = if is_installed_mode {
            if let Some(product_root) = &product_root {
                self.load_lan_config_snapshot_from_cwd_async(product_root.clone())
                    .await?
            } else {
                self.load_lan_config_snapshot_from_cwd_async(cwd.clone()).await?
            }
        } else if let Some(product_root) = &product_root {
            self.load_lan_config_snapshot_from_cwd_async(product_root.clone())
                .await?
        } else {
            self.load_lan_config_snapshot_from_cwd_async(cwd.clone()).await?
        };
        let mut summary = ProjectExtensionBootstrapSummary::default();

        if snapshot.extensions.dynamic_commands {
            dynamic_commands::bootstrap_dynamic_commands(
                self,
                &cwd,
                product_root.as_ref(),
                is_installed_mode,
                &mut summary,
            )
            .await?;
        }

        if snapshot.extensions.dynamic_commands || !snapshot.hooks.is_empty() {
            hook_bindings::bootstrap_project_hook_bindings(self, &cwd, &snapshot, &mut summary)
                .await;
        }

        Ok(summary)
    }
}
