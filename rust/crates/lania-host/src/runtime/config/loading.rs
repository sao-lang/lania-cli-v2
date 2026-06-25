use anyhow::{anyhow, Result};
use lania_config::{ConfigService, LanConfigSnapshot};
use lania_hooks::{hook_keys, HookRuntime};
use serde_json::{json, Value};

use crate::capability::{CapabilityName, CapabilityRegistrar, CapabilitySnapshot};

use super::super::HostRuntime;

impl HostRuntime {
    pub(in crate::runtime) fn bootstrap_host_capabilities(&mut self) {
        self.register_capability(CapabilityName::Logger, "Structured logger facade");
        self.register_capability(
            CapabilityName::Config,
            "Project and tool configuration facade",
        );
        self.register_capability(CapabilityName::Exec, "Process execution facade");
        self.register_capability(CapabilityName::Fs, "Filesystem planning and write facade");
        self.register_capability(CapabilityName::Task, "Task orchestration facade");
        self.register_capability(CapabilityName::Progress, "Progress reporting facade");
        self.register_capability(CapabilityName::Prompt, "Prompt orchestration facade");
        self.register_capability(CapabilityName::Git, "Git workflow facade");
        self.register_capability(
            CapabilityName::PackageManager,
            "Package manager detection and command planner",
        );
        self.register_capability(CapabilityName::Compiler, "Node-backed compiler facade");
        self.register_capability(CapabilityName::Lint, "Node-backed lint facade");
        self.register_capability(CapabilityName::Template, "Node-backed template facade");
        self.register_capability(CapabilityName::NodeBridge, "Node bridge client facade");
    }

    pub(super) fn register_capability(
        &mut self,
        name: CapabilityName,
        description: impl Into<String>,
    ) {
        self.state.capabilities.register(CapabilitySnapshot {
            name,
            provider: "host-runtime".into(),
            description: description.into(),
        });
    }

    pub fn load_lan_config_snapshot_from_cwd(
        &self,
        cwd: impl Into<String>,
    ) -> Result<LanConfigSnapshot> {
        let request = self.services.node_bridge.load_lan_config_request(cwd.into());
        let exchange = self.services.node_bridge.call(request);
        let payload = exchange
            .response
            .result
            .as_ref()
            .ok_or_else(|| anyhow!("config.loadLan returned no payload"))?;
        ConfigService::load_lan_snapshot(payload)
    }

    pub async fn load_lan_config_snapshot_from_cwd_async(
        &self,
        cwd: impl Into<String>,
    ) -> Result<LanConfigSnapshot> {
        let cwd = cwd.into();
        let config_get_payload = json!({
            "cwd": cwd,
            "traceId": "",
            "command": { "name": "", "handlerId": null },
            "config": {
                "searchFrom": cwd,
                "candidates": ["lan.config.js", "lan.config.cjs", "lan.config.json", "lan.config.ts"]
            }
        });
        let config_get_payload = self
            .state
            .hooks
            .call_waterfall(
                "host-runtime".into(),
                hook_keys::ON_CONFIG_GET.to_string(),
                config_get_payload,
            )
            .await
            .unwrap_or_else(|_| {
                json!({
                    "cwd": cwd,
                    "traceId": "",
                    "command": { "name": "", "handlerId": null },
                    "config": {
                        "searchFrom": cwd,
                        "candidates": ["lan.config.js", "lan.config.cjs", "lan.config.json", "lan.config.ts"]
                    }
                })
            });
        let search_from = config_get_payload
            .get("config")
            .and_then(|v| v.get("searchFrom"))
            .and_then(Value::as_str)
            .unwrap_or(&cwd);
        let candidates = config_get_payload
            .get("config")
            .and_then(|v| v.get("candidates"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![
                    "lan.config.js".into(),
                    "lan.config.cjs".into(),
                    "lan.config.json".into(),
                    "lan.config.ts".into(),
                ]
            });

        let exchange = self
            .services
            .node_bridge
            .call_async(self.services.node_bridge.request(
                "config.loadLan",
                json!({ "cwd": cwd, "searchFrom": search_from, "candidates": candidates }),
            ))
            .await?;
        let payload = exchange
            .response
            .result
            .as_ref()
            .ok_or_else(|| anyhow!("config.loadLan returned no payload"))?;
        let mut snapshot = ConfigService::load_lan_snapshot(payload)?;

        let resolved_payload = self
            .state
            .hooks
            .call_waterfall(
                "host-runtime".into(),
                hook_keys::ON_CONFIG_RESOLVE.to_string(),
                json!({
                    "cwd": cwd,
                    "traceId": "",
                    "command": { "name": "", "handlerId": null },
                    "config": {
                        "path": snapshot.config_path.clone(),
                        "value": snapshot.raw
                    }
                }),
            )
            .await
            .unwrap_or_else(|_| {
                json!({
                    "cwd": cwd,
                    "traceId": "",
                    "command": { "name": "", "handlerId": null },
                    "config": {
                        "path": snapshot.config_path.clone(),
                        "value": snapshot.raw
                    }
                })
            });

        if let Some(value) = resolved_payload
            .get("config")
            .and_then(|v| v.get("value"))
            .cloned()
        {
            let mut rebuilt_payload = payload.clone();
            if let Some(object) = rebuilt_payload.as_object_mut() {
                object.insert("config".into(), value);
            }
            snapshot = ConfigService::load_lan_snapshot(&rebuilt_payload)?;
        }

        Ok(snapshot)
    }
}
