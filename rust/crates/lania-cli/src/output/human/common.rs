//! 人类可读输出渲染的通用工具函数。
//!
//! 这个文件主要提供：
//! - JSON 深度本地化（把用户可见的字符串替换为对应语言）
//! - 输出抑制逻辑（交互式渲染已发生时避免重复输出）
//! - pretty json / raw value 渲染
//! - 简单的 block 拼装函数
//!
//! 设计原则：
//! - 所有函数都是“纯工具”：不依赖具体 kind，不做 IO，不做全局状态修改。
//! - 通过 `pub(super)` 限制作用域，仅供 `output::human` 模块内部使用。

use anyhow::Result;

use crate::{OutputMode, OutputProfile};
use crate::profile::localized_user_message;

pub(super) fn localize_json_strings(value: &mut serde_json::Value, locale: &str) {
    match value {
        // 约定：用户可见的文案在 JSON 中以 string 形式出现（例如 message/labels）。
        // 这里采用“深度遍历 + 原地替换”的方式，保证所有嵌套字段都能被处理到。
        serde_json::Value::String(text) => {
            *text = localized_user_message(locale, text);
        }
        serde_json::Value::Array(items) => {
            for item in items {
                localize_json_strings(item, locale);
            }
        }
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                localize_json_strings(value, locale);
            }
        }
        _ => {}
    }
}

pub(super) fn is_locale_value(value: &serde_json::Value) -> bool {
    value.get("kind").and_then(|kind| kind.as_str()) == Some("locale")
}

pub(super) fn is_config_value(value: &serde_json::Value) -> bool {
    value.get("kind").and_then(|kind| kind.as_str()) == Some("config_value")
}

pub(super) fn should_suppress_rendered_output(
    value: &serde_json::Value,
    profile: &OutputProfile,
) -> bool {
    // 某些交互式 UI 会在 host 侧“直接渲染”，此时再输出一份会重复。
    // 约定字段：`_interactiveRendered`（可能在顶层或 execution 下）
    let interactive_rendered = value
        .get("_interactiveRendered")
        .and_then(|item| item.as_bool())
        .or_else(|| {
            value
                .get("execution")
                .and_then(|execution| execution.get("_interactiveRendered"))
                .and_then(|item| item.as_bool())
        })
        .unwrap_or(false);
    if interactive_rendered {
        return true;
    }

    // 人类可读输出模式下允许正常输出；其它模式下这里通常不会被调用，
    // 但我们仍保守返回 false（只要不是 interactiveRendered）。
    if matches!(profile.mode, OutputMode::Human) {
        return false;
    }

    false
}

pub(super) fn strip_internal_output_flags(value: &mut serde_json::Value) {
    // `_interactiveRendered` 只是内部控制字段，不应出现在最终输出里。
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.remove("_interactiveRendered");
    if let Some(execution) = object
        .get_mut("execution")
        .and_then(|value| value.as_object_mut())
    {
        execution.remove("_interactiveRendered");
    }
}

pub(super) fn render_pretty_json(value: &serde_json::Value) -> Result<String> {
    // 使用固定的 4-space 缩进，保证 human/json 输出风格一致且可读。
    let mut buffer = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
    serde::Serialize::serialize(value, &mut serializer)?;
    Ok(String::from_utf8(buffer).expect("json serializer should only emit utf-8"))
}

pub(super) fn render_raw_value(value: &serde_json::Value) -> String {
    // `config_value` / `locale` 这类 kind 在 human 模式下属于“原样回显”，
    // 不能再包裹额外的结构（否则会影响 shell 里管道/脚本的解析）。
    let raw = if is_config_value(value) {
        value.get("value")
    } else {
        value.get("locale")
    };
    match raw.unwrap_or(&serde_json::Value::Null) {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

pub(super) fn human_block(title: &str, body: impl AsRef<str>) -> String {
    // 一个简单的标题 + 正文拼装器：
    // - body 为空时只输出标题（避免多余空行）
    // - body 不为空时输出两行：title + body
    let body = body.as_ref();
    if body.trim().is_empty() {
        title.to_string()
    } else {
        format!("{title}\n{body}")
    }
}
