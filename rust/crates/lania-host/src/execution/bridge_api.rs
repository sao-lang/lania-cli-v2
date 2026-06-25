//! Host 执行层与 Node bridge 的高层 API 胶水。
//!
//! 这一层站在“命令执行”的角度包装 bridge：
//! - 发请求前后触发 hooks
//! - 把 bridge events 映射成日志/进度/用户可见输出
//! - 把协议错误整理成 host 统一的 `ExecutionError`
//! - 对长任务补上“等待 Ctrl-C -> 发 shutdown”的交互语义
//!
//! 真正的 transport、超时与重试细节在 `bridge_helpers.rs`，这里更关注“对命令来说一次 bridge 调用意味着什么”。

use anyhow::Result;
use lania_hooks::hook_keys;
use lania_logger::LogLevel;
use lania_node_bridge::BridgeRequest;
use lania_workflows::WorkflowExecution;
use serde_json::json;

use super::context::CommandExecutionContext;
use super::types::{BridgeCommandRun, CommandExecution, EXIT_CANCELLED};

impl<'a> CommandExecutionContext<'a> {
    pub async fn call_bridge(&self, mut request: BridgeRequest) -> Result<BridgeCommandRun> {
        // Bridge 调用生命周期：
        // 1) dispatch 请求并收集 exchange（response + events）。
        // 2) 先消费 events（日志/进度等副作用），再决定是否把 response.error 上抛。
        // 3) 如果是长任务（dev/watch/longRunning），等待 Ctrl-C 并发送 follow-up shutdown。
        self.log(
            LogLevel::Debug,
            "command_execute",
            Some(request.method.clone()),
            format!(
                "dispatch bridge request {} with retry {} and timeout {}ms",
                request.method, self.policy.retry_attempts, self.policy.timeout_ms
            ),
        );

        // before hook（waterfall）允许插件“改写 params”：
        // - 用于注入默认值、做参数归一化、或加上审计字段
        // - 注意：这里改写的是 request.params，不应改写 request.id/method（否则无法路由）
        let before_payload = self
            .hooks
            .call_waterfall(
                "host-runtime".to_string(),
                hook_keys::ON_PLUGIN_API_CALL.to_string(),
                json!({
                    "cwd": self.command.cwd,
                    "traceId": self.command.trace_id,
                    "command": { "name": self.command.handler_id, "handlerId": self.command.handler_id },
                    "call": {
                        "stage": "before",
                        "plugin": request.method.split('.').next().unwrap_or("bridge"),
                        "method": request.method,
                        "params": request.params
                    }
                }),
            )
            .await
            .unwrap_or_else(|_| {
                // before hook 失败时回退到原始 payload：
                // 插件增强可以失效，但不应该阻断真实 bridge 调用本身。
                json!({
                    "cwd": self.command.cwd,
                    "traceId": self.command.trace_id,
                    "command": { "name": self.command.handler_id, "handlerId": self.command.handler_id },
                    "call": {
                        "stage": "before",
                        "plugin": request.method.split('.').next().unwrap_or("bridge"),
                        "method": request.method,
                        "params": request.params
                    }
                })
            });
        if let Some(params) = before_payload
            .get("call")
            .and_then(|call| call.get("params"))
            .cloned()
        {
            // 这里只接收 params 回写，等于显式限制了 waterfall hook 的改写边界。
            // 即使插件返回了别的字段，也不会影响真正发出去的 request.id / request.method。
            request.params = params;
        }

        // 真正发起 bridge 调用（包含 timeout/retry/流式 events 的处理），见 bridge_helpers.rs。
        let exchange = self.dispatch_bridge_request(&request).await?;
        if let Some(error) = exchange.response.error.as_ref() {
            // after hook 使用 parallel：不影响主流程（仅用于观测/上报）。
            // 失败和成功都发 after hook，说明 hook 的职责是“记录这次调用发生了什么”，
            // 不是“只有成功时才有意义”。
            self.hooks
                .call_parallel(
                    "host-runtime".to_string(),
                    hook_keys::ON_PLUGIN_API_CALL.to_string(),
                    json!({
                        "cwd": self.command.cwd,
                        "traceId": self.command.trace_id,
                        "command": { "name": self.command.handler_id, "handlerId": self.command.handler_id },
                        "call": {
                            "stage": "after",
                            "plugin": request.method.split('.').next().unwrap_or("bridge"),
                            "method": request.method,
                            "params": request.params,
                            "ok": false,
                            "error": { "code": error.code, "message": error.message }
                        }
                    }),
                )
                .await
                .ok();
        } else {
            self.hooks
                .call_parallel(
                    "host-runtime".to_string(),
                    hook_keys::ON_PLUGIN_API_CALL.to_string(),
                    json!({
                        "cwd": self.command.cwd,
                        "traceId": self.command.trace_id,
                        "command": { "name": self.command.handler_id, "handlerId": self.command.handler_id },
                        "call": {
                            "stage": "after",
                            "plugin": request.method.split('.').next().unwrap_or("bridge"),
                            "method": request.method,
                            "params": request.params,
                            "ok": true
                        }
                    }),
                )
                .await
                .ok();
        }
        // 统一把协议错误映射成 ExecutionError（保留 exit code），并附带足够上下文用于定位。
        self.raise_bridge_error(&request, &exchange.response.error)?;

        // 对“长任务”做特殊处理：等待用户 Ctrl-C，然后发送 follow-up shutdown 请求。
        // 这让 dev/watch 场景不会“命令立刻返回”，而是保持与用户交互一致。
        let (follow_up, interrupted) = self.maybe_wait_for_shutdown(&request, &exchange).await?;
        if let Some(shutdown) = follow_up.as_ref() {
            // follow-up shutdown 本身也可能失败，因此这里同样要经过统一错误映射。
            self.raise_bridge_error(&request, &shutdown.response.error)?;
        }

        Ok(BridgeCommandRun {
            request,
            exchange,
            follow_up,
            interrupted,
        })
    }

    pub fn complete_bridge(&self, run: BridgeCommandRun, exit_code: i32) -> CommandExecution {
        CommandExecution::Bridge {
            context: self.command.clone(),
            request: run.request,
            exchange: run.exchange,
            follow_up: run.follow_up,
            host_state: self.host_state(),
            // 用户手动中断长任务时，最终 exit code 以“已取消”为准，
            // 不再沿用原本业务命令的 exit code。
            exit_code: if run.interrupted {
                EXIT_CANCELLED
            } else {
                exit_code
            },
        }
    }

    pub fn complete_workflow(
        &self,
        execution: WorkflowExecution,
        exit_code: i32,
    ) -> CommandExecution {
        CommandExecution::Workflow {
            context: self.command.clone(),
            execution,
            host_state: self.host_state(),
            exit_code,
        }
    }

    pub fn complete_template_info(
        &self,
        output: serde_json::Value,
        exit_code: i32,
    ) -> CommandExecution {
        CommandExecution::TemplateInfo {
            context: self.command.clone(),
            output,
            host_state: self.host_state(),
            exit_code,
        }
    }

    pub async fn emit_workflow_start(&self, workflow: &str) {
        self.hooks
            .call_parallel(
                "host-runtime".to_string(),
                hook_keys::ON_WORKFLOW_START.to_string(),
                json!({
                    "cwd": self.command.cwd,
                    "traceId": self.command.trace_id,
                    "command": { "name": self.command.handler_id, "handlerId": self.command.handler_id },
                    "workflow": { "name": workflow }
                }),
            )
        .await
        .ok();
    }

    pub async fn emit_workflow_complete(&self, workflow: &str) {
        self.hooks
            .call_parallel(
                "host-runtime".to_string(),
                hook_keys::ON_WORKFLOW_COMPLETE.to_string(),
                json!({
                    "cwd": self.command.cwd,
                    "traceId": self.command.trace_id,
                    "command": { "name": self.command.handler_id, "handlerId": self.command.handler_id },
                    "workflow": { "name": workflow, "state": "completed" }
                }),
            )
            .await
            .ok();
    }
}
