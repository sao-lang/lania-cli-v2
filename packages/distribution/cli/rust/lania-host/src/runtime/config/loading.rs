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
        let request = self
            .services
            .node_bridge
            .load_lan_config_request(cwd.into());
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
        // 候选文件名列表的**唯一权威**在 bridge 侧（`node-bridge/src/core/runtime.ts`
        // 的 `LAN_CONFIG_SEARCH_PLACES`）。这里刻意不传 `candidates`，
        // 让 bridge 用它自己的 7 元素默认列表 —— 否则 Rust 少列几个（曾经只有 4 个）
        // 会导致「同一个项目，Rust 路径找不到、直接调 bridge 却能找到」。
        let config_get_payload = json!({
            "cwd": cwd,
            "traceId": "",
            "command": { "name": "", "handlerId": null },
            "config": { "searchFrom": cwd }
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
                    "config": { "searchFrom": cwd }
                })
            });
        let search_from = config_get_payload
            .get("config")
            .and_then(|v| v.get("searchFrom"))
            .and_then(Value::as_str)
            .unwrap_or(&cwd);
        // hook 仍然可以显式增删候选项（这是显式扩展点）；缺省时保持 `null`，
        // bridge 侧 `Array.isArray(params.candidates) ? ... : undefined` 会回退到默认表。
        let candidates = config_get_payload
            .get("config")
            .and_then(|v| v.get("candidates"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            });

        let mut params = json!({ "cwd": cwd, "searchFrom": search_from });
        if let Some(candidates) = candidates {
            params["candidates"] = json!(candidates);
        }

        let exchange = self
            .services
            .node_bridge
            .call_async(self.services.node_bridge.request("config.loadLan", params))
            .await?;
        let payload = exchange
            .response
            .result
            .as_ref()
            .ok_or_else(|| anyhow!("config.loadLan returned no payload"))?;
        let mut snapshot = ConfigService::load_lan_snapshot(payload)?;
        // bridge 加载失败时不再静默返回空配置（§6.5.9）：把错误并入
        // `validation_errors`，CLI 层以 warning 展示，不阻断命令。
        snapshot
            .validation_errors
            .extend(load_error_validation_entries(payload));

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
            snapshot
                .validation_errors
                .extend(load_error_validation_entries(payload));
        }

        Ok(snapshot)
    }
}

/// 把 bridge 报告的「配置加载失败」翻译成 `ConfigValidationError`。
///
/// bridge 在 `result.loadError` / `result.validationErrors` 里给出原因；
/// Rust 侧只做一次格式转换，报错语义（code/message）由 bridge 决定。
fn load_error_validation_entries(payload: &Value) -> Vec<lania_config::ConfigValidationError> {
    let mut out = Vec::new();
    if let Some(errors) = payload.get("validationErrors").and_then(Value::as_array) {
        for entry in errors {
            let message = entry
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("failed to load lan.config")
                .to_string();
            out.push(lania_config::ConfigValidationError {
                code: lania_config::ConfigValidationErrorCode::InvalidValue,
                message,
                position: lania_config::ConfigPosition {
                    path: "$".into(),
                    line: None,
                    column: None,
                },
            });
        }
        return out;
    }
    if let Some(message) = payload.get("loadError").and_then(Value::as_str) {
        out.push(lania_config::ConfigValidationError {
            code: lania_config::ConfigValidationErrorCode::InvalidValue,
            message: format!("failed to load lan.config: {message}"),
            position: lania_config::ConfigPosition {
                path: "$".into(),
                line: None,
                column: None,
            },
        });
    }
    out
}
