//! JSON 输出准备：把 `lania_host::CommandExecution` 转成 `serde_json::Value`。
//!
//! 这里做的事情都是“结构调整”，不做最终字符串渲染：
//! - 根据 `OutputProfile` 决定是否包含 `host_state` / `bridge exchange`
//! - `events=stream` 时剥离已流式输出过的 events，避免重复
//! - 对输出中出现的可本地化字符串做递归替换（深度遍历 JSON）

use crate::{EventMode, OutputProfile};

use crate::profile::localized_user_message;

fn localize_json_strings(value: &mut serde_json::Value, locale: &str) {
    match value {
        // 约定：需要本地化的用户可见文案会以 string 形式出现，直接替换即可。
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

pub(crate) fn execution_json_value(
    execution: &lania_host::CommandExecution,
    profile: &OutputProfile,
) -> serde_json::Value {
    // `CommandExecution` 是 host/runtime 的统一结果 envelope。
    // 这里先把它完全序列化，再按 profile 做裁剪/本地化，避免在 Rust 侧维护一堆结构体镜像。
    let mut value = serde_json::to_value(execution).expect("execution serializes");

    // 这些 kind 属于“纯信息类输出”，需要尽量简洁：
    // - template_info / locale / config_value / config
    if is_template_info_value(&value)
        || is_locale_value(&value)
        || is_config_value(&value)
        || is_config_root_value(&value)
    {
        strip_summary_debug_fields(&mut value);
        return value;
    }

    // host_state 默认不输出，除非 profile 显式打开（便于调试）。
    if !profile.include_host_state {
        if let Some(object) = value.as_object_mut() {
            object.remove("host_state");
        }
    }

    // bridge exchange 体积可能很大：request/exchange/follow_up/events 等。
    // 默认只保留 result；如果 events 是 stream 模式，还要避免重复携带 events。
    if !profile.include_bridge_exchange {
        strip_bridge_exchange(&mut value, profile);
    } else if profile.events == EventMode::Stream {
        strip_streamed_events(&mut value);
    }

    // 最后做文案本地化：确保裁剪后的结构不会再被其它逻辑改变。
    localize_json_strings(&mut value, profile.locale.as_str());
    value
}

fn is_template_info_value(value: &serde_json::Value) -> bool {
    value.get("kind").and_then(|kind| kind.as_str()) == Some("template_info")
}

fn is_locale_value(value: &serde_json::Value) -> bool {
    value.get("kind").and_then(|kind| kind.as_str()) == Some("locale")
}

fn is_config_value(value: &serde_json::Value) -> bool {
    value.get("kind").and_then(|kind| kind.as_str()) == Some("config_value")
}

fn is_config_root_value(value: &serde_json::Value) -> bool {
    value.get("kind").and_then(|kind| kind.as_str()) == Some("config")
}

fn strip_summary_debug_fields(value: &mut serde_json::Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.remove("context");
    object.remove("host_state");
    object.remove("exit_code");
}

fn strip_bridge_exchange(value: &mut serde_json::Value, profile: &OutputProfile) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if object.get("kind").and_then(|kind| kind.as_str()) != Some("bridge") {
        return;
    }

    // 兼容点：
    // - host 的 bridge execution 会带上 `request/exchange/follow_up` 等调试字段
    // - 真正用户关心的结果通常在 `exchange.response.result`
    // 因此这里把它抬到顶层 `result`，然后移除体积很大的字段，避免 JSON 输出爆炸。
    if let Some(exchange) = object.get("exchange").cloned() {
        let response_result = exchange
            .get("response")
            .and_then(|response| response.get("result"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        object.insert("result".into(), response_result);
    }

    object.remove("request");
    object.remove("exchange");
    object.remove("follow_up");

    // events=stream 时，事件已经逐条输出过了；这里再带上会造成重复。
    if profile.events == EventMode::Stream {
        object.remove("events");
    }
}

fn strip_streamed_events(value: &mut serde_json::Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };

    // stream 模式下 events 会实时输出：
    // - 如果最终 result 里还携带一份 events，会造成“重复输出”
    // - 但为了兼容下游解析器，这里不直接 remove 整个字段，而是置空数组
    if let Some(exchange) = object
        .get_mut("exchange")
        .and_then(|value| value.as_object_mut())
    {
        exchange.insert("events".into(), serde_json::Value::Array(vec![]));
    }
    if let Some(follow_up) = object
        .get_mut("follow_up")
        .and_then(|value| value.as_object_mut())
    {
        follow_up.insert("events".into(), serde_json::Value::Array(vec![]));
    }
}
