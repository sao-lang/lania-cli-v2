//! 配置校验阶段的通用工具函数。
//!
//! 这些函数做的是“结构层面”的校验（type/shape），而不是业务语义校验：
//! - `validate_type`：字段类型是否符合预期
//! - `validate_string_array`：数组元素是否都是字符串
//! - `validate_string_object`：对象 value 是否都是字符串
//!
//! 它们统一返回 `ConfigValidationError`，让上层可以把多条错误一起展示给用户。

use crate::{ConfigPosition, ConfigValidationError, ConfigValidationErrorCode, Value};

fn push_validation_error(
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

pub(crate) fn validate_string_object(
    value: Option<&Value>,
    path: &str,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_validation_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            path,
            format!("{path} must be an object"),
        );
        return;
    };
    for (key, item) in object {
        if !item.is_string() {
            push_validation_error(
                errors,
                ConfigValidationErrorCode::InvalidType,
                format!("{path}.{key}"),
                format!("{path}.{key} must be a string"),
            );
        }
    }
}

pub(crate) fn validate_tool_config(raw: &Value) -> Vec<ConfigValidationError> {
    if raw.is_null() || raw.is_object() {
        Vec::new()
    } else {
        let mut errors = Vec::new();
        push_validation_error(
            &mut errors,
            ConfigValidationErrorCode::InvalidType,
            "$",
            "tool config root should be an object",
        );
        errors
    }
}

pub(crate) fn validate_type(
    value: Option<&Value>,
    path: &str,
    predicate: impl Fn(&Value) -> bool,
    expected: &str,
    errors: &mut Vec<ConfigValidationError>,
) {
    if let Some(value) = value {
        if !predicate(value) {
            push_validation_error(
                errors,
                ConfigValidationErrorCode::InvalidType,
                path,
                format!("{path} must be a {expected}"),
            );
        }
    }
}

pub(crate) fn validate_string_array(
    value: Option<&Value>,
    path: &str,
    errors: &mut Vec<ConfigValidationError>,
) {
    if let Some(value) = value {
        if let Some(items) = value.as_array() {
            for (index, item) in items.iter().enumerate() {
                if !item.is_string() {
                    push_validation_error(
                        errors,
                        ConfigValidationErrorCode::InvalidType,
                        format!("{path}[{index}]"),
                        format!("{path} entries must be strings"),
                    );
                }
            }
        } else {
            push_validation_error(
                errors,
                ConfigValidationErrorCode::InvalidType,
                path,
                format!("{path} must be an array"),
            );
        }
    }
}

pub(crate) fn as_string_vec(value: &Value) -> Option<Vec<String>> {
    value.as_array().map(|items| {
        items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect::<Vec<String>>()
    })
}
