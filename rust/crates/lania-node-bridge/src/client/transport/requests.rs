//! 所有 bridge request 的统一构造器。
//!
//! 这个文件的价值在于“把 method 名和 params 结构集中收口”：
//! - 上层不需要到处手写 `"config.loadLan"` / `"compiler.build"` 这类字符串
//! - request id 的生成也统一在这里完成
//! - 协议字段变化时，只需要集中改这一层
//!
//! 也可以把它看成 Node bridge 协议的 Rust 侧“请求工厂”。

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use tokio::sync::broadcast;

use super::super::*;

impl NodeBridgeClient {
    pub fn new(config: BridgeClientConfig) -> Self {
        let (global_events, _) = broadcast::channel(config.event_buffer_capacity.max(1));
        Self {
            config,
            sequence: Arc::new(AtomicU64::new(0)),
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
        // request id 统一在客户端侧单调递增生成，
        // 这样 reader 路由、日志、问题排查都能稳定依赖同一种 id 格式。
        let id = format!("req-{}", self.sequence.fetch_add(1, Ordering::SeqCst) + 1);
        BridgeRequest {
            id,
            method: method.into(),
            params,
        }
    }

    pub fn ping_request(&self) -> BridgeRequest {
        self.request("bridge.ping", serde_json::json!({}))
    }

    pub fn load_lan_config_request(&self, cwd: impl Into<String>) -> BridgeRequest {
        self.request("config.loadLan", serde_json::json!({ "cwd": cwd.into() }))
    }

    pub fn load_tool_config_request(
        &self,
        cwd: impl Into<String>,
        tool: impl Into<String>,
    ) -> BridgeRequest {
        self.request(
            "config.loadTool",
            serde_json::json!({
                "cwd": cwd.into(),
                "tool": tool.into(),
            }),
        )
    }

    pub fn compiler_dev_request(&self, cwd: impl Into<String>, port: Option<u16>) -> BridgeRequest {
        self.request(
            "compiler.dev",
            serde_json::json!({
                "cwd": cwd.into(),
                "port": port,
            }),
        )
    }

    pub fn compiler_build_with_options_request(
        &self,
        cwd: impl Into<String>,
        watch: bool,
        mode: Option<String>,
        output_dir: Option<String>,
    ) -> BridgeRequest {
        // 比起只暴露一个“最小 build request”，这里保留带 options 的版本，
        // 是为了让上层调用点明确表达“我要不要 watch / mode / outputDir”，
        // 而不是到处手写 JSON object。
        self.request(
            "compiler.build",
            serde_json::json!({
                "cwd": cwd.into(),
                "watch": watch,
                "mode": mode,
                "outputDir": output_dir,
            }),
        )
    }

    pub fn compiler_build_request(&self, cwd: impl Into<String>, watch: bool) -> BridgeRequest {
        self.compiler_build_with_options_request(cwd, watch, None, None)
    }

    pub fn compiler_stop_request(&self) -> BridgeRequest {
        self.request("compiler.stop", serde_json::json!({}))
    }

    pub fn lint_run_request(
        &self,
        cwd: impl Into<String>,
        fix: bool,
        concurrency: Option<usize>,
    ) -> BridgeRequest {
        self.request(
            "lint.run",
            serde_json::json!({
                "cwd": cwd.into(),
                "fix": fix,
                "concurrency": concurrency,
            }),
        )
    }

    pub fn system_list_commands_request(
        &self,
        cwd: impl Into<String>,
        filter: Option<String>,
        limit: Option<usize>,
        all_matches: bool,
        include_shell: bool,
    ) -> BridgeRequest {
        self.request(
            "system.listCommands",
            serde_json::json!({
                "cwd": cwd.into(),
                "filter": filter,
                "limit": limit,
                "allMatches": all_matches,
                "includeShell": include_shell,
            }),
        )
    }

    pub fn template_list_request(&self, cwd: impl Into<String>) -> BridgeRequest {
        self.request("template.list", serde_json::json!({ "cwd": cwd.into() }))
    }

    pub fn template_questions_request(
        &self,
        template: impl Into<String>,
        options: serde_json::Value,
    ) -> BridgeRequest {
        self.request(
            "template.getQuestions",
            serde_json::json!({ "template": template.into(), "options": options }),
        )
    }

    pub fn template_dependencies_request(
        &self,
        template: impl Into<String>,
        options: serde_json::Value,
    ) -> BridgeRequest {
        self.request(
            "template.getDependencies",
            serde_json::json!({ "template": template.into(), "options": options }),
        )
    }

    pub fn template_output_tasks_request(
        &self,
        template: impl Into<String>,
        options: serde_json::Value,
    ) -> BridgeRequest {
        self.request(
            "template.getOutputTasks",
            serde_json::json!({ "template": template.into(), "options": options }),
        )
    }

    pub fn template_render_request(
        &self,
        template: impl Into<String>,
        context: serde_json::Value,
        options: serde_json::Value,
    ) -> BridgeRequest {
        self.request(
            "template.render",
            serde_json::json!({
                "template": template.into(),
                "context": context,
                "options": options,
            }),
        )
    }

    pub fn add_template_render_request(
        &self,
        template: impl Into<String>,
        context: serde_json::Value,
    ) -> BridgeRequest {
        self.request(
            "addTemplate.render",
            serde_json::json!({
                "template": template.into(),
                "context": context,
            }),
        )
    }

    pub fn commitizen_run_request(
        &self,
        cwd: impl Into<String>,
        kind: impl Into<String>,
        scope: Option<String>,
        subject: impl Into<String>,
    ) -> BridgeRequest {
        self.request(
            "commitizen.run",
            serde_json::json!({
                "cwd": cwd.into(),
                "kind": kind.into(),
                "scope": scope,
                "subject": subject.into(),
            }),
        )
    }

    pub fn commitlint_run_request(
        &self,
        cwd: impl Into<String>,
        message: impl Into<String>,
    ) -> BridgeRequest {
        self.request(
            "commitlint.run",
            serde_json::json!({
                "cwd": cwd.into(),
                "message": message.into(),
            }),
        )
    }

    pub fn shutdown_request(&self) -> BridgeRequest {
        self.request("bridge.shutdown", serde_json::json!({}))
    }

    pub fn metrics_request(&self, cwd: Option<String>) -> BridgeRequest {
        self.request("bridge.metrics", serde_json::json!({ "cwd": cwd }))
    }

    pub fn subscribe_request(&self, cwd: Option<String>) -> BridgeRequest {
        // 订阅类请求也走普通 request 工厂，说明在协议层它和其它 RPC 没有本质区别：
        // 差别主要体现在“返回的 events 会更多、更持续”。
        self.request("bridge.subscribe", serde_json::json!({ "cwd": cwd }))
    }
}
