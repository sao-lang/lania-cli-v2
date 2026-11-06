//! 任务执行辅助函数：一次 attempt 如何跑、失败如何重试、结果如何落到 `TaskService`。
//!
//! `executor.rs` 更像调度器，决定“什么时候跑哪个任务”；
//! 而这个文件负责“单个任务真正执行时的公共套路”：
//! - 调 runner future
//! - 同时监听超时与取消
//! - 根据重试策略决定是否再来一轮
//! - 把结果转换成统一的 `TaskExecutionResult`
//!
//! 也就是说，这里描述的是任务状态机的局部片段，而不是全局并发调度。

use std::{
    sync::Arc,
};

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::service::{TaskService, TaskStateStore};
use crate::types::{
    TaskDefinition, TaskEvent, TaskEventKind, TaskExecutionResult, TaskFailure, TaskOutcome,
    TaskRecord, TaskSink, TaskState,
};

#[derive(Debug)]
pub(crate) enum TaskAttemptError {
    Failed(anyhow::Error),
    Cancelled(String),
}

pub(crate) async fn run_attempt(
    task: &TaskDefinition,
    cancellation: CancellationToken,
) -> Result<Value, TaskAttemptError> {
    // 这里先把 runner future 构造出来，再交给 `select!` 同时等待：
    // - 任务自己正常结束
    // - 超时
    // - 外部取消
    //
    // `CancellationToken` 之所以 clone 一份传给 runner，是因为：
    // - 外层调度器需要保留 token，随时触发 cancel
    // - 任务内部也可能想主动轮询 `is_cancelled()` 或等待 `cancelled().await`
    // - token clone 的成本很低，本质上共享的是同一份取消状态
    let future = (task.runner)(cancellation.clone());
    match task.timeout {
        Some(timeout) => {
            tokio::select! {
                // runner 自己先结束时，再进一步区分：
                // - 真成功
                // - 真失败
                // - 还是因为取消导致的失败（例如任务内部感知取消后返回 Err）
                result = future => match result {
                    Ok(detail) => Ok(detail),
                    Err(_error) if cancellation.is_cancelled() => Err(TaskAttemptError::Cancelled("task cancelled".into())),
                    Err(error) => Err(TaskAttemptError::Failed(error)),
                },
                // 注意：超时被归类为 Failed，而不是 Cancelled。
                // 这是因为“超时”通常意味着任务没有在 SLA 内完成，语义上更接近执行失败。
                _ = tokio::time::sleep(timeout) => Err(TaskAttemptError::Failed(anyhow!("task timed out after {}ms", timeout.as_millis()))),
                _ = cancellation.cancelled() => Err(TaskAttemptError::Cancelled("task cancelled".into())),
            }
        }
        None => {
            tokio::select! {
                result = future => match result {
                    Ok(detail) => Ok(detail),
                    Err(_error) if cancellation.is_cancelled() => Err(TaskAttemptError::Cancelled("task cancelled".into())),
                    Err(error) => Err(TaskAttemptError::Failed(error)),
                },
                _ = cancellation.cancelled() => Err(TaskAttemptError::Cancelled("task cancelled".into())),
            }
        }
    }
}

pub(crate) fn value_to_detail(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub(crate) async fn execute_task_no_semaphore(
    service: TaskService,
    task: TaskDefinition,
    cancellation: CancellationToken,
) -> TaskExecutionResult {
    service.start(task.id.clone(), task.title.clone());

    let mut attempts = 0;
    loop {
        attempts += 1;
        service.update(&task.id, format!("attempt {attempts}"));

        let attempt_result = run_attempt(&task, cancellation.clone()).await;
        match attempt_result {
            Ok(data) => {
                let detail = value_to_detail(&data);
                service.complete(&task.id, detail.clone());
                return TaskExecutionResult {
                    rollback: task
                        .rollback
                        .clone()
                        .map(|rollback| (task.id.clone(), rollback)),
                    failure: None,
                    outcome: TaskOutcome {
                        id: task.id,
                        state: TaskState::Completed,
                        detail: Some(detail),
                        data: Some(data),
                        attempts,
                        group: task.group,
                        priority: task.priority,
                    },
                };
            }
            Err(TaskAttemptError::Cancelled(message)) => {
                service.cancel(&task.id, message.clone());
                return TaskExecutionResult {
                    rollback: None,
                    failure: None,
                    outcome: TaskOutcome {
                        id: task.id,
                        state: TaskState::Cancelled,
                        detail: Some(message),
                        data: None,
                        attempts,
                        group: task.group,
                        priority: task.priority,
                    },
                };
            }
            Err(TaskAttemptError::Failed(error)) if attempts <= task.max_retries => {
                // 这里的判断是 `<= max_retries`，因为 `attempts` 统计的是“已经执行了几次”，
                // 而 `max_retries` 表示“失败后最多还能再补跑几轮”。
                // 例如 `max_retries = 2` 时，最坏情况一共会尝试 3 次。
                service.update(&task.id, format!("retrying after error: {error}"));
                if !task.retry_delay.is_zero() {
                    tokio::select! {
                        _ = tokio::time::sleep(task.retry_delay) => {}
                        _ = cancellation.cancelled() => {
                            // retry delay 期间也要响应取消，否则用户会看到：
                            // “任务明明已经取消了，却还在傻等 backoff 睡眠结束”。
                            let message = "task cancelled during retry delay".to_string();
                            service.cancel(&task.id, message.clone());
                            return TaskExecutionResult {
                                rollback: None,
                                failure: None,
                                outcome: TaskOutcome {
                                    id: task.id,
                                    state: TaskState::Cancelled,
                                    detail: Some(message),
                                    data: None,
                                    attempts,
                                    group: task.group,
                                    priority: task.priority,
                                },
                            };
                        }
                    }
                }
            }
            Err(TaskAttemptError::Failed(error)) => {
                service.fail(&task.id, error.to_string());
                return TaskExecutionResult {
                    rollback: None,
                    failure: Some(TaskFailure {
                        id: task.id.clone(),
                        message: error.to_string(),
                        attempts,
                    }),
                    outcome: TaskOutcome {
                        id: task.id,
                        state: TaskState::Failed,
                        detail: Some(error.to_string()),
                        data: None,
                        attempts,
                        group: task.group,
                        priority: task.priority,
                    },
                };
            }
        }
    }
}

pub(crate) fn set_task_state(
    state: &mut TaskStateStore,
    id: &str,
    next_state: impl Into<Option<TaskState>>,
    detail: Option<String>,
    attempts: Option<u32>,
) {
    if let Some(task) = state.tasks.iter_mut().find(|task| task.id == id) {
        if let Some(next_state) = next_state.into() {
            task.state = next_state;
        }
        if let Some(detail) = detail {
            task.detail = Some(detail);
        }
        if let Some(attempts) = attempts {
            task.attempts = attempts;
        }
    }
}

pub(crate) fn emit_event(
    state: &mut TaskStateStore,
    task_id: String,
    kind: TaskEventKind,
    detail: Option<String>,
) -> TaskEvent {
    state.next_sequence += 1;
    let event = TaskEvent {
        sequence: state.next_sequence,
        task_id,
        kind,
        detail,
    };
    state.events.push(event.clone());
    event
}

pub(crate) fn notify_sinks(sinks: &[Arc<dyn TaskSink>], record: &TaskRecord, event: &TaskEvent) {
    for sink in sinks {
        sink.on_event(record, event);
    }
}
