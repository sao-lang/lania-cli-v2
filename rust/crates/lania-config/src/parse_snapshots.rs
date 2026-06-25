//! 各配置 section 的快照解析器。
//!
//! `normalize.rs` 更像总装配入口，而这个文件负责把每个 section 单独拆开解析：
//! - `ui`
//! - `extensions`
//! - `schemaDiscovery`
//! - `commands`
//! - `hooks`
//! - `release`
//!
//! 这样做的好处是：每个配置分区都能独立演进，不会让 `normalize` 入口变成超长巨石函数。

use std::collections::BTreeMap;

use crate::{
    as_string_vec, CommandsSnapshot, HookBinding, HookBindingKind, HookBindingSource,
    LanExtensionsSnapshot, ReleaseConfigSnapshot, ReleaseProfile, SchemaDiscoverySnapshot,
    UiInteractionSnapshot, UiOutputSnapshot, UiProgressSnapshot, UiSnapshot, Value,
};

use crate::parse_release::{
    parse_release_deploy_config, parse_release_git_config, parse_release_post_check_config,
    parse_release_profile, parse_release_step_config, parse_release_verify_config,
    parse_release_versioning_config,
};

type JsonObject = serde_json::Map<String, Value>;

fn object_value<'a>(object: &'a JsonObject, key: &str) -> Option<&'a Value> {
    object.get(key)
}

pub(crate) fn parse_extensions_snapshot(value: Option<&Value>) -> LanExtensionsSnapshot {
    let raw = value.cloned().unwrap_or(Value::Null);
    let object = value.and_then(Value::as_object);
    LanExtensionsSnapshot {
        dynamic_commands: object
            .and_then(|map| object_value(map, "dynamicCommands"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        raw,
    }
}

pub(crate) fn parse_schema_discovery_snapshot(value: Option<&Value>) -> SchemaDiscoverySnapshot {
    let mut snapshot = SchemaDiscoverySnapshot::default();
    if let Some(raw) = value {
        snapshot.raw = raw.clone();
        if let Some(object) = raw.as_object() {
            // 这里对每个字段都采用“能解析就收下，解析不了就保留默认值”的策略。
            // 也就是说，snapshot 解析阶段尽量温和，不在这里制造 hard error；
            // 真正的类型问题会在 validate 阶段进入 `validation_errors`。
            if let Some(files) = object_value(object, "files").and_then(as_string_vec) {
                snapshot.files = files;
            }
            if let Some(dirs) = object_value(object, "dirs").and_then(as_string_vec) {
                snapshot.dirs = dirs;
            }
            if let Some(allow_extensions) =
                object_value(object, "allowExtensions").and_then(as_string_vec)
            {
                snapshot.allow_extensions = allow_extensions;
            }
        }
    }
    snapshot
}

pub(crate) fn parse_ui_snapshot(value: Option<&Value>) -> UiSnapshot {
    let mut snapshot = UiSnapshot::default();
    if let Some(raw) = value {
        snapshot.raw = raw.clone();
        if let Some(object) = raw.as_object() {
            snapshot.locale = object_value(object, "locale")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            snapshot.output = parse_ui_output_snapshot(object_value(object, "output"));
            snapshot.progress = parse_ui_progress_snapshot(object_value(object, "progress"));
            snapshot.interaction = parse_ui_interaction_snapshot(object_value(object, "interaction"));
        }
    }
    snapshot
}

pub(crate) fn parse_ui_output_snapshot(value: Option<&Value>) -> UiOutputSnapshot {
    let mut snapshot = UiOutputSnapshot::default();
    if let Some(raw) = value {
        snapshot.raw = raw.clone();
        if let Some(object) = raw.as_object() {
            if let Some(mode) = object_value(object, "mode").and_then(Value::as_str) {
                snapshot.mode = mode.to_string();
            }
            if let Some(events) = object_value(object, "events").and_then(Value::as_str) {
                snapshot.events = events.to_string();
            }
            if let Some(pretty) = object_value(object, "pretty").and_then(Value::as_bool) {
                snapshot.pretty = pretty;
            }
            if let Some(include_host_state) =
                object_value(object, "includeHostState").and_then(Value::as_bool)
            {
                snapshot.include_host_state = include_host_state;
            }
            if let Some(include_bridge_exchange) =
                object_value(object, "includeBridgeExchange").and_then(Value::as_bool)
            {
                snapshot.include_bridge_exchange = include_bridge_exchange;
            }
        }
    }
    snapshot
}

pub(crate) fn parse_ui_progress_snapshot(value: Option<&Value>) -> UiProgressSnapshot {
    let mut snapshot = UiProgressSnapshot::default();
    if let Some(raw) = value {
        snapshot.raw = raw.clone();
        if let Some(object) = raw.as_object() {
            if let Some(style) = object_value(object, "style").and_then(Value::as_str) {
                snapshot.style = style.to_string();
            }
            if let Some(grouping) = object_value(object, "grouping").and_then(Value::as_str) {
                snapshot.grouping = grouping.to_string();
            }
        }
    }
    snapshot
}

pub(crate) fn parse_ui_interaction_snapshot(value: Option<&Value>) -> UiInteractionSnapshot {
    let mut snapshot = UiInteractionSnapshot::default();
    if let Some(raw) = value {
        snapshot.raw = raw.clone();
        if let Some(object) = raw.as_object() {
            if let Some(mode) = object_value(object, "mode").and_then(Value::as_str) {
                snapshot.mode = mode.to_string();
            }
            snapshot.timeout_ms = object_value(object, "timeoutMs").and_then(Value::as_u64);
            if let Some(default_strategy) =
                object_value(object, "defaultStrategy").and_then(Value::as_str)
            {
                snapshot.default_strategy = default_strategy.to_string();
            }
        }
    }
    snapshot
}

pub(crate) fn parse_commands_snapshot(value: Option<&Value>) -> CommandsSnapshot {
    let mut snapshot = CommandsSnapshot::default();
    if let Some(raw) = value {
        snapshot.raw = raw.clone();
        if let Some(object) = raw.as_object() {
            snapshot.aliases = parse_string_map(object_value(object, "aliases"));
            snapshot.shortcuts = parse_string_map(object_value(object, "shortcuts"));
        }
    }
    snapshot
}

pub(crate) fn parse_hook_bindings_snapshot(
    value: Option<&Value>,
) -> BTreeMap<String, Vec<HookBinding>> {
    let mut hooks = BTreeMap::new();
    let Some(object) = value.and_then(Value::as_object) else {
        return hooks;
    };

    for (hook_key, handlers) in object {
        let Some(items) = handlers.as_array() else {
            continue;
        };
        let bindings = items
            .iter()
            .filter_map(|item| {
                let object = item.as_object()?;
                // 这里用 `filter_map`，意味着单个 hook binding 条目格式不对时会被静默跳过，
                // 而不是让整个 hooks section 解析失败。
                // 对用户来说，具体问题仍会在 validate 阶段以错误路径报告出来。
                Some(HookBinding {
                    r#type: object_value(object, "type").and_then(Value::as_str).and_then(
                        |value| match value {
                            "plugin" => Some(HookBindingSource::Plugin),
                            "inline" => Some(HookBindingSource::Inline),
                            _ => None,
                        },
                    ),
                    kind: object_value(object, "kind").and_then(Value::as_str).and_then(
                        |value| match value {
                            "waterfall" => Some(HookBindingKind::Waterfall),
                            "parallel" => Some(HookBindingKind::Parallel),
                            _ => None,
                        },
                    ),
                    plugin: object_value(object, "plugin")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    handler: object_value(object, "handler")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    timeout_ms: object_value(object, "timeoutMs").and_then(Value::as_u64),
                    on_error: object_value(object, "onError")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    raw: item.clone(),
                })
            })
            .collect::<Vec<HookBinding>>();
        if !bindings.is_empty() {
            hooks.insert(hook_key.clone(), bindings);
        }
    }

    hooks
}

pub(crate) fn parse_string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    // 只有 value 真的是 string 才收进快照。
                    // 这让 snapshot 始终保持强类型，避免下游再到处判断 JSON 类型。
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_release_snapshot(value: Option<&Value>) -> Option<ReleaseConfigSnapshot> {
    let raw = value?.clone();
    if raw.is_null() || !raw.is_object() {
        return None;
    }
    let object = raw.as_object()?;
    Some(ReleaseConfigSnapshot {
        profile: object_value(object, "profile")
            .and_then(Value::as_str)
            .and_then(parse_release_profile)
            .unwrap_or(ReleaseProfile::Package),
        env: object_value(object, "env")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        channel: object_value(object, "channel")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        state_file: object_value(object, "stateFile")
            .and_then(Value::as_str)
            .unwrap_or(".lania/release-state.json")
            .to_string(),
        verify: parse_release_verify_config(object_value(object, "verify")),
        versioning: parse_release_versioning_config(object_value(object, "versioning")),
        changelog: parse_release_step_config(object_value(object, "changelog"), false),
        artifact: parse_release_step_config(object_value(object, "artifact"), false),
        deploy: parse_release_deploy_config(object_value(object, "deploy")),
        post_check: parse_release_post_check_config(object_value(object, "postCheck")),
        git: parse_release_git_config(object_value(object, "git")),
        raw,
    })
}
