use anyhow::{anyhow, Result};
use lania_logger::{render_ascii_banner, LogEntry, LogLevel};
use serde_json::json;

use super::{payload_required_str, HostPayload, HostRpcAdapter, HostRpcResponse};

/// log rpc 被刻意从其它 host-rpc 逻辑中独立出来：
/// - 它主要做的是元数据适配与序列化形状转换
/// - 真正的日志存储/渲染仍然集中在 `LoggerService`
/// 这样可以避免“日志协议适配”和“日志系统实现”耦合在一起。
pub(super) fn handle_log_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<HostRpcResponse> {
    match method {
        "host.log.emit" => {
            let level = payload
                .get("level")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("info");
            let message = payload_required_str(payload, "message", method)?;
            let target = payload
                .get("target")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("schema");
            let trace_id = payload
                .get("traceId")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            let phase = payload
                .get("phase")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            let operation = payload
                .get("operation")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            let level = match level {
                "trace" => LogLevel::Trace,
                "debug" => LogLevel::Debug,
                "warn" => LogLevel::Warn,
                "error" => LogLevel::Error,
                _ => LogLevel::Info,
            };
            adapter
                .logger
                .log_with_context(level, target, &message, trace_id, phase, operation);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.log.entries" => {
            let entries: Vec<LogEntry> = adapter.logger.entries();
            Ok((serde_json::to_value(entries)?, Vec::new()))
        }
        "host.log.clear" => {
            adapter.logger.clear();
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.log.ascii" => {
            let message = payload_required_str(payload, "message", method)?;
            Ok((
                json!({ "lines": render_ascii_banner(&message) }),
                Vec::new(),
            ))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}
