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

/// 本文支持的七个运行时 id（与 bridge 的 `toolchain.ts` / `RuntimeKind` 一致）。
///
/// 放在 `lania-config` 内而不是依赖 `runex-core`：配置校验不应引入下载/解压依赖树。
pub fn known_runtime_ids() -> &'static [&'static str] {
    &["node", "bun", "deno", "python", "go", "rust", "dart"]
}

/// `lan.config` 允许的顶层字段（validate 与 `lan config set/sync` 共用一份）。
///
/// 单一来源：`validate_lan.rs` 的白名单从这里取，`lan config set` 写盘前也用它校验，
/// 避免「校验认、写入不认」或反过来。
pub fn known_config_fields() -> &'static [&'static str] {
    validate_lan::ALLOWED_TOP_LEVEL_FIELDS
}

/// `lan.config` 当前 schema 版本（`config init` / `sync` 写盘时使用）。
pub fn current_config_version() -> u32 {
    CURRENT_LAN_CONFIG_VERSION
}

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
