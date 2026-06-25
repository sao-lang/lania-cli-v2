use std::path::Path;

use anyhow::{anyhow, Result};
use lania_config::ConfigService;
use lania_node_bridge::ConfigBridgeCapability;
use serde_json::Value;

use crate::models::WorkflowServices;

#[derive(Debug, Clone)]
pub(crate) struct AddTemplateContextSnapshot {
    pub(crate) project_name: String,
    pub(crate) language: String,
    pub(crate) css_processor: String,
}

pub(crate) async fn load_add_template_context(
    services: &WorkflowServices,
    cwd: &Path,
) -> Result<AddTemplateContextSnapshot> {
    let exchange = services
        .bridge
        .load_lan_config(cwd.display().to_string())
        .await?;
    let payload = exchange
        .response
        .result
        .as_ref()
        .ok_or_else(|| anyhow!("config.loadLan returned no payload"))?;
    let snapshot = ConfigService::load_lan_snapshot(payload)?;
    let raw = snapshot.raw.as_object().cloned().unwrap_or_default();
    let language = match raw.get("language").and_then(Value::as_str) {
        Some("JavaScript") | Some("javascript") | Some("js") => "js",
        _ => "ts",
    };
    Ok(AddTemplateContextSnapshot {
        project_name: raw
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("lania-app")
            .to_string(),
        language: language.to_string(),
        css_processor: raw
            .get("cssProcessor")
            .and_then(Value::as_str)
            .unwrap_or("css")
            .to_string(),
    })
}
