//! bridge events 到宿主副作用的映射逻辑。

use std::io::Write;

use lania_logger::LogLevel;
use lania_node_bridge::{BridgeEvent, BridgeEventMethod, BridgeExchange, BridgeRequest};

use super::super::{context::CommandExecutionContext, utils::human_event_message};

impl<'a> CommandExecutionContext<'a> {
    pub(in crate::execution) fn apply_bridge_events(
        &self,
        request: &BridgeRequest,
        exchange: &BridgeExchange,
    ) {
        // 事件处理是“副作用优先”：即使最终 response.error 需要上抛，
        // 也应该先把 events 里的诊断信息展示出来。
        for event in &exchange.events {
            self.maybe_stream_bridge_event(request, event);
            self.log(
                LogLevel::Trace,
                "command_execute",
                Some(request.method.clone()),
                format!("bridge event: {:?}", event.method),
            );
            match event.method {
                BridgeEventMethod::Log => {
                    let level = match event.params["level"].as_str().unwrap_or("info") {
                        "trace" => LogLevel::Trace,
                        "debug" => LogLevel::Debug,
                        "warn" => LogLevel::Warn,
                        "error" => LogLevel::Error,
                        _ => LogLevel::Info,
                    };
                    let message = event.params["message"]
                        .as_str()
                        .unwrap_or("bridge event")
                        .to_string();
                    self.log(
                        level,
                        "command_execute",
                        Some(request.method.clone()),
                        message,
                    );
                }
                BridgeEventMethod::Progress => {
                    let id = self.progress_id_for_event(request, event);
                    let current = event.params["current"].as_u64().unwrap_or_default();
                    let total = event.params["total"].as_u64();
                    if self.progress.snapshot().iter().all(|item| item.id != id) {
                        self.progress.begin(&id, total);
                    }
                    let existing = self
                        .progress
                        .snapshot()
                        .into_iter()
                        .find(|item| item.id == id)
                        .map(|item| item.current)
                        .unwrap_or_default();
                    if current > existing {
                        self.progress.advance(&id, current - existing);
                    }
                    if let Some(message) = event.params["message"].as_str() {
                        self.progress.message(&id, message);
                    }
                }
                BridgeEventMethod::DevUrl => {
                    if let Some(url) = event.params["url"].as_str() {
                        self.log(
                            LogLevel::Info,
                            "command_execute",
                            Some(request.method.clone()),
                            format!("dev url: {url}"),
                        );
                    }
                }
                BridgeEventMethod::BuildAsset => {
                    if let Some(file) = event.params["file"].as_str() {
                        self.log(
                            LogLevel::Info,
                            "command_execute",
                            Some(request.method.clone()),
                            format!(
                                "built asset: {file} ({})",
                                event.params["bytes"].as_u64().unwrap_or_default()
                            ),
                        );
                    }
                }
                BridgeEventMethod::CompilerStart => {
                    let id = self.progress_id_for_event(request, event);
                    if self.progress.snapshot().iter().all(|item| item.id != id) {
                        self.progress.begin(&id, None);
                    }
                    let tool = event.params["tool"].as_str().unwrap_or("compiler");
                    let action = event.params["action"].as_str().unwrap_or("run");
                    self.progress.message(&id, format!("{tool} {action}"));
                    self.log(
                        LogLevel::Info,
                        "command_execute",
                        Some(request.method.clone()),
                        format!(
                            "compiler start: tool={tool}, action={action}, workerMode={}, isolated={}",
                            event.params["workerMode"].as_str().unwrap_or("inline_bridge"),
                            event.params["isolated"].as_bool().unwrap_or(false)
                        ),
                    );
                }
                BridgeEventMethod::CompilerStatus => {
                    let id = self.progress_id_for_event(request, event);
                    if self.progress.snapshot().iter().all(|item| item.id != id) {
                        self.progress.begin(&id, None);
                    }
                    let stage = event.params["stage"].as_str().unwrap_or("running");
                    let message = event.params["message"]
                        .as_str()
                        .unwrap_or("compiler status update");
                    self.progress.message(&id, message);
                    self.progress.detail(&id, format!("stage={stage}"));
                    self.log(
                        LogLevel::Info,
                        "command_execute",
                        Some(request.method.clone()),
                        format!("compiler status [{stage}]: {message}"),
                    );
                }
                BridgeEventMethod::CompilerServerReady => {
                    let url = event.params["url"].as_str().unwrap_or("unknown");
                    let id = self.progress_id_for_event(request, event);
                    if self.progress.snapshot().iter().any(|item| item.id == id) {
                        self.progress.detail(&id, format!("server={url}"));
                    }
                    self.log(
                        LogLevel::Info,
                        "command_execute",
                        Some(request.method.clone()),
                        format!("compiler server ready: {url}"),
                    );
                }
                BridgeEventMethod::CompilerAsset => {
                    if let Some(file) = event.params["file"].as_str() {
                        self.log(
                            LogLevel::Info,
                            "command_execute",
                            Some(request.method.clone()),
                            format!(
                                "compiler asset: {file} ({})",
                                event.params["bytes"].as_u64().unwrap_or_default()
                            ),
                        );
                    }
                }
                BridgeEventMethod::CompilerIssue => {
                    let severity = event.params["severity"].as_str().unwrap_or("warning");
                    let message = event.params["message"].as_str().unwrap_or("compiler issue");
                    let level = if severity == "error" {
                        LogLevel::Error
                    } else {
                        LogLevel::Warn
                    };
                    self.log(
                        level,
                        "command_execute",
                        Some(request.method.clone()),
                        format!("compiler issue [{severity}]: {message}"),
                    );
                }
                BridgeEventMethod::CompilerWatchChange => {
                    if let Some(path) = event.params["path"].as_str() {
                        self.log(
                            LogLevel::Info,
                            "command_execute",
                            Some(request.method.clone()),
                            format!(
                                "compiler watch change: {} {}",
                                event.params["change"].as_str().unwrap_or("updated"),
                                path
                            ),
                        );
                    }
                }
                BridgeEventMethod::CompilerDone => {
                    let id = self.progress_id_for_event(request, event);
                    let success = event.params["success"].as_bool().unwrap_or(true);
                    if self.progress.snapshot().iter().any(|item| item.id == id) {
                        if success {
                            self.progress.finish(&id);
                        } else {
                            self.progress.fail(&id, "compiler reported failure");
                        }
                    }
                    self.log(
                        if success {
                            LogLevel::Info
                        } else {
                            LogLevel::Error
                        },
                        "command_execute",
                        Some(request.method.clone()),
                        format!(
                            "compiler done: action={}, success={}, implementation={}",
                            event.params["action"].as_str().unwrap_or("run"),
                            success,
                            event.params["implementation"].as_str().unwrap_or("unknown")
                        ),
                    );
                }
                BridgeEventMethod::LintStart => {
                    let adaptors = event.params["adaptors"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|value| value.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.log(
                        LogLevel::Info,
                        "command_execute",
                        Some(request.method.clone()),
                        format!("lint start: {adaptors}"),
                    );
                }
                BridgeEventMethod::LintFile => {
                    let path = event.params["filePath"].as_str().unwrap_or(".");
                    let adaptor = event.params["adaptor"].as_str().unwrap_or("unknown");
                    let errors = event.params["errors"].as_u64().unwrap_or_default();
                    let warnings = event.params["warnings"].as_u64().unwrap_or_default();
                    self.log(
                        if errors > 0 {
                            LogLevel::Warn
                        } else {
                            LogLevel::Debug
                        },
                        "command_execute",
                        Some(request.method.clone()),
                        format!("{adaptor} file {path}: {errors} errors, {warnings} warnings"),
                    );
                }
                BridgeEventMethod::LintResult => {
                    let adaptor = event.params["adaptor"].as_str().unwrap_or("unknown");
                    let errors = event.params["errors"].as_u64().unwrap_or_default();
                    let warnings = event.params["warnings"].as_u64().unwrap_or_default();
                    self.log(
                        if errors > 0 {
                            LogLevel::Warn
                        } else {
                            LogLevel::Info
                        },
                        "command_execute",
                        Some(request.method.clone()),
                        format!("{adaptor}: {errors} errors, {warnings} warnings"),
                    );
                }
                BridgeEventMethod::LintSummary => {
                    let errors = event.params["errors"].as_u64().unwrap_or_default();
                    let warnings = event.params["warnings"].as_u64().unwrap_or_default();
                    let exit_code = event.params["exitCode"].as_i64().unwrap_or_default();
                    self.log(
                        if errors > 0 {
                            LogLevel::Warn
                        } else {
                            LogLevel::Info
                        },
                        "command_execute",
                        Some(request.method.clone()),
                        format!(
                            "lint summary: {errors} errors, {warnings} warnings, exitCode={exit_code}"
                        ),
                    );
                }
                BridgeEventMethod::WatchChange => {
                    if let Some(path) = event.params["path"].as_str() {
                        self.log(
                            LogLevel::Info,
                            "command_execute",
                            Some(request.method.clone()),
                            format!("watch change: {path}"),
                        );
                    }
                }
                BridgeEventMethod::Shutdown => {
                    self.log(
                        LogLevel::Info,
                        "command_execute",
                        Some(request.method.clone()),
                        format!(
                            "bridge shutdown: {}",
                            event.params["reason"].as_str().unwrap_or("requested")
                        ),
                    );
                }
                BridgeEventMethod::Heartbeat => {
                    self.log(
                        LogLevel::Debug,
                        "command_execute",
                        Some(request.method.clone()),
                        "bridge heartbeat received",
                    );
                }
                BridgeEventMethod::Ready => {}
            }
        }
    }

    fn maybe_stream_bridge_event(&self, request: &BridgeRequest, event: &BridgeEvent) {
        let Some(config) = self.project_config() else {
            return;
        };
        if config.ui.output.events != "stream" {
            return;
        }
        let envelope = serde_json::json!({
            "kind": "event",
            "requestId": request.id,
            "method": request.method,
            "event": event,
        });
        // 这里根据输出模式选择 stdout / stderr / human 文本：
        // - `jsonl` 走 stdout，适合脚本消费
        // - `human` 走 stderr，适合人看
        match config.ui.output.mode.as_str() {
            "jsonl" => {
                let mut stdout = std::io::stdout().lock();
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&envelope).expect("stream envelope serializes")
                );
            }
            "human" => {
                eprintln!("[event:{}] {}", request.method, human_event_message(event));
            }
            _ => {
                let mut stderr = std::io::stderr().lock();
                let _ = writeln!(
                    stderr,
                    "{}",
                    serde_json::to_string(&envelope).expect("stream envelope serializes")
                );
            }
        }
    }

    fn progress_id_for_event(&self, request: &BridgeRequest, event: &BridgeEvent) -> String {
        // 相同 bridge 事件在不同 grouping 策略下，会归到不同进度条。
        let grouping = self
            .project_config()
            .map(|config| config.ui.progress.grouping.as_str())
            .unwrap_or("command");
        match grouping {
            "task" => request.method.clone(),
            "operation" => event.params["operationId"]
                .as_str()
                .or_else(|| event.params["taskId"].as_str())
                .or_else(|| event.params["file"].as_str())
                .or_else(|| event.params["filePath"].as_str())
                .or_else(|| event.params["adaptor"].as_str())
                .or_else(|| event.params["tool"].as_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| request.id.clone()),
            _ => request.id.clone(),
        }
    }
}
