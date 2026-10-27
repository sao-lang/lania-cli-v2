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
use lania_node_bridge::{BridgeExchange, BridgeRequest, NodeBridgeClient};
use lania_workflows::WorkflowExecution;
use serde_json::json;

use super::bridge_helpers::BridgeDispatchResult;
use super::context::{CommandExecutionContext, ExecutionError};
use super::types::{BridgeCommandRun, CommandExecution, EXIT_CANCELLED, EXIT_RUNTIME_ERROR};
use crate::RuntimeKind;

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
        let BridgeDispatchResult { exchange, active } =
            self.dispatch_bridge_request(&request).await?;
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
        let (follow_up, interrupted) = self
            .maybe_wait_for_shutdown(&request, &exchange, active)
            .await?;
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

    /// 独立运行时 bridge 调用（§5.3 首选路径）。
    ///
    /// 与 `call_bridge` 的区别：这里的目标客户端由调用方指定（`lan-py-bridge` 等），
    /// 不走 `host_state.node_bridge`，也不做长任务/事件订阅（各语言 bridge 的
    /// lint/build 都是「请求 → 响应」，不需要常驻订阅）。
    /// 返回 `Err` 表示 bridge 不可用 —— 调用方据此回落到 Rust 侧 CLI spawn。
    pub async fn call_runtime_bridge(
        &self,
        client: &NodeBridgeClient,
        request: BridgeRequest,
    ) -> Result<BridgeExchange> {
        self.log(
            LogLevel::Debug,
            "command_execute",
            Some(request.method.clone()),
            format!("dispatch runtime bridge request {}", request.method),
        );
        let start = client.open_call(request.clone());
        let exchange = match start {
            Ok(mut call) => tokio::time::timeout(
                std::time::Duration::from_millis(self.policy.timeout_ms),
                call.collect_until_response(|event| self.apply_bridge_event(&request, event)),
            )
            .await
            .map_err(|_| ExecutionError {
                exit_code: EXIT_RUNTIME_ERROR,
                message: format!("runtime bridge request {} timed out", request.method),
            })??,
            Err(_) => tokio::time::timeout(
                std::time::Duration::from_millis(self.policy.timeout_ms),
                client.call_async(request.clone()),
            )
            .await
            .map_err(|_| ExecutionError {
                exit_code: EXIT_RUNTIME_ERROR,
                message: format!("runtime bridge request {} timed out", request.method),
            })??,
        };
        if let Some(error) = exchange.response.error.as_ref() {
            return Err(ExecutionError {
                exit_code: EXIT_RUNTIME_ERROR,
                message: format!(
                    "runtime bridge error for {}: [{}] {}",
                    request.method, error.code, error.message
                ),
            }
            .into());
        }
        Ok(exchange)
    }

    /// 运行时判定（§5.1）并在有歧义时给一条 warning，返回最终生效的运行时。
    ///
    /// 只做「提示」不做决策：`--runtime` 与 `lan.config.runtime` 仍然优先。
    /// 放在 `execution` 层是因为 `lint` / `build` / `dev` 三个 handler 都要用同一套提示。
    pub fn log_runtime_decision(&self, cli_runtime: Option<&str>) -> RuntimeKind {
        let cwd = std::path::PathBuf::from(&self.command.cwd);
        if self.command.cwd.is_empty() {
            return RuntimeKind::Node;
        }
        let decision = crate::runtime::decide_runtime(&cwd, self.project_config(), cli_runtime);
        if let Some(notice) = decision.notice {
            self.log(
                LogLevel::Warn,
                "command_execute",
                Some(self.command.handler_id.clone()),
                notice,
            );
        }
        decision.kind
    }

    /// 解析「某运行时的某个动作」应当执行什么命令（`build` / `dev`）。
    ///
    /// 优先级（§5.2：语言知识只允许一处定义）：
    /// 1. `lan.config.runtimes.<id>.<action>` —— 用户在配置里显式覆盖；
    /// 2. JS 簇 bridge 的 `toolchain.describe` —— `toolchain.ts` 是唯一权威来源，
    ///    Rust 不维护第二份「默认 build 命令」表；
    /// 3. `None` —— 调用方据此**不路由**到语言 bridge（回落 JS 簇路径）。
    ///
    /// `output` 会替换命令里的 `{output}` 占位符（Go 的 `go build -o {output}`）。
    pub async fn resolve_runtime_command(
        &self,
        kind: RuntimeKind,
        action: &str,
        output: Option<&str>,
    ) -> Option<Vec<String>> {
        let configured = self
            .project_config()
            .and_then(|config| config.runtimes.get(kind.id()))
            .and_then(|override_| match action {
                "build" => override_.build.clone(),
                "dev" => override_.dev.clone(),
                _ => None,
            })
            .filter(|command| !command.is_empty());

        let raw = match configured {
            Some(command) => command,
            None => {
                let fallback = self.toolchain_command(kind, action).await?;
                if fallback.is_empty() {
                    return None;
                }
                fallback
            }
        };

        let output = output.unwrap_or("bin/app");
        Some(
            raw.into_iter()
                .map(|arg| arg.replace("{output}", output))
                .collect(),
        )
    }

    /// 从 `toolchain.describe` 里取某个运行时的 `buildCommand` / `devCommand`。
    ///
    /// 只做一次请求、不做进程内缓存：`build` / `lint` 每条命令最多问一次，
    /// 而缓存会让「改了 toolchain.ts 但没重启 CLI」的场景读到旧值。
    async fn toolchain_command(&self, kind: RuntimeKind, action: &str) -> Option<Vec<String>> {
        let client = self.node_bridge();
        let request = client.request("toolchain.describe", serde_json::json!({}));
        let exchange = tokio::time::timeout(
            std::time::Duration::from_millis(self.policy.timeout_ms),
            client.call_async(request),
        )
        .await
        .ok()?
        .ok()?;
        let result = exchange.response.result?;
        let runtimes = result.get("runtimes")?.as_array()?;
        let entry = runtimes
            .iter()
            .find(|entry| entry.get("id").and_then(|id| id.as_str()) == Some(kind.id()))?;
        let key = if action == "dev" {
            "devCommand"
        } else {
            "buildCommand"
        };
        Some(
            entry
                .get(key)?
                .as_array()?
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect(),
        )
    }

    /// 把运行时 bridge 的交换结果包装成命令执行结果。
    pub fn complete_runtime_bridge(
        &self,
        request: BridgeRequest,
        exchange: BridgeExchange,
        exit_code: i32,
    ) -> CommandExecution {
        CommandExecution::RuntimeBridge {
            context: self.command.clone(),
            request,
            exchange,
            host_state: self.host_state(),
            exit_code,
        }
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
