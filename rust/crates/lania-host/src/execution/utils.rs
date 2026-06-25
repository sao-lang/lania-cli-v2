//! 命令执行阶段用到的一些小型纯函数工具。
//!
//! 这类文件通常不长，但非常关键：
//! - `redact_secret_fields()` 负责保证日志和输出不会泄露敏感值
//! - `human_event_message()` 把 bridge event 翻译成更适合终端即时展示的人类文案
//!
//! 可以把这里理解成“桥接层和输出层之间的适配胶水”。

use lania_node_bridge::{BridgeEvent, BridgeEventMethod};
use serde_json::{json, Value};

pub(crate) fn redact_secret_fields(value: Value, secret_fields: &[String]) -> Value {
    // 这里做的是“结构保持型脱敏”：
    // - 不删除字段
    // - 不改对象/数组形状
    // - 只把命中的敏感值替换成 `***`
    //
    // 这样下游日志/调试工具仍能看到完整 payload 结构，只是看不到真正的秘密值。
    match value {
        Value::Object(map) => {
            let mut redacted = serde_json::Map::new();
            for (key, value) in map {
                // 递归替换敏感字段：保持结构不变，只替换字段值，便于下游继续使用 payload 结构。
                // 这里按“字段名命中”脱敏，而不是按值模式（例如像 token 的字符串）猜测，
                // 是为了避免误伤普通文本，同时把“哪些字段敏感”的决定权留给 prompt/runtime。
                if secret_fields.iter().any(|field| field == &key) {
                    redacted.insert(key, json!("***"));
                } else {
                    redacted.insert(key, redact_secret_fields(value, secret_fields));
                }
            }
            Value::Object(redacted)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redact_secret_fields(item, secret_fields))
                .collect(),
        ),
        other => other,
    }
}

pub(crate) fn human_event_message(event: &BridgeEvent) -> String {
    // 这个函数的目标不是覆盖所有 event，而是给“终端即时输出”提供少量高频友好文案。
    // 对不常见事件保留 Debug 风格字符串，能避免维护一个过大且容易过期的映射表。
    match event.method {
        BridgeEventMethod::Log => event.params["message"]
            .as_str()
            .unwrap_or("bridge log")
            .to_string(),
        BridgeEventMethod::Progress => event.params["message"]
            .as_str()
            .unwrap_or("progress update")
            .to_string(),
        // 仅对少量常见 event 做“友好消息”转换；
        // 其它事件保留 Debug 字符串，避免维护不完整的映射表。
        BridgeEventMethod::DevUrl => format!(
            "dev url: {}",
            event.params["url"].as_str().unwrap_or("unknown")
        ),
        BridgeEventMethod::Shutdown => format!(
            "shutdown: {}",
            event.params["reason"].as_str().unwrap_or("requested")
        ),
        // 其余事件直接退回方法名，意味着：
        // - 至少还能给人一个“发生了哪类事件”的即时提示
        // - 但不会为了少见事件维护一大堆容易过期的人类文案模板
        _ => format!("{:?}", event.method),
    }
}
