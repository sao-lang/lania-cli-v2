//! NodeBridgeClient 的“发起调用”这一层。
//!
//! 它主要负责：
//! - 把 request 登记到 pending 表
//! - 决定是走真实 process transport，还是退回 mock
//! - 为调用方暴露两种使用方式：
//!   - `call/call_async`：一次性收完整 exchange
//!   - `open_call`：先拿到活跃句柄，再流式消费 event
//!
//! 可以把这里理解成“面向上层业务的 bridge API 表面层”。

use std::{sync::atomic::Ordering, time::Duration};

use anyhow::{anyhow, Result};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::super::process::{fail_pending_requests, update_max_pending, write_envelope};
use super::super::*;

impl NodeBridgeClient {
    pub fn call(&self, request: BridgeRequest) -> BridgeExchange {
        let fallback_request = request.clone();
        // 同步 API 主要给非 async 代码路径使用；如果内部 async 失败则 fallback 到 mock。
        // 注意：这里吞掉错误是“体验优先”的策略，适用于开发期；关键路径应使用 call_async。
        self.block_on_result(self.call_async(request))
            .unwrap_or_else(|_| self.mock_call(fallback_request))
    }

    pub async fn call_async(&self, request: BridgeRequest) -> Result<BridgeExchange> {
        if self.config.prefer_process_transport {
            match self.open_call(request.clone()) {
                Ok(call) => return call.collect_exchange().await,
                Err(_error) => {
                    // 这里静默继续走 mock，而不是立刻把错误抛给上层。
                    // 这和 `call()` 的设计一致：`call_async()` 在当前项目里也偏“体验优先”入口，
                    // 目的是尽量给上层一个结构稳定的 exchange。
                }
            }
        }

        // process transport 不可用时退回 mock：
        // - 便于在没有 node 环境/桥接资源缺失时仍可运行核心链路
        // - 同时保持接口一致（依旧返回 exchange）
        Ok(self.mock_call(request))
    }

    pub fn open_call(&self, request: BridgeRequest) -> Result<BridgeActiveCall> {
        let mut state = self.state.lock().expect("bridge state poisoned");
        // 这里虽然只是在“发一个请求”，但第一步不是直接写 stdin，
        // 而是先确保进程存在且健康。
        // 这样上层的调用方就不需要自己关心“bridge 有没有启动 / 要不要重连”。
        self.ensure_process_locked(&mut state)?;
        let pending_count = state
            .process
            .as_ref()
            .expect("bridge process initialized")
            .pending
            .lock()
            .expect("bridge pending store poisoned")
            .len();
        // 背压判断发生在“真正插入 pending 之前”：
        // 这样上层拿到错误时，系统内部状态仍然是干净的，
        // 不会出现“请求其实没发出去，但 pending 里已经多了一条半残记录”。
        if pending_count >= self.config.max_pending_requests {
            // 背压：限制并发 in-flight 请求数，避免 bridge 事件积压导致内存增长。
            state.metrics.errors.fetch_add(1, Ordering::Relaxed);
            return Err(anyhow!(
                "bridge backpressure limit reached: {} pending requests",
                self.config.max_pending_requests
            ));
        }
        state.metrics.requests_sent.fetch_add(1, Ordering::Relaxed);
        update_max_pending(
            &state.metrics.max_pending_requests_seen,
            pending_count.saturating_add(1),
        );
        let process = state.process.as_mut().expect("bridge process initialized");
        // `mpsc` 适合事件流，因为一条请求可能收到多条 event；
        // `oneshot` 适合最终响应，因为 response 语义上只能到达一次。
        let (event_tx, event_rx) = mpsc::channel(self.config.event_buffer_capacity.max(1));
        let (response_tx, response_rx) = oneshot::channel();
        // 先登记 pending，再发送请求：确保 reader 线程先收到 event/response 时也能路由成功。
        process
            .pending
            .lock()
            .expect("bridge pending store poisoned")
            .insert(
                request.id.clone(),
                PendingRequest {
                    event_tx,
                    response_tx,
                },
            );

        let write_result = {
            let mut stdin = process.stdin.lock().expect("bridge stdin poisoned");
            write_envelope(
                &mut stdin,
                &serde_json::json!({
                    "type": "request",
                    "payload": request,
                }),
            )
        };
        if let Err(error) = write_result {
            // 发送失败时必须清理 pending，否则调用方会永远等不到 response。
            // 注意这里 request 已经 move 进 envelope，因此下面只能靠 `request.id`
            // 这个之前还保留着的字段来回收挂起项。
            process
                .pending
                .lock()
                .expect("bridge pending store poisoned")
                .remove(&request.id);
            return Err(error);
        }

        Ok(BridgeActiveCall {
            response_rx,
            event_rx,
        })
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BridgeEvent> {
        // `broadcast` 的 receiver 是“各拿各的一份游标”，互不影响。
        // 某个订阅者处理慢，只会丢自己的消息，不会拖住整个系统。
        self.state
            .lock()
            .expect("bridge state poisoned")
            .global_events
            .subscribe()
    }

    pub fn metrics_snapshot(&self) -> BridgeMetricsSnapshot {
        let state = self.state.lock().expect("bridge state poisoned");
        BridgeMetricsSnapshot {
            requests_sent: state.metrics.requests_sent.load(Ordering::Relaxed),
            responses_received: state.metrics.responses_received.load(Ordering::Relaxed),
            events_received: state.metrics.events_received.load(Ordering::Relaxed),
            reconnects: state.metrics.reconnects.load(Ordering::Relaxed),
            heartbeat_events: state.metrics.heartbeat_events.load(Ordering::Relaxed),
            timeouts: state.metrics.timeouts.load(Ordering::Relaxed),
            errors: state.metrics.errors.load(Ordering::Relaxed),
            max_pending_requests_seen: state
                .metrics
                .max_pending_requests_seen
                .load(Ordering::Relaxed),
        }
    }

    pub fn shutdown(&self) {
        let _ = self.block_on_result(self.shutdown_async());
    }

    pub async fn shutdown_async(&self) -> Result<()> {
        if self.config.prefer_process_transport {
            // 尽力而为（best-effort）：不要因为 shutdown 请求失败而阻塞整体退出流程。
            // 退出阶段最重要的是“尽快清场”，而不是保证每个 shutdown RPC 都成功返回。
            let _ = self.call_async(self.shutdown_request()).await;
        }

        if let Ok(mut state) = self.state.lock() {
            if let Some(mut process) = state.process.take() {
                // 先失败所有 pending：避免调用方卡在 oneshot await。
                // `take()` 的效果是把 `Option<BridgeProcess>` 变成 `None`，
                // 同时把原来的进程对象移出来单独处理，避免 shutdown 过程中仍被别的调用方看到。
                fail_pending_requests(&process.pending, "bridge shutdown requested".into());
                let _ = process.child.kill();
                let _ = process.child.wait();
                process.reader.take();
                process.stderr_reader.take();
            }
        }
        Ok(())
    }

    pub fn using_process_transport(&self) -> bool {
        self.state
            .lock()
            .expect("bridge state poisoned")
            .process
            .is_some()
    }

    pub fn supported_events(&self) -> Vec<BridgeEventMethod> {
        if let Some(events) = self.process_supported_events() {
            // 一旦真实 handshake 已经告诉我们“bridge 支持哪些事件”，
            // 就优先相信运行时事实，而不是继续用静态兜底表。
            return events;
        }

        // 这份静态列表主要服务于：
        // - 进程还没真正拉起时的 preview/summary
        // - mock / 降级路径下的能力展示
        vec![
            BridgeEventMethod::Ready,
            BridgeEventMethod::Log,
            BridgeEventMethod::Progress,
            BridgeEventMethod::DevUrl,
            BridgeEventMethod::BuildAsset,
            BridgeEventMethod::LintStart,
            BridgeEventMethod::LintFile,
            BridgeEventMethod::LintResult,
            BridgeEventMethod::LintSummary,
            BridgeEventMethod::WatchChange,
            BridgeEventMethod::Shutdown,
            BridgeEventMethod::Heartbeat,
        ]
    }

    pub fn timeout(&self) -> Duration {
        self.config.timeout
    }
}
