//! 所有 bridge request 的统一构造器。
//!
//! 这个文件的价值在于“把 method 名和 params 结构集中收口”：
//! - 上层不需要到处手写 `"config.loadLan"` / `"compiler.build"` 这类字符串
//! - request id 的生成也统一在这里完成
//! - 协议字段变化时，只需要集中改这一层
//!
//! 也可以把它看成 Node bridge 协议的 Rust 侧“请求工厂”。

use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use super::super::*;

impl NodeBridgeClient {
    pub fn new(config: BridgeClientConfig) -> Self {
        let (global_events, _) = broadcast::channel(config.event_buffer_capacity.max(1));
        Self {
            config,
            state: Arc::new(Mutex::new(BridgeState {
                process: None,
                metrics: Arc::new(BridgeMetrics::default()),
                global_events,
                host_rpc_handler: None,
            })),
        }
    }

    pub fn with_host_rpc_handler(self, handler: crate::HostRpcHandlerRef) -> Self {
        self.state
            .lock()
            .expect("bridge state poisoned")
            .host_rpc_handler = Some(handler);
        self
    }

    /// 取回已安装的 host RPC handler，供 Hub provider 复用同一份实现。
    pub fn host_rpc_handler(&self) -> Option<crate::HostRpcHandlerRef> {
        self.state
            .lock()
            .expect("bridge state poisoned")
            .host_rpc_handler
            .clone()
    }

    /// 用一个显式的启动描述替换默认的 node-bridge 定位逻辑。
    pub fn with_launch_spec(mut self, spec: BridgeLaunchSpec) -> Self {
        self.config.launch_spec = Some(spec);
        self
    }

    /// 直接指定 `program` / `args` / `cwd`（`launch_spec` 的便捷写法）。
    pub fn with_program(
        self,
        program: impl Into<PathBuf>,
        args: Vec<String>,
        dir: impl Into<PathBuf>,
    ) -> Self {
        self.with_launch_spec(BridgeLaunchSpec::for_program(program, args, dir))
    }

    pub fn handshake_request(&self) -> HandshakeRequest {
        HandshakeRequest {
            protocol_version: self.config.protocol_version.clone(),
            transport: self.config.transport.clone(),
            encoding: self.config.encoding.clone(),
            // `host_name` 是个很小但很有用的字段：
            // bridge 侧日志/调试工具可以知道“是谁在连我”，便于多宿主复用同一协议时区分来源。
            host_name: "lania-host".into(),
        }
    }

    pub fn request(&self, method: impl Into<String>, params: serde_json::Value) -> BridgeRequest {
        // 统一走协议层的纯构造函数（req-N 全局单调）；不依赖 client 的进程/连接状态。
        BridgeRequest::new(method, params)
    }
}
