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
    #[serde(rename = "event.agent_text_delta")]
    AgentTextDelta,
    #[serde(rename = "event.agent_tool_start")]
    AgentToolStart,
    #[serde(rename = "event.agent_tool_end")]
    AgentToolEnd,
    #[serde(rename = "event.agent_end")]
    AgentEnd,
    #[serde(rename = "event.shutdown")]
    Shutdown,
    #[serde(rename = "event.heartbeat")]
    Heartbeat,
    /// 所有「被劫持的终端输出」统一走这一条（§7.2）。
    ///
    /// 与 `event.log` 的区别：带 `source`（谁产生的）、`stream`（原始终端流）、
    /// `channel`（build / dev / lint / app）与单调递增的 `seq`（跨流合并顺序）。
    #[serde(rename = "event.diag")]
    Diag,
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

// -----------------------------
// 统一结果模型（§7.3）
// -----------------------------

/// 逐行日志的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagSource {
    /// 被劫持的工具链子进程输出。
    Tool,
    /// 运行时自身（bridge / 解释器）输出。
    Runtime,
    /// 宿主自身产生的诊断。
    Host,
}

/// 日志用途。`App` 表示应用自身日志（例如服务端 HTTP 访问日志）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagChannel {
    Build,
    Dev,
    Lint,
    App,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevelName {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// 一条原始流日志（`event.diag` 的载荷）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagLine {
    pub source: DiagSource,
    /// `stdout` / `stderr` / `syslog`
    pub stream: String,
    /// 具体工具（`source = Tool` 时必有）。
    #[serde(default)]
    pub tool: Option<String>,
    pub channel: DiagChannel,
    pub level: LogLevelName,
    /// 原始文本（未加工）。
    pub text: String,
    /// 单调递增，保证跨流合并顺序。
    pub seq: u64,
    /// 毫秒时间戳。
    #[serde(default)]
    pub ts: u64,
}

/// 诊断严重程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPosition {
    pub line: u64,
    pub col: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRange {
    pub start: DiagnosticPosition,
    #[serde(default)]
    pub end: Option<DiagnosticPosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticFix {
    pub available: bool,
    /// `safe` / `unsafe` / `none`
    pub kind: String,
}

/// 跨语言统一的诊断（所有 bridge 必须产出同构结构）。
///
/// 可选性约定：`file` / `range` 缺失 = 文件级诊断；
/// `id` 缺失 = 无规则号的通用错误（如编译失败）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub tool: String,
    #[serde(default)]
    pub id: Option<String>,
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub range: Option<DiagnosticRange>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub fix: Option<DiagnosticFix>,
}

impl Diagnostic {
    /// 去重键：`(tool, file, line, col, id)`。
    pub fn dedup_key(&self) -> (String, String, u64, u64, String) {
        (
            self.tool.clone(),
            self.file.clone().unwrap_or_default(),
            self.range.map(|range| range.start.line).unwrap_or_default(),
            self.range.map(|range| range.start.col).unwrap_or_default(),
            self.id.clone().unwrap_or_default(),
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagCounts {
    pub error: usize,
    pub warning: usize,
    pub info: usize,
    pub hint: usize,
}

impl DiagCounts {
    pub fn from_diagnostics(diagnostics: &[Diagnostic]) -> Self {
        let mut counts = Self::default();
        for diagnostic in diagnostics {
            match diagnostic.severity {
                DiagnosticSeverity::Error => counts.error += 1,
                DiagnosticSeverity::Warning => counts.warning += 1,
                DiagnosticSeverity::Info => counts.info += 1,
                DiagnosticSeverity::Hint => counts.hint += 1,
            }
        }
        counts
    }
}

/// 去重 + 排序（`§7.3`）。
///
/// 排序：`file` 升序 → `line` 升序 → `col` 升序 → `severity`（error 优先）→ `id`。
/// 同一位置的多个工具诊断**全部保留**，由渲染层按 `tool` 标签区分。
pub fn normalize_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(|diagnostic| seen.insert(diagnostic.dedup_key()))
        .collect();
    deduped.sort_by(|left, right| {
        let left_file = left.file.as_deref().unwrap_or("");
        let right_file = right.file.as_deref().unwrap_or("");
        left_file
            .cmp(right_file)
            .then_with(|| {
                let left_pos = left
                    .range
                    .map(|r| (r.start.line, r.start.col))
                    .unwrap_or((0, 0));
                let right_pos = right
                    .range
                    .map(|r| (r.start.line, r.start.col))
                    .unwrap_or((0, 0));
                left_pos.cmp(&right_pos)
            })
            .then_with(|| left.severity.cmp(&right.severity))
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.tool.cmp(&right.tool))
    });
    deduped
}

/// 从 `event.diag` 的 params 解析出一行日志；字段缺失时返回 `None`（忽略脏事件）。
pub fn parse_diag_line(params: &serde_json::Value) -> Option<DiagLine> {
    let channel = match params["channel"].as_str().unwrap_or("build") {
        "lint" => DiagChannel::Lint,
        "dev" => DiagChannel::Dev,
        "app" => DiagChannel::App,
        _ => DiagChannel::Build,
    };
    let source = match params["source"].as_str().unwrap_or("runtime") {
        "tool" => DiagSource::Tool,
        "host" => DiagSource::Host,
        _ => DiagSource::Runtime,
    };
    let level = match params["level"].as_str().unwrap_or("info") {
        "trace" => LogLevelName::Trace,
        "debug" => LogLevelName::Debug,
        "warn" => LogLevelName::Warn,
        "error" => LogLevelName::Error,
        _ => LogLevelName::Info,
    };
    Some(DiagLine {
        source,
        stream: params["stream"].as_str().unwrap_or("stdout").to_string(),
        tool: params["tool"].as_str().map(ToOwned::to_owned),
        channel,
        level,
        text: params["text"].as_str()?.to_string(),
        seq: params["seq"].as_u64().unwrap_or_default(),
        ts: params["ts"].as_u64().unwrap_or_default(),
    })
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
