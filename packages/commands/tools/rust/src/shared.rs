//! `tools` 各子模块共享的命令参数读取与路径解析辅助函数。

use anyhow::{anyhow, Result};
use lania_command::CommandContext;
use serde_json::Value;
use std::path::PathBuf;

pub(super) fn required_arg(context: &CommandContext, name: &str) -> Result<String> {
    context
        .argv
        .args
        .get(name)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("missing required argument `{name}`"))
}

pub(super) fn resolve_from_cwd(cwd: &str, input: &str) -> PathBuf {
    let candidate = PathBuf::from(input);
    if candidate.is_absolute() {
        candidate
    } else {
        PathBuf::from(cwd).join(candidate)
    }
}

pub(super) fn json_value_to_args(value: &Value) -> Vec<String> {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect();
    }
    value
        .as_str()
        .map(|item| vec![item.to_string()])
        .unwrap_or_default()
}

pub(super) fn numeric_option(context: &CommandContext, name: &str) -> Option<usize> {
    context
        .argv
        .options
        .get(name)
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
}

pub(super) fn bool_option(context: &CommandContext, name: &str) -> bool {
    context
        .argv
        .options
        .get(name)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(super) fn string_option(context: &CommandContext, name: &str) -> Option<String> {
    context
        .argv
        .options
        .get(name)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}
