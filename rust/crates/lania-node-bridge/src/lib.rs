//! Rust 侧 Node bridge 客户端、请求构造与生命周期封装。
pub mod client;
pub mod protocol;

use anyhow::Result;
use std::sync::Arc;

pub use client::{
    AddTemplateBridgeCapability, BridgeActiveCall, BridgeClientConfig, CommitBridgeCapability,
    CompilerBridgeCapability, ConfigBridgeCapability, LintBridgeCapability, NodeBridgeClient,
    TemplateBridgeCapability,
};
pub use protocol::{
    BridgeError, BridgeEvent, BridgeEventMethod, BridgeExchange, BridgeFailureStrategy,
    BridgeMetricsSnapshot, BridgeRequest, BridgeResponse, HandshakeRequest, HandshakeResponse,
    HostRequest, HostResponse,
};

/// Rust 宿主侧用于处理“Node 主动发起的 host RPC 调用”的接口。
///
/// 具体实现通常放在宿主运行时 crate（例如 `lania-host`）里，
/// 然后安装到 `NodeBridgeClient` 上，
/// 这样 node-bridge 就能通过 stdio 反向调用宿主能力。
pub trait HostRpcHandler: Send + Sync {
    fn handle(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(serde_json::Value, Vec<crate::protocol::BridgeEvent>)>;
}

pub type HostRpcHandlerRef = Arc<dyn HostRpcHandler>;
