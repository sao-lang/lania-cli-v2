//! lan.config 与工具配置快照的规范化结构，以及插件声明模型。
//!
//! 主要导出：requires_review、is_rejected、lan_schema_doc、lan_schema_markdown、load_lan_snapshot、load_tool_snapshot。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use anyhow::{anyhow, Result};
use serde_json::Value;

mod discovery;
mod normalize;
mod validate;

const CURRENT_LAN_CONFIG_VERSION: u32 = 1;

mod models;
mod service;

mod parse_plugins;
mod parse_release;
mod parse_snapshots;
mod parse_utils;

mod validate_lan;
mod validate_release;
mod validate_sections;
mod validate_utils;

mod version;

pub use models::*;
pub use service::ConfigService;

// Internal helpers used by normalize/validate modules.
//
// 这个 crate 的公开 API 非常薄：
// - 外部通常只需要 `load_*_snapshot()` 和 schema 文档能力
// - 具体的 parse / normalize / validate 细节都尽量藏在模块内部
//
// 这样上层 crate 就不会过度耦合配置实现细节，后续演进 schema 也更容易。
pub(crate) use validate_utils::{
    as_string_vec, validate_string_array, validate_string_object, validate_type,
};

pub fn requires_review(plugin: &ConfigPluginRef) -> bool {
    plugin.requires_review()
}

pub fn is_rejected(plugin: &ConfigPluginRef) -> bool {
    plugin.is_rejected()
}

pub fn lan_schema_doc() -> LanConfigSchemaDoc {
    ConfigService::lan_schema_doc()
}

pub fn lan_schema_markdown() -> String {
    ConfigService::lan_schema_markdown()
}

pub fn load_lan_snapshot(payload: &Value) -> Result<LanConfigSnapshot> {
    // 这些自由函数只是对 `ConfigService` 的轻包装，
    // 目的是让调用方既能用 service 风格，也能直接用 crate-level API。
    ConfigService::load_lan_snapshot(payload)
}

pub fn load_tool_snapshot(payload: &Value) -> Result<ToolConfigSnapshot> {
    ConfigService::load_tool_snapshot(payload)
}

// Parsing helpers exposed for normalize/validate submodules.

#[cfg(test)]
mod tests;
