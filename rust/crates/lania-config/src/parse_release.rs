//! `release` 配置分区的解析器（Value -> 强类型 config）。
//!
//! 它和 `validate_release.rs` 的区别是：
//! - validate：发现问题并返回 `ConfigValidationError`
//! - parse：尽量容错地把配置解析成结构体，并提供合理默认值
//!
//! 解析结果最终会被 `parse_snapshots.rs` / `normalize.rs` 装配进 `LanConfigSnapshot.release`。

use crate::{
    ReleaseDeployConfig, ReleaseGitConfig, ReleasePostCheckConfig, ReleaseProfile,
    ReleaseStepConfig, ReleaseVerifyConfig, ReleaseVersioningConfig, Value,
};

type JsonObject = serde_json::Map<String, Value>;

fn object_value<'a>(object: &'a JsonObject, key: &str) -> Option<&'a Value> {
    object.get(key)
}

pub(crate) fn parse_release_profile(value: &str) -> Option<ReleaseProfile> {
    match value.trim().to_ascii_lowercase().as_str() {
        "package" => Some(ReleaseProfile::Package),
        "web-app" | "web_app" => Some(ReleaseProfile::WebApp),
        "service" => Some(ReleaseProfile::Service),
        "custom" => Some(ReleaseProfile::Custom),
        _ => None,
    }
}

pub(crate) fn parse_release_step_config(
    value: Option<&Value>,
    default_enabled: bool,
) -> ReleaseStepConfig {
    // release 中很多“步骤”支持三种写法：
    // - `true/false`：只表达开关
    // - `"npm test"`：简写成命令字符串
    // - `{ enabled, command }`：完整对象写法
    //
    // parser 的职责是把这三种输入统一折叠成同一个强类型结构。
    match value {
        Some(Value::Bool(enabled)) => ReleaseStepConfig {
            enabled: *enabled,
            command: None,
        },
        Some(Value::String(command)) => ReleaseStepConfig {
            enabled: true,
            command: Some(command.clone()),
        },
        Some(Value::Object(object)) => ReleaseStepConfig {
            enabled: object_value(object, "enabled")
                .and_then(Value::as_bool)
                .unwrap_or(default_enabled || object_value(object, "command").is_some()),
            // 如果对象里给了 command，但没显式给 enabled，
            // 这里会把它视为“既然配置了命令，那这个步骤默认就是启用的”。
            command: object_value(object, "command")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        },
        _ => ReleaseStepConfig {
            enabled: default_enabled,
            command: None,
        },
    }
}

pub(crate) fn parse_release_verify_config(value: Option<&Value>) -> ReleaseVerifyConfig {
    let object = value.and_then(Value::as_object);
    ReleaseVerifyConfig {
        lint: parse_release_step_config(object.and_then(|map| object_value(map, "lint")), false),
        test: parse_release_step_config(object.and_then(|map| object_value(map, "test")), false),
        build: parse_release_step_config(object.and_then(|map| object_value(map, "build")), false),
        smoke: parse_release_step_config(object.and_then(|map| object_value(map, "smoke")), false),
    }
}

pub(crate) fn parse_release_versioning_config(value: Option<&Value>) -> ReleaseVersioningConfig {
    let mut config = ReleaseVersioningConfig::default();
    if let Some(object) = value.and_then(Value::as_object) {
        // 这里大量使用 `unwrap_or(config.xxx)` / `.or(config.xxx)`，
        // 本质是在说：“只覆盖用户显式提供的字段，其余沿用默认配置”。
        config.enabled = object_value(object, "enabled")
            .and_then(Value::as_bool)
            .unwrap_or(config.enabled);
        config.source = object_value(object, "source")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or(config.source);
        if let Some(tag_prefix) = object_value(object, "tagPrefix").and_then(Value::as_str) {
            config.tag_prefix = tag_prefix.to_string();
        }
        config.command = object_value(object, "command")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    config
}

pub(crate) fn parse_release_deploy_config(value: Option<&Value>) -> ReleaseDeployConfig {
    let mut config = ReleaseDeployConfig::default();
    if let Some(object) = value.and_then(Value::as_object) {
        if let Some(provider) = object_value(object, "provider").and_then(Value::as_str) {
            config.provider = provider.to_string();
        }
        config.command = object_value(object, "command")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    config
}

pub(crate) fn parse_release_post_check_config(value: Option<&Value>) -> ReleasePostCheckConfig {
    let mut config = ReleasePostCheckConfig::default();
    if let Some(object) = value.and_then(Value::as_object) {
        config.url = object_value(object, "url")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        config.command = object_value(object, "command")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    config
}

pub(crate) fn parse_release_git_config(value: Option<&Value>) -> ReleaseGitConfig {
    let mut config = ReleaseGitConfig::default();
    if let Some(object) = value.and_then(Value::as_object) {
        config.commit = object_value(object, "commit")
            .and_then(Value::as_bool)
            .unwrap_or(config.commit);
        config.tag = object_value(object, "tag")
            .and_then(Value::as_bool)
            .unwrap_or(config.tag);
        config.push = object_value(object, "push")
            .and_then(Value::as_bool)
            .unwrap_or(config.push);
        config.remote = object_value(object, "remote")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or(config.remote);
        config.branch = object_value(object, "branch")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    config
}
