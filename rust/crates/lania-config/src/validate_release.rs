//! `release` 配置分区的专用校验器。
//!
//! release 配置比其它 section 更复杂，因为它本质上描述的是一条多阶段工作流：
//! - verify
//! - versioning
//! - changelog
//! - artifact
//! - deploy
//! - postCheck
//! - git
//!
//! 所以这里既要校验字段白名单，也要校验这些阶段配置的结构形态。

use crate::{
    validate_type, ConfigPosition, ConfigValidationError, ConfigValidationErrorCode, Value,
};

use crate::parse_release::parse_release_profile;

type JsonObject = serde_json::Map<String, Value>;

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

pub(crate) fn validate_release_config(
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
            "$.release",
            "release must be an object",
        );
        return;
    };

    let allowed = [
        "profile",
        "env",
        "channel",
        "stateFile",
        "verify",
        "versioning",
        "changelog",
        "artifact",
        "deploy",
        "postCheck",
        "git",
    ];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            push_error(
                errors,
                ConfigValidationErrorCode::UnknownField,
                format!("$.release.{key}"),
                format!("unknown release field `{key}`"),
            );
        }
    }

    if let Some(profile) = object_value(object, "profile") {
        if let Some(value) = profile.as_str() {
            if parse_release_profile(value).is_none() {
                push_error(
                    errors,
                    ConfigValidationErrorCode::InvalidValue,
                    "$.release.profile",
                    "release.profile must be package, web-app, service, or custom",
                );
            }
        } else {
            push_error(
                errors,
                ConfigValidationErrorCode::InvalidType,
                "$.release.profile",
                "release.profile must be a string",
            );
        }
    }
    validate_type(
        object_value(object, "env"),
        "$.release.env",
        Value::is_string,
        "string",
        errors,
    );
    validate_type(
        object_value(object, "channel"),
        "$.release.channel",
        Value::is_string,
        "string",
        errors,
    );
    validate_type(
        object_value(object, "stateFile"),
        "$.release.stateFile",
        Value::is_string,
        "string",
        errors,
    );
    validate_release_stage_object(object_value(object, "verify"), "$.release.verify", true, errors);
    validate_release_stage_object(
        object_value(object, "versioning"),
        "$.release.versioning",
        false,
        errors,
    );
    // 这里把 release 拆成两类阶段来校验：
    // - `stage_object`：本身应该是一个对象，里面再放若干字段
    // - `stage_leaf`：允许 bool/string/object 三种紧凑写法
    //
    // 这种区分正好对应 parser 里的容错能力，但 validator 会更明确地指出“哪种写法是允许的”。
    validate_release_stage_leaf(object_value(object, "changelog"), "$.release.changelog", errors);
    validate_release_stage_leaf(object_value(object, "artifact"), "$.release.artifact", errors);
    validate_release_stage_object(object_value(object, "deploy"), "$.release.deploy", false, errors);
    validate_release_stage_object(
        object_value(object, "postCheck"),
        "$.release.postCheck",
        false,
        errors,
    );
    validate_release_stage_object(object_value(object, "git"), "$.release.git", false, errors);
}

pub(crate) fn validate_release_stage_object(
    value: Option<&Value>,
    path: &str,
    nested_steps: bool,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_error(
            errors,
            ConfigValidationErrorCode::InvalidType,
            path,
            format!("{path} must be an object"),
        );
        return;
    };
    for (key, item) in object {
        let child_path = format!("{path}.{key}");
        if nested_steps && matches!(key.as_str(), "lint" | "test" | "build" | "smoke") {
            // `verify` 下面的 lint/test/build/smoke 自己又是一个“leaf stage”，
            // 所以这里递归委托给 leaf validator，而不是把它们当普通字符串字段看待。
            validate_release_stage_leaf(Some(item), &child_path, errors);
            continue;
        }
        if matches!(key.as_str(), "enabled" | "commit" | "tag" | "push") {
            if !item.is_boolean() {
                push_error(
                    errors,
                    ConfigValidationErrorCode::InvalidType,
                    child_path,
                    format!("{path}.{key} must be a boolean"),
                );
            }
            continue;
        }
        if matches!(
            key.as_str(),
            "command" | "source" | "tagPrefix" | "provider" | "url" | "remote" | "branch"
        ) && !item.is_string()
        {
            push_error(
                errors,
                ConfigValidationErrorCode::InvalidType,
                child_path,
                format!("{path}.{key} must be a string"),
            );
        }
    }
}

pub(crate) fn validate_release_stage_leaf(
    value: Option<&Value>,
    path: &str,
    errors: &mut Vec<ConfigValidationError>,
) {
    let Some(value) = value else {
        return;
    };
    // leaf stage 故意允许三种形态：
    // - bool：只表达开关
    // - string：直接写命令
    // - object：精细配置
    //
    // 这和 parser 的容错目标一致，确保“用户喜欢的简写方式”不会被 validator 错判。
    if value.is_boolean() || value.is_string() {
        return;
    }
    if let Some(object) = value.as_object() {
        for (key, item) in object {
            let child_path = format!("{path}.{key}");
            if key == "enabled" {
                if !item.is_boolean() {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidType,
                        child_path,
                        format!("{path}.enabled must be a boolean"),
                    );
                }
            } else if key == "command" {
                if !item.is_string() {
                    push_error(
                        errors,
                        ConfigValidationErrorCode::InvalidType,
                        child_path,
                        format!("{path}.command must be a string"),
                    );
                }
            } else {
                push_error(
                    errors,
                    ConfigValidationErrorCode::UnknownField,
                    child_path,
                    format!("unknown release step field `{key}`"),
                );
            }
        }
        return;
    }
    push_error(
        errors,
        ConfigValidationErrorCode::InvalidType,
        path,
        format!("{path} must be a boolean, string, or object"),
    );
}
