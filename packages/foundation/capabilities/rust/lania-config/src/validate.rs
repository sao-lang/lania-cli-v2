//! 配置校验入口的薄封装。
//!
//! 这个文件本身很薄，价值在于把外部调用统一收口到两个入口：
//! - `validate_lan_config`
//! - `validate_tool_config`
//!
//! 具体规则分散在 `validate_lan.rs`、`validate_sections.rs`、`validate_release.rs`、
//! `validate_utils.rs` 等文件中。

use serde_json::Value;

use super::ConfigValidationError;

pub(crate) fn validate_lan_config(raw: &Value, schema_version: u32) -> Vec<ConfigValidationError> {
    crate::validate_lan::validate_lan_config(raw, schema_version)
}

pub(crate) fn validate_tool_config(raw: &Value) -> Vec<ConfigValidationError> {
    crate::validate_utils::validate_tool_config(raw)
}
