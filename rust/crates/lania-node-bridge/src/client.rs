//! Node Bridge（Rust 侧）客户端。
//!
//! 这个模块负责：
//! - 启动/复用一个 Node 子进程（默认 stdio 传输），并完成 handshake。
//! - 通过“请求-事件-响应”的 envelope 协议与 Node bridge 交互。
//! - 将每个 request 的 events 与最终 response 分发到对应的 channel。
//!
//! 并发模型（简化版）：
//! - 每次发送 request 都会在 `pending` 表中登记一个 `request_id -> (event_tx, response_tx)`。
//! - 后台 reader 线程持续读取 stdout，每行一个 JSON envelope：
//!   - event: 转发到对应 request 的 event_tx（或广播到 global_events）
//!   - response: 通过 response_tx 完成一次请求
//! - 通过 `max_pending_requests` 防止请求堆积导致内存增长。

use std::{
    path::PathBuf,
    process::{Child, ChildStdin},
    sync::{
        atomic::{AtomicU64, AtomicUsize},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, oneshot};

mod capabilities;
mod process;
mod transport;

#[cfg(test)]
mod tests;

use crate::protocol::{
    BridgeEvent, BridgeEventMethod, BridgeExchange, BridgeMetricsSnapshot, BridgeRequest,
    BridgeResponse, HandshakeRequest, HandshakeResponse,
};
use crate::HostRpcHandlerRef;

#[derive(Debug, Clone)]
pub struct BridgeClientConfig {
    // 这个配置基本决定了 Rust 客户端如何看待 Node bridge：
    // - 协议版本/编码：双方“说什么话”
    // - transport：通过什么通道说
    // - timeout / heartbeat：多久认为对方失联
    // - event_buffer_capacity / max_pending_requests：并发压力下如何控内存
    pub protocol_version: String,
    pub transport: String,
    pub encoding: String,
    pub timeout: Duration,
    pub prefer_process_transport: bool,
    pub event_buffer_capacity: usize,
    pub max_pending_requests: usize,
    pub heartbeat_timeout: Duration,
}

impl Default for BridgeClientConfig {
    fn default() -> Self {
        Self {
            protocol_version: "0.1.0".into(),
            transport: "stdio".into(),
            encoding: "json".into(),
            timeout: Duration::from_secs(30),
            prefer_process_transport: true,
            event_buffer_capacity: 32,
            max_pending_requests: 32,
            heartbeat_timeout: Duration::from_secs(45),
        }
    }
}

#[derive(Debug)]
pub struct NodeBridgeClient {
    config: BridgeClientConfig,
    // `sequence` 用原子计数器生成 request id，避免多线程/多任务并发发请求时冲突。
    sequence: Arc<AtomicU64>,
    // `state` 用 `Arc<Mutex<_>>`，原因和前面几个服务类似：
    // - NodeBridgeClient 会被 clone 到很多 workflow / capability / runtime 对象里；
    // - 这些 clone 应共享同一个 bridge 子进程和同一份 metrics；
    // - 因此需要共享所有权 + 互斥修改。
    state: Arc<Mutex<BridgeState>>,
}

struct BridgeState {
    process: Option<BridgeProcess>,
    metrics: Arc<BridgeMetrics>,
    // 全局事件广播器：
    // - 与某个 request 无关的 ready/heartbeat/log 都可以走这里
    // - 调试器、日志器、UI 都可以各自订阅一份 receiver
    global_events: broadcast::Sender<BridgeEvent>,
    host_rpc_handler: Option<HostRpcHandlerRef>,
}

impl std::fmt::Debug for BridgeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeState")
            .field("process", &self.process.is_some())
            .field("metrics", &"<metrics>")
            .field("global_events", &"<broadcast>")
            .field("host_rpc_handler", &self.host_rpc_handler.is_some())
            .finish()
    }
}

struct BridgeProcess {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    handshake: HandshakeResponse,
    // 每个 in-flight request 的分发表：reader 线程据此把 event/response 路由到调用方。
    pending: Arc<Mutex<std::collections::HashMap<String, PendingRequest>>>,
    // 最近一次收到 heartbeat 的时间戳，用于判断 bridge 是否“假死”。
    last_heartbeat: Arc<Mutex<Instant>>,
    reader: Option<thread::JoinHandle<()>>,
    stderr_reader: Option<thread::JoinHandle<()>>,
}

impl std::fmt::Debug for BridgeProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeProcess")
            .field("child", &self.child)
            .field("stdin", &"<child-stdin>")
            .field("handshake", &self.handshake)
            .field("pending", &self.pending)
            .field("last_heartbeat", &self.last_heartbeat.lock().ok())
            .field("reader_running", &self.reader.is_some())
            .finish()
    }
}

#[derive(Debug)]
struct PendingRequest {
    // 一个请求有两条“返回通道”：
    // - `event_tx`: 接收流式事件（可以有 0..n 条）
    // - `response_tx`: 接收最终响应（严格只有 1 条）
    event_tx: mpsc::Sender<BridgeEvent>,
    response_tx: oneshot::Sender<BridgeResponse>,
}

#[derive(Debug, Clone)]
struct BridgeLaunchSpec {
    package_dir: PathBuf,
    program: PathBuf,
    args: Vec<String>,
    mode: &'static str,
}

#[derive(Debug)]
enum BridgeEnvelope {
    Handshake {
        payload: HandshakeResponse,
    },
    Event {
        request_id: String,
        payload: BridgeEvent,
    },
    Response {
        payload: BridgeResponse,
    },
    HostRequest {
        payload: crate::protocol::HostRequest,
    },
}

pub struct BridgeActiveCall {
    // `BridgeActiveCall` 相当于“一个尚未收集完成的调用句柄”：
    // 调用方既可以边收 event，边等待 response；
    // 也可以直接 `collect_exchange()` 一次性收全。
    response_rx: oneshot::Receiver<BridgeResponse>,
    event_rx: mpsc::Receiver<BridgeEvent>,
}

#[derive(Debug, Default)]
struct BridgeMetrics {
    requests_sent: AtomicU64,
    responses_received: AtomicU64,
    events_received: AtomicU64,
    reconnects: AtomicU64,
    heartbeat_events: AtomicU64,
    timeouts: AtomicU64,
    errors: AtomicU64,
    max_pending_requests_seen: AtomicUsize,
}

#[derive(Clone)]
struct BridgeMetricsHandles {
    inner: Arc<BridgeMetrics>,
}

#[async_trait(?Send)]
pub trait ConfigBridgeCapability {
    async fn load_lan_config(&self, cwd: String) -> Result<BridgeExchange>;
    async fn load_tool_config(&self, cwd: String, tool: String) -> Result<BridgeExchange>;
}

#[async_trait(?Send)]
pub trait TemplateBridgeCapability {
    async fn list_templates(&self, cwd: String) -> Result<BridgeExchange>;
    async fn get_template_questions(
        &self,
        template: String,
        options: serde_json::Value,
    ) -> Result<BridgeExchange>;
    async fn get_template_dependencies(
        &self,
        template: String,
        options: serde_json::Value,
    ) -> Result<BridgeExchange>;
    async fn get_template_output_tasks(
        &self,
        template: String,
        options: serde_json::Value,
    ) -> Result<BridgeExchange>;
    async fn render_template(
        &self,
        template: String,
        context: serde_json::Value,
        options: serde_json::Value,
    ) -> Result<BridgeExchange>;
}

#[async_trait(?Send)]
pub trait AddTemplateBridgeCapability {
    async fn render_add_template(
        &self,
        template: String,
        context: serde_json::Value,
    ) -> Result<BridgeExchange>;
}

#[async_trait(?Send)]
pub trait CompilerBridgeCapability {
    async fn run_dev_server(&self, cwd: String, port: Option<u16>) -> Result<BridgeExchange>;
    async fn run_build(
        &self,
        cwd: String,
        watch: bool,
        mode: Option<String>,
        output_dir: Option<String>,
    ) -> Result<BridgeExchange>;
    async fn stop_compiler(&self) -> Result<BridgeExchange>;
}

#[async_trait(?Send)]
pub trait LintBridgeCapability {
    async fn run_lint(
        &self,
        cwd: String,
        fix: bool,
        concurrency: Option<usize>,
    ) -> Result<BridgeExchange>;
}

#[async_trait(?Send)]
pub trait CommitBridgeCapability {
    async fn run_commitizen(
        &self,
        cwd: String,
        kind: String,
        scope: Option<String>,
        subject: String,
    ) -> Result<BridgeExchange>;
    async fn run_commitlint(&self, cwd: String, message: String) -> Result<BridgeExchange>;
}

impl Clone for NodeBridgeClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            sequence: Arc::clone(&self.sequence),
            state: Arc::clone(&self.state),
        }
    }
}

impl BridgeActiveCall {
    pub async fn next_event(&mut self) -> Option<BridgeEvent> {
        self.event_rx.recv().await
    }

    pub async fn collect_exchange(mut self) -> Result<BridgeExchange> {
        let mut events = Vec::new();
        // `oneshot::Receiver` 不是 `Unpin` 友好地可反复借用的 future，
        // 所以这里先 pin 住，后面才能在 `select!` 循环里多次轮询它。
        let response_rx = self.response_rx;
        tokio::pin!(response_rx);

        loop {
            tokio::select! {
                // 这里的 `select!` 非常像“谁先到就先处理谁”的多路复用器：
                // - event 先来，就先累计 event
                // - response 先来，就把剩余 event 尽量收完然后返回
                maybe_event = self.event_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            // 事件先暂存在 Vec 里，而不是边收边做复杂处理，
                            // 是因为 `collect_exchange()` 的职责只是“收齐事实”，
                            // 如何解释这些 event 留给更高层的 host/runtime。
                            events.push(event)
                        },
                        None => {
                            // event channel 提前关闭时，仍然尝试等待最终 response。
                            // 因为“没有更多流式事件”并不等于“请求失败了”。
                            let response = response_rx
                                .await
                                .map_err(|_| anyhow!("bridge response channel closed unexpectedly"))?;
                            return Ok(BridgeExchange { response, events });
                        }
                    }
                }
                result = &mut response_rx => {
                    let response = result
                        .map_err(|_| anyhow!("bridge response channel closed unexpectedly"))?;
                    // response 到达后仍继续把 channel 里剩下的 event 收走，
                    // 避免“最后几条 event 比 response 晚几个调度片段”时被漏掉。
                    //
                    // 这说明协议层并没有强依赖“event 必须严格早于 response”；
                    // Rust 客户端自己做了一层容错收尾，让上层看到的 `BridgeExchange`
                    // 尽量接近完整事实。
                    while let Some(event) = self.event_rx.recv().await {
                        // 这里不再 `select!` 新的 response，是因为 response 已经拿到了；
                        // 剩下只是在做“尾流排空”。
                        events.push(event);
                    }
                    return Ok(BridgeExchange { response, events });
                }
            }
        }
    }
}
