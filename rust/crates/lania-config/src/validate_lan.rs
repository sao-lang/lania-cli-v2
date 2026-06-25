//! `lan.config` 的顶层校验入口（字段白名单 + 基础类型检查 + 分区校验）。
//!
//! 这个文件的定位是“总校验器”：
//! - 先确保根节点是 object、schema version 在可接受范围内
//! - 再做顶层字段白名单检查（UnknownField）
//! - 最后把 `ui/extensions/schemaDiscovery/commands/hooks/release` 分发给对应的分区校验器
//!
//! 注意：这里的校验不会阻止 snapshot 构建成功，它更像“诊断信息”：
//! 宿主会继续运行，但会把 errors 附带在 `LanConfigSnapshot.validation_errors` 里。

use crate::{
    validate_string_array, validate_type, ConfigPosition, ConfigValidationError,
    ConfigValidationErrorCode, Value, CURRENT_LAN_CONFIG_VERSION,
};

use crate::parse_plugins::parse_plugin_ref;
use crate::validate_release::validate_release_config;
use crate::validate_sections::{
    validate_commands_config, validate_extensions_config, validate_hooks_config,
    validate_schema_discovery_config, validate_ui_config,
};

type JsonObject = serde_json::Map<String, Value>;

fn raw_value<'a>(raw: &'a Value, key: &str) -> Option<&'a Value> {
    raw.get(key)
}

fn object_value<'a>(object: &'a JsonObject, key: &str) -> Option<&'a Value> {
    object.get(key)
}

fn push_error(
    errors: &mut Vec<ConfigValidationError>,
    code: ConfigValidationErrorCode,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    errors.push(ConfigValidationError {
        code,
        message: message.into(),
        position: ConfigPosition {
            path: path.into(),
            line: None,
            column: None,
        },
    });
}

pub(crate) fn validate_lan_config(raw: &Value, schema_version: u32) -> Vec<ConfigValidationError> {
    let mut errors = Vec::new();
    let object = match raw.as_object() {
        Some(object) => object,
        None => {
            push_error(
                &mut errors,
                ConfigValidationErrorCode::InvalidType,
                "$",
                "lan.config root must be an object",
            );
            return errors;
        }
    };

    if schema_version > CURRENT_LAN_CONFIG_VERSION {
        push_error(
            &mut errors,
            ConfigValidationErrorCode::UnsupportedVersion,
            "$.version",
            format!(
                "config version {schema_version} is newer than supported version {CURRENT_LAN_CONFIG_VERSION}"
            ),
        );
    }

    let allowed = [
        "version",
        "buildTool",
        "buildAdaptors",
        "lintAdaptors",
        "lintTools",
        "plugins",
        "extensions",
        "ui",
        "schemaDiscovery",
        "commands",
        "hooks",
        "release",
        "custom",
        "pluginAllowlist",
        "pluginMethodAllowlist",
        "pluginTrustedSources",
        "pluginRequireSignature",
        "pluginSignatureAllowlist",
    ];
    // 顶层白名单的作用不是“替用户猜字段名”，而是尽快把拼写错误显式指出来。
    // 否则用户把 `buildTool` 写成 `buildtool` 时，normalize 往往只会默默回退默认值，定位更难。
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                &mut errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.{key}"),
                format!("unknown lan.config field `{key}`"),
            );
        }
    }

    validate_type(
        raw_value(raw, "buildTool"),
        "$.buildTool",
        Value::is_string,
        "string",
        &mut errors,
    );
    validate_type(
        raw_value(raw, "buildAdaptors"),
        "$.buildAdaptors",
        Value::is_object,
        "object",
        &mut errors,
    );
    validate_type(
        raw_value(raw, "lintAdaptors"),
        "$.lintAdaptors",
        Value::is_object,
        "object",
        &mut errors,
    );
    if let Some(value) = raw_value(raw, "lintTools") {
        if let Some(items) = value.as_array() {
            for (index, item) in items.iter().enumerate() {
                if !item.is_string() {
                    push_error(
                        &mut errors,
                        ConfigValidationErrorCode::InvalidType,
                        format!("$.lintTools[{index}]"),
                        "lintTools entries must be strings",
                    );
                }
            }
        } else {
            push_error(
                &mut errors,
                ConfigValidationErrorCode::InvalidType,
                "$.lintTools",
                "lintTools must be an array",
            );
        }
    }
    if let Some(value) = raw_value(raw, "plugins") {
        if let Some(items) = value.as_array() {
            for (index, item) in items.iter().enumerate() {
                // 这里复用 `parse_plugin_ref` 做结构与安全边界校验，
                // 避免“解析逻辑一套、校验逻辑另一套”逐渐漂移。
                if let Err(error) = parse_plugin_ref(item) {
                    push_error(
                        &mut errors,
                        ConfigValidationErrorCode::InvalidValue,
                        format!("$.plugins[{index}]"),
                        error.to_string(),
                    );
                }
            }
        } else {
            push_error(
                &mut errors,
                ConfigValidationErrorCode::InvalidType,
                "$.plugins",
                "plugins must be an array",
            );
        }
    }
    validate_extensions_config(raw_value(raw, "extensions"), &mut errors);
    validate_ui_config(raw_value(raw, "ui"), &mut errors);
    validate_schema_discovery_config(raw_value(raw, "schemaDiscovery"), &mut errors);
    validate_commands_config(raw_value(raw, "commands"), &mut errors);
    validate_hooks_config(raw_value(raw, "hooks"), &mut errors);
    validate_type(
        raw_value(raw, "custom"),
        "$.custom",
        Value::is_object,
        "object",
        &mut errors,
    );
    validate_release_config(
        raw_value(raw, "config")
            .and_then(Value::as_object)
            .and_then(|object| object_value(object, "release"))
            .or_else(|| raw_value(raw, "release")),
        &mut errors,
    );
    // 这里兼容 `config.release` 和顶层 `release` 两种形态，
    // 说明 validator 也承担一部分历史兼容职责，而不只是机械做类型检查。
    validate_string_array(raw_value(raw, "pluginAllowlist"), "$.pluginAllowlist", &mut errors);
    validate_string_array(
        raw_value(raw, "pluginMethodAllowlist"),
        "$.pluginMethodAllowlist",
        &mut errors,
    );
    if let Some(value) = raw_value(raw, "pluginTrustedSources") {
        if let Some(items) = value.as_array() {
            for (index, item) in items.iter().enumerate() {
                match item.as_str() {
                    Some("package") | Some("local_path") => {}
                    _ => push_error(
                        &mut errors,
                        ConfigValidationErrorCode::InvalidValue,
                        format!("$.pluginTrustedSources[{index}]"),
                        "trusted plugin source must be `package` or `local_path`",
                    ),
                }
            }
        } else {
            push_error(
                &mut errors,
                ConfigValidationErrorCode::InvalidType,
                "$.pluginTrustedSources",
                "pluginTrustedSources must be an array",
            );
        }
    }
    validate_type(
        raw_value(raw, "pluginRequireSignature"),
        "$.pluginRequireSignature",
        Value::is_boolean,
        "boolean",
        &mut errors,
    );
    validate_string_array(
        raw_value(raw, "pluginSignatureAllowlist"),
        "$.pluginSignatureAllowlist",
        &mut errors,
    );

    errors
}
