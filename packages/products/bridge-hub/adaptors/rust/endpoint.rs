// 手写 Rust 端点 adaptor：进程内 channel，直接连 Hub（bridge-layer §4）。
//
// 由 `lania-bridge-bus` 通过 `include!` 编译进 `rust_adaptor` 模块。
// 这里只有业务回调；帧编解码、correlation id、错误包装由生成的 `runtime.rs` 负责。

use crate::Handler;
use lania_bridge_core::{bus_error, E_INVALID_PARAMS, E_METHOD_NOT_FOUND};
use lania_bridge_protocol::{BusErrorDetail, Value};

/// 能力处理器示例：`echo.ping` 回显参数，`echo.upper` 大写化文本。
#[derive(Debug, Default)]
pub struct EchoHandler {
    pub handled: usize,
}

impl Handler for EchoHandler {
    fn on_request(&mut self, capability: &str, params: Value) -> Result<Value, BusErrorDetail> {
        self.handled += 1;
        match capability {
            "echo.ping" => Ok(serde_json::json!({
                "pong": true,
                "params": params,
                "endpointId": "rust-endpoint",
            })),
            "echo.upper" => {
                let text = params.get("text").and_then(Value::as_str);
                match text {
                    Some(text) => Ok(serde_json::json!({
                        "text": text.to_uppercase(),
                        "endpointId": "rust-endpoint",
                    })),
                    None => Err(bus_error(E_INVALID_PARAMS, "text must be a string")),
                }
            }
            _ => Err(bus_error(E_METHOD_NOT_FOUND, "unsupported capability")),
        }
    }

    fn on_event(&mut self, _topic: &str, _payload: Value, _from: &str) {}
}

/// 声明的能力与订阅（供宿主装配时读取）。
pub const PROVIDES: &[&str] = &["echo.ping", "echo.upper"];
pub const SUBSCRIBES: &[&str] = &["event.diag", "event.agent_*"];
