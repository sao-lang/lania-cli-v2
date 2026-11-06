//! `lan.config` 各分区（section）的细粒度校验器。
//!
//! 为什么要单独拆这个文件？
//! - `ui`、`extensions`、`schemaDiscovery`、`commands`、`hooks` 这些分区彼此独立
//! - 拆开后更容易维护白名单、默认值和错误路径
//! - 也更方便后续 schema 扩展时局部修改
//!
//! 可以把这里理解成 `validate_lan.rs` 的“子模块集合”。

use crate::{
    validate_string_array, validate_string_object, validate_type, ConfigPosition,
    ConfigValidationError, ConfigValidationErrorCode, Value,
};

type JsonObject = serde_json::Map<String, Value>;

/// 校验 `runtime` / `runtimes`（§6.5.5）。
///
/// 约束：
/// - `runtimes.<id>.build` / `.dev` 必须是 `string[]`；
/// - 未知运行时 id 只报 **warning 级别** 的问题（这里用 `InvalidValue` 之外的单独消息表达），
///   因为未来可能新增运行时，报 error 会让旧版本 CLI 拒绝新配置。
pub(crate) fn validate_runtime_config(
    runtime: Option<&Value>,
    runtimes: Option<&Value>,
    errors: &mut Vec<ConfigValidationError>,
) {
    if let Some(value) = runtime {
        match value.as_str() {
            Some(text) if !text.trim().is_empty() => {
                if !crate::known_runtime_ids()
                    .iter()
                    .any(|id| id.eq_ignore_ascii_case(text.trim()))
                {
                    // 未知运行时只提示，不阻断：配置可能来自更新版本的 lania。
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidValue,
                        "$.runtime",
                        format!(
                            "unknown runtime `{text}`; expected one of: {}",
                            crate::known_runtime_ids().join(", ")
                        ),
                    );
                }
            }
            _ => push_error(
                errors,
                ConfigValidationErrorCode::InvalidType,
                "$.runtime",
                "runtime must be a non-empty string",
            ),
        }
    }

    let Some(runtimes) = runtimes else {
        return;
    };
    let Some(object) = runtimes.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.runtimes",
            "runtimes must be an object",
        );
        return;
    };

    for (id, entry) in object {
        let path = format!("$.runtimes.{id}");
        let Some(entry) = entry.as_object() else {
            push_error(
                errors,
                ConfigValidationErrorCode::InvalidType,
                &path,
                "each runtime override must be an object",
            );
            continue;
        };
        if !crate::known_runtime_ids()
            .iter()
            .any(|known| known.eq_ignore_ascii_case(id))
        {
            push_error(
                errors,
                ConfigValidationErrorCode::InvalidValue,
                &path,
                format!(
                    "unknown runtime id `{id}`; expected one of: {}",
                    crate::known_runtime_ids().join(", ")
                ),
            );
        }
        for key in ["build", "dev"] {
            let Some(value) = entry.get(key) else {
                continue;
            };
            match value.as_array() {
                Some(items) if items.iter().all(Value::is_string) => {}
                _ => push_error(
                    errors,
                    ConfigValidationErrorCode::InvalidType,
                    format!("{path}.{key}"),
                    format!("{path}.{key} must be an array of strings"),
                ),
            }
        }
        if let Some(value) = entry.get("packageManager") {
            if !value.is_string() {
                push_error(
                    errors,
                    ConfigValidationErrorCode::InvalidType,
                    format!("{path}.packageManager"),
                    "packageManager must be a string",
                );
            }
        }
        for key in entry.keys() {
            if !["build", "dev", "packageManager"].contains(&key.as_str()) {
                push_error(
                    errors,
                    ConfigValidationErrorCode::UnknownField,
                    format!("{path}.{key}"),
                    format!("unknown runtime override field `{key}`"),
                );
            }
        }
    }
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

pub(crate) fn validate_extensions_config(
    value: Option<&Value>,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.extensions",
            "extensions must be an object",
        );
        return;
    };

    let allowed = ["dynamicCommands"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.extensions.{key}"),
                format!("unknown extensions field `{key}`"),
            );
        }
    }

    validate_type(
        object_value(object, "dynamicCommands"),
        "$.extensions.dynamicCommands",
        Value::is_boolean,
        "boolean",
        errors,
    );
    // lifecycle extension is removed in v2.1 final; use `hooks` instead.
}

pub(crate) fn validate_schema_discovery_config(
    value: Option<&Value>,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.schemaDiscovery",
            "schemaDiscovery must be an object",
        );
        return;
    };

    let allowed = ["files", "dirs", "allowExtensions"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.schemaDiscovery.{key}"),
                format!("unknown schemaDiscovery field `{key}`"),
            );
        }
    }

    validate_string_array(
        object_value(object, "files"),
        "$.schemaDiscovery.files",
        errors,
    );
    validate_string_array(
        object_value(object, "dirs"),
        "$.schemaDiscovery.dirs",
        errors,
    );
    validate_string_array(
        object_value(object, "allowExtensions"),
        "$.schemaDiscovery.allowExtensions",
        errors,
    );
}

pub(crate) fn validate_ui_config(value: Option<&Value>, errors: &mut Vec<ConfigValidationError>) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.ui",
            "ui must be an object",
        );
        return;
    };

    let allowed = ["locale", "output", "progress", "interaction"];
    // section 级白名单的好处是错误路径能精确到 `$.ui.xxx`，
    // 用户比起只看到“顶层配置有问题”更容易快速修复。
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.ui.{key}"),
                format!("unknown ui field `{key}`"),
            );
        }
    }

    validate_type(
        object_value(object, "locale"),
        "$.ui.locale",
        Value::is_string,
        "string",
        errors,
    );
    validate_ui_output_config(object_value(object, "output"), errors);
    validate_ui_progress_config(object_value(object, "progress"), errors);
    validate_ui_interaction_config(object_value(object, "interaction"), errors);
}

pub(crate) fn validate_ui_output_config(
    value: Option<&Value>,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.ui.output",
            "ui.output must be an object",
        );
        return;
    };

    let allowed = ["mode"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.ui.output.{key}"),
                format!("unknown ui.output field `{key}`"),
            );
        }
    }

    let mode = object_value(object, "mode");
    validate_type(mode, "$.ui.output.mode", Value::is_string, "string", errors);
    if let Some(mode) = mode.and_then(Value::as_str) {
        if !matches!(mode, "json" | "human") {
            push_error(
                errors,
                ConfigValidationErrorCode::InvalidValue,
                "$.ui.output.mode",
                "ui.output.mode must be `json` or `human`",
            );
        }
    }
}

pub(crate) fn validate_ui_progress_config(
    value: Option<&Value>,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.ui.progress",
            "ui.progress must be an object",
        );
        return;
    };

    let allowed = ["style", "grouping"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.ui.progress.{key}"),
                format!("unknown ui.progress field `{key}`"),
            );
        }
    }

    validate_type(
        object_value(object, "style"),
        "$.ui.progress.style",
        Value::is_string,
        "string",
        errors,
    );
    validate_type(
        object_value(object, "grouping"),
        "$.ui.progress.grouping",
        Value::is_string,
        "string",
        errors,
    );
}

pub(crate) fn validate_ui_interaction_config(
    value: Option<&Value>,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.ui.interaction",
            "ui.interaction must be an object",
        );
        return;
    };

    let allowed = ["mode", "timeoutMs", "defaultStrategy"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.ui.interaction.{key}"),
                format!("unknown ui.interaction field `{key}`"),
            );
        }
    }

    validate_type(
        object_value(object, "mode"),
        "$.ui.interaction.mode",
        Value::is_string,
        "string",
        errors,
    );
    validate_type(
        object_value(object, "timeoutMs"),
        "$.ui.interaction.timeoutMs",
        Value::is_u64,
        "number",
        errors,
    );
    validate_type(
        object_value(object, "defaultStrategy"),
        "$.ui.interaction.defaultStrategy",
        Value::is_string,
        "string",
        errors,
    );
}

pub(crate) fn validate_commands_config(
    value: Option<&Value>,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.commands",
            "commands must be an object",
        );
        return;
    };

    let allowed = ["aliases", "shortcuts"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.commands.{key}"),
                format!("unknown commands field `{key}`"),
            );
        }
    }

    validate_string_object(
        object_value(object, "aliases"),
        "$.commands.aliases",
        errors,
    );
    validate_string_object(
        object_value(object, "shortcuts"),
        "$.commands.shortcuts",
        errors,
    );
}

pub(crate) fn validate_hooks_config(
    value: Option<&Value>,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            "$.hooks",
            "hooks must be an object",
        );
        return;
    };

    for (hook_name, handlers) in object {
        let path = format!("$.hooks.{hook_name}");
        let Some(items) = handlers.as_array() else {
            push_error(
                errors,
                ConfigValidationErrorCode::InvalidType,
                path,
                "hook bindings must be an array",
            );
            continue;
        };

        for (index, item) in items.iter().enumerate() {
            let item_path = format!("$.hooks.{hook_name}[{index}]");
            let Some(binding) = item.as_object() else {
                push_error(
                    errors,
                    ConfigValidationErrorCode::InvalidType,
                    item_path,
                    "hook binding must be an object",
                );
                continue;
            };
            for key in binding.keys() {
                if !matches!(
                    key.as_str(),
                    "type" | "kind" | "plugin" | "handler" | "timeoutMs" | "onError" | "fn"
                ) {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::UnknownField,
                        format!("{item_path}.{key}"),
                        format!("unknown hook binding field `{key}`"),
                    );
                }
            }
            if let Some(value) = object_value(binding, "type").and_then(Value::as_str) {
                if !matches!(value, "plugin" | "inline") {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidValue,
                        format!("{item_path}.type"),
                        "hook binding type must be `plugin` or `inline`",
                    );
                }
            }
            if let Some(value) = object_value(binding, "kind").and_then(Value::as_str) {
                if !matches!(value, "waterfall" | "parallel") {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidValue,
                        format!("{item_path}.kind"),
                        "hook binding kind must be `waterfall` or `parallel`",
                    );
                }
            }
            let binding_type = object_value(binding, "type")
                .and_then(Value::as_str)
                .unwrap_or("plugin");
            if binding_type == "plugin" {
                if object_value(binding, "plugin")
                    .and_then(Value::as_str)
                    .is_none()
                {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidType,
                        format!("{item_path}.plugin"),
                        "plugin hook binding plugin must be a string",
                    );
                }
                if object_value(binding, "handler")
                    .and_then(Value::as_str)
                    .is_none()
                {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidType,
                        format!("{item_path}.handler"),
                        "plugin hook binding handler must be a string",
                    );
                }
            }
            if let Some(timeout) = object_value(binding, "timeoutMs") {
                if !timeout.is_u64() {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidType,
                        format!("{item_path}.timeoutMs"),
                        "hook binding timeoutMs must be a positive integer",
                    );
                }
            }
            if let Some(value) = object_value(binding, "onError").and_then(Value::as_str) {
                if !matches!(value, "throw" | "collect") {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidValue,
                        format!("{item_path}.onError"),
                        "hook binding onError must be `throw` or `collect`",
                    );
                }
            }
        }
    }
}
