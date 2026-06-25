//! Rust 与 Node bridge 共享的请求、响应、事件与握手协议模型。
//!
//! 主要导出：HandshakeRequest、HandshakeResponse、BridgeRequest、BridgeResponse、BridgeError、BridgeEvent。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeRequest {
    /// 协议版本（用于 host/bridge 能力协商与向后兼容）。
    pub protocol_version: String,
    /// 传输类型（例如 `stdio` / `ipc` 等），便于 bridge 做差异化实现。
    pub transport: String,
    /// 编码约定（目前主要影响序列化与 envelope 格式）。
    pub encoding: String,
    /// Host 端标识，用于日志/诊断。
    pub host_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeResponse {
    /// bridge 端最终确认的协议版本。
    pub protocol_version: String,
    /// bridge 实现名称（用于诊断与兼容性判断）。
    pub bridge_name: String,
    /// bridge 支持的方法列表（例如 `compiler.build`）。
    pub methods: Vec<String>,
    /// bridge 会发送的事件类型（用于 host 侧筛选/降级）。
    pub events: Vec<BridgeEventMethod>,
    /// 心跳间隔：bridge 会周期性发送 `event.heartbeat`。
    pub heartbeat_interval_ms: u64,
    /// bridge 端允许积压的待发送事件数量上限（用于避免内存增长）。
    pub max_pending_events: usize,
    /// 失败策略：出现心跳超时/读写失败时是 fail-fast 还是允许重连。
    pub failure_strategy: BridgeFailureStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeFailureStrategy {
    /// 失败即终止：由 host 层决定是否/如何重试。
    FailFast,
    /// 允许重连：host 侧可在心跳超时等情况下重启 bridge 进程。
    Reconnect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRequest {
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<BridgeError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeError {
    pub code: String,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeEventMethod {
    #[serde(rename = "event.ready")]
    Ready,
    #[serde(rename = "event.log")]
    Log,
    #[serde(rename = "event.progress")]
    Progress,
    #[serde(rename = "event.dev_url")]
    DevUrl,
    #[serde(rename = "event.build_asset")]
    BuildAsset,
    #[serde(rename = "event.compiler_start")]
    CompilerStart,
    #[serde(rename = "event.compiler_status")]
    CompilerStatus,
    #[serde(rename = "event.compiler_server_ready")]
    CompilerServerReady,
    #[serde(rename = "event.compiler_asset")]
    CompilerAsset,
    #[serde(rename = "event.compiler_issue")]
    CompilerIssue,
    #[serde(rename = "event.compiler_watch_change")]
    CompilerWatchChange,
    #[serde(rename = "event.compiler_done")]
    CompilerDone,
    #[serde(rename = "event.lint_start")]
    LintStart,
    #[serde(rename = "event.lint_file")]
    LintFile,
    #[serde(rename = "event.lint_result")]
    LintResult,
    #[serde(rename = "event.lint_summary")]
    LintSummary,
    #[serde(rename = "event.watch_change")]
    WatchChange,
    #[serde(rename = "event.shutdown")]
    Shutdown,
    #[serde(rename = "event.heartbeat")]
    Heartbeat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeEvent {
    pub method: BridgeEventMethod,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeExchange {
    pub response: BridgeResponse,
    pub events: Vec<BridgeEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BridgeMetricsSnapshot {
    pub requests_sent: u64,
    pub responses_received: u64,
    pub events_received: u64,
    pub reconnects: u64,
    pub heartbeat_events: u64,
    pub timeouts: u64,
    pub errors: u64,
    pub max_pending_requests_seen: usize,
}

// -----------------------------
// Host RPC（Node -> Rust）
// -----------------------------

/// 由 node-bridge 进程发起、用于回调 Rust 宿主能力的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRequest {
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// 由 Rust 宿主主动返回给 node-bridge 进程的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostResponse {
    pub id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<BridgeError>,
    #[serde(default)]
    pub events: Vec<BridgeEvent>,
}
