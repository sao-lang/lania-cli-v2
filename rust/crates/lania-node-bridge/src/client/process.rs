//! Node bridge 子进程侧的低层进程/IO 处理。
//!
//! 这个文件解决的是最底层的几个问题：
//! - bridge 子进程应该如何启动
//! - stdout/stderr 线程如何把文本或 envelope 重新路由回 Rust
//! - in-flight 请求如何通过 `pending` 表匹配到对应调用方
//! - mock 模式下，在没有真实 bridge 进程时怎样返回结构稳定的兜底结果
//!
//! 可以把它看成 `NodeBridgeClient` 背后的“进程驱动层”。

use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{ChildStdin, ChildStdout},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use tokio::sync::broadcast;

use crate::protocol::{BridgeError, BridgeEvent, BridgeEventMethod, BridgeResponse, HostRequest};

use super::{
    BridgeEnvelope, BridgeLaunchSpec, BridgeMetrics, BridgeMetricsHandles, PendingRequest,
};

static BRIDGE_ENV_MUTEX: Mutex<()> = Mutex::new(());

impl BridgeMetricsHandles {
    pub(super) fn new(metrics: &Arc<BridgeMetrics>) -> Self {
        // 这里返回的是对 metrics 的共享句柄，供多个线程（stdout reader / stderr reader）并发更新。
        Self {
            inner: Arc::clone(metrics),
        }
    }
}

pub(super) fn mock_build_tool(cwd: &str) -> &'static str {
    // mock 模式下只能做非常粗略的推断（基于配置文件文本包含关系），
    // 目的是让“没有 bridge 进程”时也能返回结构稳定的结果，方便 CLI 演示/开发。
    let Some(config) = mock_lan_config_contents(cwd) else {
        return "vite";
    };
    if config.contains("buildTool") && config.contains("webpack") {
        "webpack"
    } else if config.contains("buildTool") && config.contains("rollup") {
        "rollup"
    } else {
        "vite"
    }
}

pub(super) fn mock_lint_tools(cwd: &str) -> Vec<String> {
    let Some(config) = mock_lan_config_contents(cwd) else {
        return vec!["eslint".into()];
    };
    let mut tools = Vec::new();
    if config.contains("oxlint") {
        tools.push("oxlint".into());
    }
    if config.contains("eslint") {
        tools.push("eslint".into());
    }
    if config.contains("oxfmt") {
        tools.push("oxfmt".into());
    }
    if config.contains("prettier") {
        tools.push("prettier".into());
    }
    if tools.is_empty() {
        // mock 没识别出任何工具时，也给一个保守默认值，
        // 避免上层把“空数组”误解释成“项目明确配置了不需要 lint”。
        tools.push("eslint".into());
    }
    tools
}

pub(super) fn mock_lan_config_contents(cwd: &str) -> Option<String> {
    if cwd.is_empty() {
        return None;
    }
    // mock 只读取 js/cjs/ts（不读 json/yaml），避免引入更多解析依赖；
    // 这条路径主要用于 process transport 不可用时的兜底。
    ["lan.config.js", "lan.config.cjs", "lan.config.ts"]
        .iter()
        .map(|name| Path::new(cwd).join(name))
        .find_map(|path| fs::read_to_string(path).ok())
}

pub(super) fn mock_lan_config_value(cwd: &str) -> serde_json::Value {
    let Some(config) = mock_lan_config_contents(cwd) else {
        // 返回空对象而不是报错，说明 mock 路径的目标是“结构稳定的兜底结果”，
        // 而不是高保真还原真实配置。
        return serde_json::json!({});
    };
    let build_tool = if config.contains("buildTool") && config.contains("webpack") {
        "webpack"
    } else if config.contains("buildTool") && config.contains("rollup") {
        "rollup"
    } else {
        "vite"
    };

    let mut lint_tools = Vec::new();
    if config.contains("oxlint") {
        lint_tools.push("oxlint");
    }
    if config.contains("eslint") {
        lint_tools.push("eslint");
    }
    if config.contains("oxfmt") {
        lint_tools.push("oxfmt");
    }
    if config.contains("prettier") {
        lint_tools.push("prettier");
    }

    let mut plugins = Vec::new();
    if config.contains("@demo/project-plugin") {
        plugins.push(serde_json::json!("@demo/project-plugin"));
    }
    if config.contains("@lania/plugin-custom-template") {
        plugins.push(serde_json::json!("@lania/plugin-custom-template"));
    }
    if config.contains("./scripts/lania.plugin") {
        plugins.push(serde_json::json!("./scripts/lania.plugin.ts"));
    }
    if config.contains("/abs/not-allowed") {
        plugins.push(serde_json::json!("/abs/not-allowed.js"));
    }

    // 这里刻意只拼出少量关键字段，让上层能覆盖最常见的演示/测试路径。
    // 换句话说，mock config 更像“协议样本”，不是完整配置解析器。
    serde_json::json!({
        "buildTool": build_tool,
        "buildAdaptors": {},
        "lintAdaptors": {},
        "lintTools": lint_tools,
        "plugins": plugins,
    })
}

pub(super) fn pump_process_events(
    mut stdout: BufReader<ChildStdout>,
    stdin: Arc<Mutex<ChildStdin>>,
    // `pending` 保存“已经发出去、但还没收到最终 response 的请求”。
    //
    // 为什么这里是 `Arc<Mutex<HashMap<...>>>`？
    // - stdout 读线程会根据 `request_id` 查找/移除待完成请求；
    // - 发请求的一侧也会往这个表里插入新的 pending 项；
    // - 因此这是典型的“多线程共享可变状态”场景：
    //   - `Arc` 共享所有权
    //   - `Mutex` 保证 HashMap 的并发访问安全
    pending: Arc<Mutex<std::collections::HashMap<String, PendingRequest>>>,
    global_events: broadcast::Sender<BridgeEvent>,
    last_heartbeat: Arc<Mutex<Instant>>,
    metrics: BridgeMetricsHandles,
    host_rpc_handler: Option<crate::HostRpcHandlerRef>,
) {
    loop {
        match read_envelope(&mut stdout) {
            Ok(BridgeEnvelope::Event {
                request_id,
                payload,
            }) => {
                metrics
                    .inner
                    .events_received
                    .fetch_add(1, Ordering::Relaxed);
                if payload.method == BridgeEventMethod::Heartbeat {
                    metrics
                        .inner
                        .heartbeat_events
                        .fetch_add(1, Ordering::Relaxed);
                    *last_heartbeat.lock().expect("bridge heartbeat poisoned") = Instant::now();
                }
                // `broadcast::Sender` 很适合这种“一份事件，多个观察者都想收到”的场景。
                // 例如日志输出、调试探针、全局状态机都可能订阅同一条 bridge event。
                //
                // global_events：无论是否属于某个 request，都广播出去（例如 ready/heartbeat）。
                // per-request events：如果 request_id 命中 pending，则额外投递到该调用方的 event_rx。
                let _ = global_events.send(payload.clone());
                if let Some(request) = pending
                    .lock()
                    .expect("bridge pending store poisoned")
                    .get(&request_id)
                {
                    // 对单个请求的事件流：只投递给对应 requestId 的调用方。
                    // 这里用 `blocking_send`，是因为当前代码运行在线程式 reader 中，
                    // 不是 async task；直接用异步 `send().await` 反而不合适。
                    let _ = request.event_tx.blocking_send(payload);
                }
            }
            Ok(BridgeEnvelope::Response { payload }) => {
                metrics
                    .inner
                    .responses_received
                    .fetch_add(1, Ordering::Relaxed);
                if let Some(request) = pending
                    .lock()
                    .expect("bridge pending store poisoned")
                    .remove(&payload.id)
                {
                    // 收到 response 代表请求结束：从 pending 移除并完成 oneshot。
                    // 注意这里是 `remove` 而不是 `get`：
                    // response 是“一次性终局消息”，一旦送达，这个 request 就不再是 in-flight。
                    let _ = request.response_tx.send(payload);
                }
            }
            Ok(BridgeEnvelope::HostRequest { payload }) => {
                let response = handle_host_request(payload, host_rpc_handler.as_deref());
                let envelope = serde_json::json!({
                    "type": "host_response",
                    "payload": response,
                });
                let write_result = {
                    let mut stdin = stdin.lock().expect("bridge stdin poisoned");
                    write_envelope(&mut stdin, &envelope)
                };
                if let Err(error) = write_result {
                    metrics.inner.errors.fetch_add(1, Ordering::Relaxed);
                    fail_pending_requests(
                        &pending,
                        format!("failed to write host_response to node bridge: {error}"),
                    );
                    return;
                }
            }
            Ok(BridgeEnvelope::Handshake { .. }) => {
                // 正常情况下 handshake 只会在进程刚启动时出现，
                // 后续 reader 再看到它，多半只是协议实现上的冗余/重复消息。
                // 这里选择静默忽略，避免把低价值异常升级成整条连接失败。
            }
            Err(error) => {
                // reader 终止时要失败所有 pending 请求，避免调用方无限等待。
                metrics.inner.errors.fetch_add(1, Ordering::Relaxed);
                fail_pending_requests(&pending, format!("bridge reader terminated: {error}"));
                return;
            }
        }
    }
}

fn handle_host_request(
    payload: HostRequest,
    handler: Option<&dyn crate::HostRpcHandler>,
) -> crate::protocol::HostResponse {
    match handler {
        Some(handler) => match handler.handle(&payload.method, payload.params) {
            Ok((result, events)) => crate::protocol::HostResponse {
                id: payload.id,
                result: Some(result),
                error: None,
                events,
            },
            Err(error) => crate::protocol::HostResponse {
                id: payload.id,
                result: None,
                error: Some(BridgeError {
                    code: "E_HOST_RPC".into(),
                    message: error.to_string(),
                    data: None,
                }),
                events: Vec::new(),
            },
        },
        None => crate::protocol::HostResponse {
            id: payload.id,
            result: None,
            error: Some(BridgeError {
                code: "E_HOST_RPC_UNAVAILABLE".into(),
                message: format!(
                    "host rpc handler is not installed for method {}",
                    payload.method
                ),
                data: None,
            }),
            events: Vec::new(),
        },
    }
}

pub(super) fn pump_process_stderr(
    mut stderr: BufReader<std::process::ChildStderr>,
    pending: Arc<Mutex<std::collections::HashMap<String, PendingRequest>>>,
    global_events: broadcast::Sender<BridgeEvent>,
) {
    let mut buffer = String::new();
    loop {
        buffer.clear();
        match stderr.read_line(&mut buffer) {
            Ok(0) => return,
            Ok(_) => {
                let line = buffer.trim_end_matches(['\n', '\r']).trim();
                if line.is_empty() {
                    continue;
                }

                // 把 stderr 行“降级”为 event.log：
                // - 便于在 bridge 崩溃/抛错时仍能看到有效信息
                // - 但这不是结构化日志，level 只能用简单规则猜测
                let level = if line.contains("ERROR") || line.starts_with("error") {
                    "error"
                } else if line.contains("WARN") || line.starts_with("warn") {
                    "warn"
                } else {
                    "info"
                };

                let event = BridgeEvent {
                    method: BridgeEventMethod::Log,
                    params: serde_json::json!({
                        "level": level,
                        "message": line,
                        "source": "stderr"
                    }),
                };

                let _ = global_events.send(event.clone());
                // stderr 不知道具体归属哪个 request，因此广播给所有 in-flight 请求。
                // 这是一个“宁可多给，也不要漏掉”的取舍：
                // - 可能有些请求会看到与自己无关的 stderr 行
                // - 但至少真正相关的请求不会因为缺少 request_id 而完全拿不到诊断信息
                // 注意这里先把 sender clone 到一个临时 `Vec`，再逐个发送，
                // 而不是一直拿着 Mutex 直接循环 `try_send`。
                // 原因和别处一样：缩小加锁范围，避免“持锁做 IO/发送”导致阻塞扩大。
                let pending_snapshot = pending
                    .lock()
                    .expect("bridge pending store poisoned")
                    .values()
                    .map(|request| request.event_tx.clone())
                    .collect::<Vec<_>>();
                for sender in pending_snapshot {
                    let _ = sender.try_send(event.clone());
                }
            }
            Err(_) => return,
        }
    }
}

pub(super) fn update_max_pending(target: &AtomicUsize, candidate: usize) {
    // 这里展示了原子变量的一个经典用途：无锁维护“历史最大值”。
    // `compare_exchange` 的意思可以粗略理解为：
    // “如果当前值还是我刚刚读到的那个，就把它改成 candidate；否则说明别人刚改过，重试。”
    let mut current = target.load(Ordering::Relaxed);
    while candidate > current {
        match target.compare_exchange(current, candidate, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

pub(super) fn fail_pending_requests(
    pending: &Arc<Mutex<std::collections::HashMap<String, PendingRequest>>>,
    message: String,
) {
    // `drain()` 会一次性“取出并清空”整个 HashMap。
    // 这里的语义很明确：bridge 已经不可用了，所有 in-flight 请求都必须被一次性失败掉。
    // 清空动作和失败通知放在一起，能避免后续还有别的线程误以为这些请求仍在 pending。
    let requests = pending
        .lock()
        .expect("bridge pending store poisoned")
        .drain()
        .collect::<Vec<_>>();
    for (request_id, request) in requests {
        let _ = request.response_tx.send(BridgeResponse {
            id: request_id,
            result: None,
            error: Some(BridgeError {
                code: "E_BRIDGE_CLOSED".into(),
                message: message.clone(),
                data: None,
            }),
        });
    }
}

pub(super) fn bridge_launch_spec() -> Result<BridgeLaunchSpec> {
    let package_dir = bridge_package_dir()?;
    bridge_launch_spec_for_dir(&package_dir)
}

pub(super) fn bridge_launch_spec_for_dir(package_dir: &Path) -> Result<BridgeLaunchSpec> {
    let source_entry = package_dir.join("src/entry/stdio.ts");
    let workspace_templates_entry = package_dir.join("../templates/src/index.ts");
    let tsx_import = resolve_tsx_import(package_dir);
    if source_entry.exists() && workspace_templates_entry.exists() {
        // workspace 开发模式优先直接跑 ts 源码，避免源码已更新但 dist 仍然陈旧时出现行为漂移。
        return Ok(BridgeLaunchSpec {
            package_dir: package_dir.to_path_buf(),
            program: PathBuf::from(env::var("LANIA_NODE_BIN").unwrap_or_else(|_| "node".into())),
            args: vec![
                "--import".into(),
                tsx_import.clone(),
                source_entry.display().to_string(),
            ],
            mode: "dev_source",
        });
    }

    if source_entry.exists() {
        // 在源码 checkout 场景下，即使 templates workspace 不完整，也优先用源码入口；
        // 这样 host/runtime/e2e 能与当前本地改动保持一致。
        return Ok(BridgeLaunchSpec {
            package_dir: package_dir.to_path_buf(),
            program: PathBuf::from(env::var("LANIA_NODE_BIN").unwrap_or_else(|_| "node".into())),
            args: vec![
                "--import".into(),
                tsx_import.clone(),
                source_entry.display().to_string(),
            ],
            mode: "dev_source",
        });
    }

    let dist_entry = package_dir.join("dist/entry/stdio.js");
    if dist_entry.exists() {
        // release 模式：优先使用已构建产物（启动更快，也不依赖 tsx/ts 源码）。
        return Ok(BridgeLaunchSpec {
            package_dir: package_dir.to_path_buf(),
            program: PathBuf::from(env::var("LANIA_NODE_BIN").unwrap_or_else(|_| "node".into())),
            // 注意：
            // templates 包当前仍然把运行时模块（questions/dependencies/config）
            // 以 `.ts` 资源的形式发布到 `dist/templates/**` 下。
            // 如果没有 TS loader，Node 在执行 `template.*` 请求时会报：
            // `Unknown file extension ".ts"`。
            //
            // 因此这里使用 `--import tsx`，保证 installed_dist 模式在：
            // - workspace 开发场景
            // - 打包后的安装场景
            // 都能正常工作，直到模板资源被完全转译成 `.js` 为止。
            args: vec![
                "--import".into(),
                tsx_import,
                dist_entry.display().to_string(),
            ],
            mode: "installed_dist",
        });
    }

    Err(anyhow!(
        "node bridge assets not found in {} (expected dist/entry/stdio.js for release mode or src/entry/stdio.ts for development mode). Install the built bridge assets or the `tsx` runtime dependency with the bridge package, or set LANIA_NODE_BRIDGE_DIR to an installed bridge package directory if needed.",
        package_dir.display()
    ))
}

fn resolve_tsx_import(package_dir: &Path) -> String {
    let bundled_loader = package_dir.join("node_modules/tsx/dist/loader.mjs");
    if bundled_loader.exists() {
        return bundled_loader.display().to_string();
    }
    "tsx".into()
}

fn bridge_package_dir_unlocked() -> Result<PathBuf> {
    if let Ok(explicit) = env::var("LANIA_NODE_BRIDGE_DIR") {
        let path = PathBuf::from(explicit);
        if path.exists() {
            return Ok(path);
        }
        return Err(anyhow!(
            "LANIA_NODE_BRIDGE_DIR points to a missing directory: {}",
            path.display()
        ));
    }

    if let Ok(current_exe) = env::current_exe() {
        // 安装产物模式：从当前可执行文件位置推断 node-bridge 的资源目录。
        let installed_candidates = installed_bridge_package_dir_candidates(&current_exe);
        if let Some(path) = installed_candidates
            .into_iter()
            .find(|candidate| candidate.exists())
        {
            return Ok(path);
        }
    }

    let dev_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("ts/packages/node-bridge");
    if dev_dir.exists() {
        return Ok(dev_dir);
    }

    Err(anyhow!(
        "unable to locate node bridge package directory from current executable or source workspace"
    ))
}

pub(super) fn bridge_package_dir() -> Result<PathBuf> {
    let _guard = BRIDGE_ENV_MUTEX
        .lock()
        .expect("bridge env mutex should not be poisoned");
    bridge_package_dir_unlocked()
}

#[cfg(test)]
pub(super) fn with_bridge_env_lock<T>(callback: impl FnOnce() -> T) -> T {
    let _guard = BRIDGE_ENV_MUTEX
        .lock()
        .expect("bridge env mutex should not be poisoned");
    callback()
}

#[cfg(test)]
pub(super) fn bridge_package_dir_for_test() -> Result<PathBuf> {
    bridge_package_dir_unlocked()
}

pub(super) fn installed_bridge_package_dir_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(bin_dir) = current_exe.parent() {
        candidates.push(bin_dir.join("../lib/node-bridge"));
        candidates.push(bin_dir.join("../node-bridge"));
        candidates.push(bin_dir.join("../Resources/node-bridge"));
    }
    candidates
}

pub(super) fn write_envelope(stdin: &mut ChildStdin, value: &serde_json::Value) -> Result<()> {
    writeln!(stdin, "{value}")?;
    stdin.flush()?;
    Ok(())
}

pub(super) fn read_envelope(stdout: &mut BufReader<ChildStdout>) -> Result<BridgeEnvelope> {
    let mut line = String::new();
    stdout.read_line(&mut line)?;
    if line.trim().is_empty() {
        return Err(anyhow!("node bridge closed without a payload"));
    }

    // 协议约定：一行一个 JSON envelope（jsonl）。
    // 解析失败通常意味着 stdout 被污染（例如用户代码 println）、或 bridge 版本不匹配。
    let value: serde_json::Value =
        serde_json::from_str(&line).context("failed to parse node bridge envelope")?;
    match value["type"].as_str() {
        Some("handshake") => Ok(BridgeEnvelope::Handshake {
            payload: serde_json::from_value(value["payload"].clone())
                .context("failed to parse handshake payload")?,
        }),
        Some("event") => Ok(BridgeEnvelope::Event {
            request_id: value["requestId"].as_str().unwrap_or_default().to_string(),
            payload: serde_json::from_value(value["payload"].clone())
                .context("failed to parse event payload")?,
        }),
        Some("response") => Ok(BridgeEnvelope::Response {
            payload: serde_json::from_value(value["payload"].clone())
                .context("failed to parse response payload")?,
        }),
        Some("host_request") => Ok(BridgeEnvelope::HostRequest {
            payload: serde_json::from_value(value["payload"].clone())
                .context("failed to parse host request payload")?,
        }),
        other => Err(anyhow!("unsupported bridge envelope type: {:?}", other)),
    }
}
