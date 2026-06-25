//! Node bridge 的真实进程传输层。
//!
//! 相比 mock transport，这里处理的是真正的 Node 子进程：
//! - 拉起 bridge
//! - 完成 handshake
//! - 监控 heartbeat
//! - 根据 failure strategy 决定是否重连
//!
//! 这个文件最值得关注的是“进程活性判断”与“重连策略”。

use std::{
    env,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::{atomic::Ordering, Arc, Mutex},
    thread,
    time::Instant,
};

use anyhow::{anyhow, Context, Result};

use crate::protocol::{BridgeEventMethod, BridgeFailureStrategy};

use super::super::process::{
    bridge_launch_spec, pump_process_events, pump_process_stderr, read_envelope, write_envelope,
};
use super::super::{
    BridgeEnvelope, BridgeMetricsHandles, BridgeProcess, BridgeState, NodeBridgeClient,
    PendingRequest,
};

impl NodeBridgeClient {
    pub(super) fn ensure_process_locked(&self, state: &mut BridgeState) -> Result<()> {
        // 这个函数是 process transport 的“守门员”：
        // 每次要发请求前，都先检查 bridge 子进程是否存在、是否已退出、是否心跳超时。
        let needs_respawn = match state.process.as_mut() {
            // try_wait()：非阻塞检查子进程是否已经退出。
            Some(process) => process.child.try_wait().ok().flatten().is_some(),
            None => true,
        };
        if needs_respawn {
            // 进程已退出或尚未创建：重新拉起并重新 handshake。
            state.process = Some(self.spawn_process(state)?);
            state.metrics.reconnects.fetch_add(1, Ordering::Relaxed);
        } else {
            let process = state.process.as_ref().expect("bridge process initialized");
            let last_heartbeat = *process
                .last_heartbeat
                .lock()
                .expect("bridge heartbeat poisoned");
            if process.handshake.failure_strategy == BridgeFailureStrategy::Reconnect
                && last_heartbeat.elapsed() > self.config.heartbeat_timeout
            {
                // 按策略允许重连时，若心跳超时则认为 bridge 假死，触发重启。
                // 注意：这是“保守策略”：
                // - 宁可重启，也不要让调用方无限等待（尤其是 dev/watch 场景）
                // - 真正的幂等/恢复由上层（host/execution）根据 method 决定是否重试
                state.process = Some(self.spawn_process(state)?);
                state.metrics.reconnects.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    pub(super) fn spawn_process(&self, state: &mut BridgeState) -> Result<BridgeProcess> {
        let spec = bridge_launch_spec()?;
        // 这里启动的是一个真正的 Node 子进程，而不是库内线程。
        // 这样 Rust 和 Node 的职责边界非常清晰：
        // - Rust 不直接解释 JS/TS
        // - Node bridge 作为独立进程，负责加载 JS 生态能力
        let mut child = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.package_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn node bridge in {} using {} ({})",
                    spec.package_dir.display(),
                    spec.program.display(),
                    spec.mode
                )
            })?;
        let mut stdin = child.stdin.take().context("bridge stdin missing")?;
        let stdout = child.stdout.take().context("bridge stdout missing")?;
        let stderr = child.stderr.take().context("bridge stderr missing")?;
        let mut stdout = BufReader::new(stdout);

        // handshake 是“协议级初始化”：
        // - 协商 protocolVersion / events / failureStrategy 等能力
        // - 同时也作为“bridge 已启动可读写”的确认点
        write_envelope(
            &mut stdin,
            &serde_json::json!({
                "type": "handshake",
                "payload": self.handshake_request(),
            }),
        )?;

        let handshake = loop {
            match read_envelope(&mut stdout)? {
                BridgeEnvelope::Handshake { payload } => break payload,
                BridgeEnvelope::Event { payload, .. } => {
                    // handshake 之前也可能发出全局事件（例如 ready/heartbeat），这里先转发出去。
                    // 注意这里不会尝试路由到某个 request：
                    // 现在还处于“进程刚启动、尚无业务请求 in-flight”的阶段。
                    let _ = state.global_events.send(payload);
                    continue;
                }
                BridgeEnvelope::Response { .. } => {
                    // 在 handshake 阶段不应该收到 response，但为兼容实现差异，直接忽略。
                    continue;
                }
                BridgeEnvelope::HostRequest { .. } => {
                    // handshake 阶段不处理 host rpc（此时 host handler/stdio 通道可能尚未就绪）。
                    continue;
                }
            }
        };
        // pending 保存“每个 requestId 对应的 event channel + response oneshot”。
        // reader 线程会按 envelope.requestId 路由 event，并在 response 到达时完成 oneshot。
        let pending = Arc::new(Mutex::new(std::collections::HashMap::<
            String,
            PendingRequest,
        >::new()));
        let stdin = Arc::new(Mutex::new(stdin));
        // 这里 `pending` 的生命周期严格绑定到当前这次 bridge 进程实例：
        // - 一旦进程重启，就会创建一份新的 pending map
        // - 旧进程上尚未完成的请求也会随 reader 退出而被视为失败
        //
        // 这样做虽然“重启后不会自动恢复旧请求”，但状态边界很清晰，不会把两代进程的消息串台。
        let reader_pending = Arc::clone(&pending);
        let reader_events = state.global_events.clone();
        let last_heartbeat = Arc::new(Mutex::new(Instant::now()));
        let reader_heartbeat = Arc::clone(&last_heartbeat);
        let metrics = BridgeMetricsHandles::new(&state.metrics);
        let reader_stdin = Arc::clone(&stdin);
        let host_rpc_handler = state.host_rpc_handler.clone();
        // reader 线程负责持续读取 stdout：
        // - 解析 envelope
        // - 广播 global events
        // - 路由 per-request events/response
        let reader = thread::spawn(move || {
            pump_process_events(
                stdout,
                reader_stdin,
                reader_pending,
                reader_events,
                reader_heartbeat,
                metrics,
                host_rpc_handler,
            );
        });

        let passthrough_stderr = env::var("LANIA_BRIDGE_PASSTHROUGH_STDERR")
            .ok()
            .map(|value| value == "1")
            .unwrap_or(false);
        let stderr_reader = {
            // 即使不透传 stderr，也要持续消费这条管道。
            // 否则某些依赖往 stderr 输出时，bridge 进程可能因为写到已关闭/无人消费的 pipe 而异常退出。
            let stderr_pending = Arc::clone(&pending);
            let stderr_events = state.global_events.clone();
            Some(thread::spawn(move || {
                if passthrough_stderr {
                    pump_process_stderr(BufReader::new(stderr), stderr_pending, stderr_events);
                } else {
                    drain_process_stderr(BufReader::new(stderr));
                }
            }))
        };

        Ok(BridgeProcess {
            child,
            stdin,
            handshake,
            pending,
            last_heartbeat,
            reader: Some(reader),
            stderr_reader,
        })
    }

    pub(super) fn block_on_result<T>(
        &self,
        future: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        match tokio::runtime::Handle::try_current() {
            // 已经在 Tokio runtime 里时，不能再粗暴创建/嵌套新的 runtime。
            // 这里用 `block_in_place + handle.block_on(...)`，把“同步等异步结果”这件事
            // 安全地安放到当前 runtime 语境里。
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
            // 如果当前根本不在 runtime 中，就临时创建一个单线程 runtime 来执行 future。
            // 这通常发生在一些同步 facade / 测试工具入口。
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| anyhow!("failed to create bridge runtime: {error}"))?
                .block_on(future),
        }
    }

    pub(super) fn process_supported_events(&self) -> Option<Vec<BridgeEventMethod>> {
        self.state
            .lock()
            .expect("bridge state poisoned")
            .process
            .as_ref()
            .map(|process| process.handshake.events.clone())
    }
}

fn drain_process_stderr(mut stderr: BufReader<std::process::ChildStderr>) {
    let mut buffer = String::new();
    loop {
        buffer.clear();
        match stderr.read_line(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}
