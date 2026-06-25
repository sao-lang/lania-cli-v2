//! 宿主侧 Host RPC 分发与各域（exec/git/pm/fs/log/tasks/progress/interaction）处理入口。
//!
//! 背景：
//! - node-bridge 在执行 workflow/工具调用时，需要“反向调用宿主能力”，例如执行命令、读写文件、
//!   git 操作、安装依赖、记录日志、更新进度条、弹交互问题等。
//! - 这些调用通过 stdio 以 RPC 形式从 Node 发起（`host.xxx.*`），由 Rust 宿主处理后返回结果。
//!
//! 设计原则：
//! - 分发逻辑保持集中：入口只做 method 路由、审计与策略快照提取。
//! - 风险能力的策略校验（例如 exec/fs）必须在对应域内再次兜底校验，避免未来拆模块时丢失边界。
//! - 各域拥有自己的参数校验与返回结构（保持内部演进空间），但对外 method 名称要稳定。

use std::{path::Path, sync::Arc, time::Instant};

use anyhow::{anyhow, Result};
use lania_exec::{ExecCommand, ExecService};
use lania_fs::FsService;
use lania_git::GitService;
use lania_logger::{LogLevel, LoggerService};
use lania_node_bridge::{BridgeEvent, HostRpcHandler};
use lania_pm::PackageManagerService;
use lania_progress::ProgressService;
use lania_prompt::PromptService;
use lania_task::TaskService;
use serde_json::{json, Value};

mod exec;
mod fs_domain;
mod git;
mod interaction;
mod log;
mod pm;
mod task_progress;

#[derive(Debug, Clone)]
struct HostToolsPolicySnapshot {
    exec_allow_shell: Option<bool>,
    exec_allow_env_write: Option<bool>,
    fs_write_root: Option<String>,
}

fn tools_policy_from_payload(payload: &serde_json::Map<String, Value>) -> HostToolsPolicySnapshot {
    let policy = payload.get("__toolsPolicy").and_then(Value::as_object);
    let exec = policy
        .and_then(|p| p.get("exec"))
        .and_then(Value::as_object);
    let fs = policy.and_then(|p| p.get("fs")).and_then(Value::as_object);
    HostToolsPolicySnapshot {
        exec_allow_shell: exec
            .and_then(|e| e.get("allowShell"))
            .and_then(Value::as_bool),
        exec_allow_env_write: exec
            .and_then(|e| e.get("allowEnvWrite"))
            .and_then(Value::as_bool),
        fs_write_root: fs
            .and_then(|f| f.get("writeRoot"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    }
}

fn deny(message: &str) -> anyhow::Error {
    anyhow!("[E_TOOLS_DENIED] {message}")
}

fn normalize_path_buf(path: &Path) -> std::path::PathBuf {
    use std::path::{Component, PathBuf};
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

fn resolve_path_from_cwd(cwd: &str, target: &str) -> std::path::PathBuf {
    let target_path = Path::new(target);
    if target_path.is_absolute() {
        normalize_path_buf(target_path)
    } else {
        normalize_path_buf(&Path::new(cwd).join(target_path))
    }
}

fn is_under_write_root(cwd: &str, write_root: &str, target: &str) -> bool {
    let root = normalize_path_buf(&Path::new(cwd).join(write_root));
    let resolved = resolve_path_from_cwd(cwd, target);
    resolved == root || resolved.starts_with(&root)
}

fn payload_required_str(
    payload: &serde_json::Map<String, Value>,
    key: &str,
    method: &str,
) -> Result<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{method} requires `{key}`"))
}

fn payload_optional_str(payload: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn payload_bool(payload: &serde_json::Map<String, Value>, key: &str, default: bool) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn payload_strings(payload: &serde_json::Map<String, Value>, key: &str) -> Vec<String> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn command_to_json(command: ExecCommand) -> Value {
    json!({
        "program": command.program,
        "args": command.args,
        "cwd": command.cwd,
        "env": command.env,
        "useShell": command.use_shell,
    })
}

type HostPayload = serde_json::Map<String, Value>;
type HostRpcResponse = (Value, Vec<BridgeEvent>);

fn payload_cwd(payload: &HostPayload) -> &str {
    payload.get("cwd").and_then(Value::as_str).unwrap_or(".")
}

fn ensure_write_root_allowed(
    tools_policy: &HostToolsPolicySnapshot,
    cwd: &str,
    path: &str,
    method: &str,
) -> Result<()> {
    if let Some(write_root) = tools_policy.fs_write_root.as_deref() {
        if !is_under_write_root(cwd, write_root, path) {
            return Err(deny(&format!(
                "{method} is blocked: path is outside writeRoot"
            )));
        }
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct HostRpcAdapter {
    exec: ExecService,
    git: GitService,
    package_manager: PackageManagerService,
    fs: FsService,
    logger: LoggerService,
    tasks: TaskService,
    progress: ProgressService,
    prompt: PromptService,
}

#[derive(Clone)]
pub(crate) struct HostRpcAdapterDeps {
    pub(crate) exec: ExecService,
    pub(crate) git: GitService,
    pub(crate) package_manager: PackageManagerService,
    pub(crate) fs: FsService,
    pub(crate) logger: LoggerService,
    pub(crate) tasks: TaskService,
    pub(crate) progress: ProgressService,
    pub(crate) prompt: PromptService,
}

impl HostRpcAdapter {
    pub(crate) fn new(deps: HostRpcAdapterDeps) -> Arc<Self> {
        Arc::new(Self {
            exec: deps.exec,
            git: deps.git,
            package_manager: deps.package_manager,
            fs: deps.fs,
            logger: deps.logger,
            tasks: deps.tasks,
            progress: deps.progress,
            prompt: deps.prompt,
        })
    }
    /// 将 `host.*` 形式的 RPC 调用路由到各个 domain handler。
    ///
    /// 契约（Contract）：
    /// - `payload` 必须是 JSON object（在 `HostRpcHandler::handle` 里做了校验）。
    /// - 每个 domain handler 自己负责参数校验与返回值结构。
    /// - 对外的 RPC method 名称保持稳定；这里的调整仅影响内部路由组织。
    ///
    /// 边界（Boundary）：
    /// - 风险能力（例如 `exec` 与 `fs`）的策略校验必须留在所属 domain 内，
    ///   这样未来继续拆分模块时仍能保持同一个“强制执行点”。
    fn dispatch_host_rpc(
        &self,
        method: &str,
        payload: &HostPayload,
        tools_policy: &HostToolsPolicySnapshot,
    ) -> Result<HostRpcResponse> {
        if method.starts_with("host.exec.") {
            return self.handle_exec_domain(method, payload, tools_policy);
        }
        if method.starts_with("host.git.") {
            return self.handle_git_domain(method, payload);
        }
        if method.starts_with("host.pm.") {
            return self.handle_pm_domain(method, payload);
        }
        if method.starts_with("host.fs.") {
            return self.handle_fs_domain(method, payload, tools_policy);
        }
        if method.starts_with("host.log.") {
            return self.handle_log_domain(method, payload);
        }
        if method.starts_with("host.tasks.") {
            return self.handle_tasks_domain(method, payload);
        }
        if method.starts_with("host.progress.") {
            return self.handle_progress_domain(method, payload);
        }
        if method.starts_with("host.interaction.") {
            return self.handle_interaction_domain(method, payload);
        }
        Err(anyhow!("unsupported host rpc method: {method}"))
    }

    /// 处理 `host.exec.*`。
    ///
    /// 输入（Inputs）：
    /// - `cwd` 默认是 `.`
    /// - `timeoutMs` 单位是毫秒
    /// - `env` 只接受 string 值（避免把复杂 JSON 结构注入进进程环境）
    ///
    /// 返回（Returns）：
    /// - 仍然保持 JSON 可序列化的命令执行快照（给 node-bridge 使用）
    ///
    /// 边界（Boundary）：
    /// - 在宿主侧再次强制执行 `allowShell` / `allowEnvWrite` 策略，避免绕过
    fn handle_exec_domain(
        &self,
        method: &str,
        payload: &HostPayload,
        tools_policy: &HostToolsPolicySnapshot,
    ) -> Result<HostRpcResponse> {
        exec::handle_exec_domain(self, method, payload, tools_policy)
    }

    /// 处理 `host.git.*`。
    ///
    /// 输入（Inputs）：
    /// - 大多数操作默认 `cwd` 为 `.`
    /// - 会对修改性操作做必要参数校验（branch/remote/tag 等）
    ///
    /// 返回（Returns）：
    /// - 稳定的 JSON object 或序列化后的 git service DTO
    ///
    /// 边界（Boundary）：
    /// - 这里不再额外叠加策略层；命令是否可用由“namespace allow/deny”在进入 host 前决定
    fn handle_git_domain(&self, method: &str, payload: &HostPayload) -> Result<HostRpcResponse> {
        git::handle_git_domain(self, method, payload)
    }

    /// 处理 `host.pm.*`（包管理/脚本计划等）。
    ///
    /// 输入（Inputs）：
    /// - 包管理器选择目前主要由 `cwd` 推断
    /// - script/package 数组只接受 string 元素
    ///
    /// 返回（Returns）：
    /// - 序列化后的 manager spec、package 快照、或可执行命令计划
    ///
    /// 边界（Boundary）：
    /// - 这里尽量保持 read/plan 导向；真正的进程执行仍然走 `host.exec.*`
    fn handle_pm_domain(&self, method: &str, payload: &HostPayload) -> Result<HostRpcResponse> {
        pm::handle_pm_domain(self, method, payload)
    }

    /// 处理 `host.fs.*`。
    ///
    /// 输入（Inputs）：
    /// - 每个操作都要求 `path`
    /// - 写操作会基于 `cwd` 解析相对路径
    ///
    /// 返回（Returns）：
    /// - 小型 JSON 快照（`exists` / `content` / `stat` / `removed` 等）
    ///
    /// 边界（Boundary）：
    /// - 对每个写入路径强制执行 `writeRoot` 限制，避免越权写文件
    fn handle_fs_domain(
        &self,
        method: &str,
        payload: &HostPayload,
        tools_policy: &HostToolsPolicySnapshot,
    ) -> Result<HostRpcResponse> {
        fs_domain::handle_fs_domain(self, method, payload, tools_policy)
    }

    /// 处理 `host.log.*`。
    ///
    /// 输入（Inputs）：
    /// - `message` 是必填字段（用于 emit 等）
    /// - trace/phase/target 等元数据会原样透传
    ///
    /// 返回（Returns）：
    /// - ack object 或序列化后的内存 log entries
    ///
    /// 边界（Boundary）：
    /// - 审计/事件面保持在 `LoggerService` 内集中处理
    fn handle_log_domain(&self, method: &str, payload: &HostPayload) -> Result<HostRpcResponse> {
        log::handle_log_domain(self, method, payload)
    }

    /// 处理 `host.tasks.*`。
    ///
    /// 输入（Inputs）：
    /// - 修改性调用要求稳定的 task id
    /// - register 还要求 title/group/priority 等元数据
    ///
    /// 返回（Returns）：
    /// - ack object 或序列化后的 task 快照/事件流
    ///
    /// 边界（Boundary）：
    /// - 任务状态修改全部集中在 `TaskService` 内
    fn handle_tasks_domain(&self, method: &str, payload: &HostPayload) -> Result<HostRpcResponse> {
        task_progress::handle_tasks_domain(self, method, payload)
    }

    /// 处理 `host.progress.*`。
    ///
    /// 输入（Inputs）：
    /// - 所有修改性调用都要求 progress item `id`
    /// - parent-child API 还要求 `parentId`
    ///
    /// 返回（Returns）：
    /// - ack object + 序列化后的快照/事件/summary/渲染输出
    ///
    /// 边界（Boundary）：
    /// - 渲染与终端暂停逻辑完全由 `ProgressService` 承担（避免在 RPC 层散落）
    fn handle_progress_domain(
        &self,
        method: &str,
        payload: &HostPayload,
    ) -> Result<HostRpcResponse> {
        task_progress::handle_progress_domain(self, method, payload)
    }

    /// 处理 `host.interaction.*`。
    ///
    /// 输入（Inputs）：
    /// - 单步 helper 会从 wire payload 推导出 prompt step
    /// - flow API 需要 `questions` 或 `steps`
    ///
    /// 返回（Returns）：
    /// - 单个 `answer` 包裹结构，或序列化后的 `PromptState`
    ///
    /// 边界（Boundary）：
    /// - prompt DSL 的转换逻辑留在 runtime 内部，不把 PromptService 内部细节暴露到 RPC 边界外
    fn handle_interaction_domain(
        &self,
        method: &str,
        payload: &HostPayload,
    ) -> Result<HostRpcResponse> {
        interaction::handle_interaction_domain(self, method, payload)
    }

    fn audit_tool_call(&self, method: &str, payload: &HostPayload, started: Instant, ok: bool) {
        // tool call 审计是 best-effort：
        // - 审计失败绝不能影响真实的 tool call 执行结果
        // - 默认用 TRACE 级别，避免污染用户输出；可通过 LANIA_TRACE=1 或 LANIA_LOG_LEVEL=trace 打开
        let duration_ms = started.elapsed().as_millis() as u64;
        let cwd = payload_cwd(payload);
        let trace_id = payload
            .get("traceId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let (tool, tool_method) = parse_tool_method(method);
        self.logger.log_with_context(
            LogLevel::Trace,
            "host.tool",
            serde_json::json!({
                "tool": tool,
                "method": tool_method,
                "rpcMethod": method,
                "cwd": cwd,
                "traceId": trace_id,
                "durationMs": duration_ms,
                "ok": ok,
            })
            .to_string(),
            trace_id.clone(),
            Some("tool_call".into()),
            Some(method.to_string()),
        );
    }
}

impl HostRpcHandler for HostRpcAdapter {
    fn handle(&self, method: &str, params: Value) -> Result<(Value, Vec<BridgeEvent>)> {
        let started = Instant::now();
        let payload = params
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow!("host rpc params must be an object"))?;
        let tools_policy = tools_policy_from_payload(&payload);
        let out = self.dispatch_host_rpc(method, &payload, &tools_policy);
        self.audit_tool_call(method, &payload, started, out.is_ok());
        out
    }
}

fn parse_tool_method(method: &str) -> (String, String) {
    // method 示例：
    // - host.exec.run
    // - host.pm.command.runScript
    // - host.interaction.multiSelect
    let stripped = method.strip_prefix("host.").unwrap_or(method);
    let mut parts = stripped.split('.');
    let tool = parts.next().unwrap_or("host").to_string();
    let rest = parts.collect::<Vec<_>>().join(".");
    if rest.is_empty() {
        (tool.clone(), tool)
    } else {
        (tool, rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_prompt_runs_in_scripted_mode_via_host_rpc() {
        let adapter = HostRpcAdapter::new(HostRpcAdapterDeps {
            exec: ExecService::new(false),
            git: GitService::default(),
            package_manager: PackageManagerService,
            fs: FsService,
            logger: LoggerService::default(),
            tasks: TaskService::default(),
            progress: ProgressService::default(),
            prompt: PromptService::default(),
        });

        let (result, events) = adapter
            .handle(
                "host.interaction.prompt",
                json!({
                    "questions": [
                        {
                            "field": "name",
                            "message": "Project name?",
                            "kind": "input"
                        },
                        {
                            "field": "env",
                            "message": "Environment?",
                            "kind": "select",
                            "choices": [
                                { "label": "dev", "value": "dev" },
                                { "label": "prod", "value": "prod" }
                            ]
                        }
                    ],
                    "answers": {
                        "name": "demo-app",
                        "env": "prod"
                    }
                }),
            )
            .expect("interaction prompt should succeed");

        assert!(events.is_empty());
        assert_eq!(result["answers"]["name"], json!("demo-app"));
        assert_eq!(result["answers"]["env"], json!("prod"));
        assert_eq!(result["interrupted"], json!(false));
    }

    #[test]
    fn host_policy_blocks_fs_write_outside_write_root() {
        let adapter = HostRpcAdapter::new(HostRpcAdapterDeps {
            exec: ExecService::new(false),
            git: GitService::default(),
            package_manager: PackageManagerService,
            fs: FsService,
            logger: LoggerService::default(),
            tasks: TaskService::default(),
            progress: ProgressService::default(),
            prompt: PromptService::default(),
        });

        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let err = adapter
            .handle(
                "host.fs.write",
                json!({
                    "cwd": cwd,
                    "path": "../outside.txt",
                    "content": "x",
                    "__toolsPolicy": { "fs": { "writeRoot": ".allowed" } }
                }),
            )
            .expect_err("expected policy to block fs write");

        assert!(err.to_string().contains("E_TOOLS_DENIED"));
    }

    #[test]
    fn host_policy_blocks_exec_shell_when_disabled() {
        let adapter = HostRpcAdapter::new(HostRpcAdapterDeps {
            exec: ExecService::new(false),
            git: GitService::default(),
            package_manager: PackageManagerService,
            fs: FsService,
            logger: LoggerService::default(),
            tasks: TaskService::default(),
            progress: ProgressService::default(),
            prompt: PromptService::default(),
        });

        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let err = adapter
            .handle(
                "host.exec.shell",
                json!({
                    "cwd": cwd,
                    "script": "echo hi",
                    "__toolsPolicy": { "exec": { "allowShell": false } }
                }),
            )
            .expect_err("expected policy to block exec shell");

        assert!(err.to_string().contains("E_TOOLS_DENIED"));
    }
}
