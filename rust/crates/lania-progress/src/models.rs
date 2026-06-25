//! 进度系统的数据模型与适配器类型。
//!
//! 这里定义了“进度长什么样”：
//! - `ProgressSnapshot`：某个进度项此刻的完整状态
//! - `ProgressEvent`：一次状态变化事件
//! - `ProgressSummary`：面向输出层的总体汇总
//! - `ProgressSink` / `TaskProgressSink`：如何把事件送到不同消费者
//!
//! 如果你想理解 `ProgressService` 到底保存了什么、sink 又拿到了什么，这个文件是入口。

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use lania_task::{TaskEvent, TaskEventKind, TaskRecord, TaskSink};
use serde::{Deserialize, Serialize};

use crate::ProgressService;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressLevel {
    Group,
    Step,
    Item,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    Spinner,
    ProgressBar,
    StaticStep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressEventKind {
    Began,
    Advanced,
    TotalUpdated,
    Message,
    Detail,
    Finished,
    Failed,
    Cancelled,
    Reset,
    LinkedTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub sequence: u64,
    pub progress_id: String,
    pub kind: ProgressEventKind,
    pub current: u64,
    pub total: Option<u64>,
    pub message: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressSnapshot {
    pub id: String,
    pub parent_id: Option<String>,
    pub level: ProgressLevel,
    pub kind: ProgressKind,
    pub status: ProgressStatus,
    pub task_id: Option<String>,
    pub current: u64,
    pub total: Option<u64>,
    pub message: Option<String>,
    pub detail: Option<String>,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
}

impl ProgressSnapshot {
    pub fn percent(&self) -> Option<u64> {
        let total = self.total?;
        if total == 0 {
            return Some(100);
        }
        Some(self.current.saturating_mul(100) / total)
    }

    pub fn duration_ms(&self) -> Option<u64> {
        let finished = self.finished_at_ms?;
        Some(finished.saturating_sub(self.started_at_ms))
    }

    pub fn rate_per_sec(&self) -> Option<f64> {
        let duration_ms = self.duration_ms()?;
        if duration_ms == 0 {
            return None;
        }
        Some((self.current as f64 / duration_ms as f64) * 1000.0)
    }

    pub fn eta_ms(&self) -> Option<u64> {
        let total = self.total?;
        let rate = self.rate_per_sec()?;
        if rate <= 0.0 || self.current >= total {
            return Some(0);
        }
        Some((((total - self.current) as f64) / rate * 1000.0) as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressSummary {
    pub items: Vec<ProgressSnapshot>,
    pub events: Vec<ProgressEvent>,
}

pub trait ProgressSink: Send + Sync {
    fn on_event(&self, snapshot: &ProgressSnapshot, event: &ProgressEvent);
}

type ProgressCallback = dyn Fn(&ProgressSnapshot, &ProgressEvent) + Send + Sync;

#[derive(Clone)]
pub struct CallbackProgressSink {
    callback: Arc<ProgressCallback>,
}

impl std::fmt::Debug for CallbackProgressSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackProgressSink").finish()
    }
}

impl CallbackProgressSink {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(&ProgressSnapshot, &ProgressEvent) + Send + Sync + 'static,
    {
        Self {
            callback: Arc::new(callback),
        }
    }
}

impl ProgressSink for CallbackProgressSink {
    fn on_event(&self, snapshot: &ProgressSnapshot, event: &ProgressEvent) {
        (self.callback)(snapshot, event);
    }
}

#[derive(Debug, Default)]
struct TaskProgressState {
    seen_tasks: HashSet<String>,
    group_total: HashMap<String, u64>,
    group_done: HashMap<String, u64>,
    terminal_groups: HashSet<String>, // failed/cancelled -> stop updating like legacy manager
}

#[derive(Debug, Clone)]
pub struct TaskProgressSink {
    progress: ProgressService,
    kind: ProgressKind,
    state: Arc<Mutex<TaskProgressState>>,
}

impl TaskProgressSink {
    pub fn new(progress: ProgressService) -> Self {
        Self {
            progress,
            kind: ProgressKind::StaticStep,
            state: Arc::new(Mutex::new(TaskProgressState::default())),
        }
    }

    fn progress_id(record: &TaskRecord) -> String {
        format!("task.{}", record.id)
    }

    fn group_progress_id(group: &str) -> String {
        format!("task_group.{group}")
    }

    fn ensure_group_snapshot(&self, record: &TaskRecord) -> String {
        let group = record.group.as_str();
        let group_id = Self::group_progress_id(group);

        let (total, done, terminal) = {
            let state = self.state.lock().expect("task progress state poisoned");
            let total = state.group_total.get(group).copied().unwrap_or(0);
            let done = state.group_done.get(group).copied().unwrap_or(0);
            let terminal = state.terminal_groups.contains(group);
            (total, done, terminal)
        };

        if !self.progress.contains(&group_id) {
            self.progress
                .begin_group(group_id.clone(), Some(total), ProgressKind::ProgressBar);
            self.progress.message(&group_id, format!("[{group}]"));
            if done > 0 {
                self.progress.advance(&group_id, done);
            }
        } else if !terminal {
            // Keep group total up-to-date while it's still live.
            self.progress.update_total(&group_id, Some(total));
        }

        group_id
    }

    fn ensure_task_snapshot(&self, record: &TaskRecord, group_id: &str) -> String {
        let progress_id = Self::progress_id(record);
        if !self.progress.contains(&progress_id) {
            self.progress
                .begin_step(progress_id.clone(), group_id.to_string(), None, self.kind);
            self.progress.link_task(&progress_id, record.id.clone());
        }
        self.progress.message(&progress_id, record.title.clone());
        if let Some(detail) = &record.detail {
            self.progress.detail(&progress_id, detail.clone());
        }
        progress_id
    }
}

impl TaskSink for TaskProgressSink {
    fn on_event(&self, record: &TaskRecord, event: &TaskEvent) {
        // Track totals by group as tasks are registered/created.
        if matches!(event.kind, TaskEventKind::Registered) {
            let mut state = self.state.lock().expect("task progress state poisoned");
            if state.seen_tasks.insert(record.id.clone()) {
                *state.group_total.entry(record.group.clone()).or_insert(0) += 1;
            }
        }

        let group_id = self.ensure_group_snapshot(record);
        let progress_id = self.ensure_task_snapshot(record, &group_id);

        let is_terminal_group = {
            self.state
                .lock()
                .expect("task progress state poisoned")
                .terminal_groups
                .contains(record.group.as_str())
        };

        match event.kind {
            TaskEventKind::Registered | TaskEventKind::Started => {}
            TaskEventKind::Updated => {
                if let Some(detail) = event.detail.as_deref() {
                    self.progress.detail(&progress_id, detail);
                }
            }
            TaskEventKind::Completed => {
                self.progress.finish(&progress_id);
                if !is_terminal_group {
                    let (done, total) = {
                        let mut state = self.state.lock().expect("task progress state poisoned");
                        let total = state
                            .group_total
                            .get(record.group.as_str())
                            .copied()
                            .unwrap_or(0);
                        let done = state.group_done.entry(record.group.clone()).or_insert(0);
                        *done += 1;
                        (*done, total)
                    };
                    self.progress.advance(&group_id, 1);
                    if total > 0 && done >= total {
                        self.progress.finish(&group_id);
                    }
                }
            }
            TaskEventKind::Failed => {
                self.progress.fail(
                    &progress_id,
                    event.detail.as_deref().unwrap_or("task failed"),
                );
                // Legacy ProgressManager treats group as terminal once failed.
                let mut state = self.state.lock().expect("task progress state poisoned");
                if state.terminal_groups.insert(record.group.clone()) {
                    self.progress
                        .fail(&group_id, event.detail.as_deref().unwrap_or("task failed"));
                }
            }
            TaskEventKind::Cancelled => {
                self.progress.cancel(
                    &progress_id,
                    event.detail.as_deref().unwrap_or("task cancelled"),
                );
                let mut state = self.state.lock().expect("task progress state poisoned");
                if state.terminal_groups.insert(record.group.clone()) {
                    self.progress.cancel(
                        &group_id,
                        event.detail.as_deref().unwrap_or("task cancelled"),
                    );
                }
            }
            TaskEventKind::RollbackStarted => {
                self.progress.detail(&progress_id, "rollback started")
            }
            TaskEventKind::RollbackCompleted => {
                self.progress.detail(&progress_id, "rollback completed")
            }
            TaskEventKind::RollbackFailed => self.progress.detail(
                &progress_id,
                event.detail.as_deref().unwrap_or("rollback failed"),
            ),
        }
    }
}
