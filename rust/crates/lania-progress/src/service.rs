//! 进度状态服务：保存当前所有进度项，并把变化广播给不同的渲染 sink。
//!
//! 和 `TaskService` 很像，这里也是“状态中心 + 事件广播器”的模式：
//! - 业务层只需要调用 `begin/advance/finish/fail`
//! - `ProgressService` 负责保存快照、记录事件
//! - 终端渲染器、日志回调、任务适配器都作为 sink 挂上来
//!
//! 如果你想理解 CLI 里为什么同时能有进度条、日志和任务状态，这个文件是关键入口。

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    CallbackProgressSink, IndicatifTerminalProgressSink, ProgressEvent, ProgressEventKind,
    ProgressKind, ProgressLevel, ProgressRenderer, ProgressSink, ProgressSnapshot, ProgressStatus,
    ProgressSummary, TaskProgressSink, TerminalProgressMode,
};

#[derive(Default)]
struct ProgressState {
    next_sequence: u64,
    items: BTreeMap<String, ProgressSnapshot>,
    events: Vec<ProgressEvent>,
    sinks: Vec<Arc<dyn ProgressSink>>,
    terminal_sink: Option<Arc<IndicatifTerminalProgressSink>>,
}

impl std::fmt::Debug for ProgressState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressState")
            .field("next_sequence", &self.next_sequence)
            .field("items", &self.items.len())
            .field("events", &self.events.len())
            .field("sinks", &self.sinks.len())
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProgressService {
    // 进度系统和任务系统很像：本质也是“共享状态 + 多个 sink 订阅事件”。
    // 因此这里同样使用 `Arc<Mutex<_>>` 保存统一状态。
    state: Arc<Mutex<ProgressState>>,
}

impl ProgressService {
    pub fn add_sink(&self, sink: Arc<dyn ProgressSink>) {
        self.state
            .lock()
            .expect("progress store poisoned")
            .sinks
            .push(sink);
    }

    pub fn on_progress<F>(&self, callback: F)
    where
        F: Fn(&ProgressSnapshot, &ProgressEvent) + Send + Sync + 'static,
    {
        self.add_sink(Arc::new(CallbackProgressSink::new(callback)));
    }

    pub fn attach_terminal_sink(
        &self,
        mode: TerminalProgressMode,
    ) -> Arc<IndicatifTerminalProgressSink> {
        let sink = Arc::new(IndicatifTerminalProgressSink::with_mode(mode));
        let mut state = self.state.lock().expect("progress store poisoned");
        // 保存一份 `terminal_sink` 的专门引用，是因为后面需要执行
        // suspend/resume 这类“只针对终端渲染器”的管理操作。
        state.sinks.push(sink.clone());
        state.terminal_sink = Some(sink.clone());
        sink
    }

    pub fn suspend_terminal(&self) {
        let sink = self
            .state
            .lock()
            .expect("progress store poisoned")
            .terminal_sink
            .clone();
        if let Some(sink) = sink {
            sink.suspend();
        }
    }

    pub fn resume_terminal(&self) {
        let sink = self
            .state
            .lock()
            .expect("progress store poisoned")
            .terminal_sink
            .clone();
        if let Some(sink) = sink {
            sink.resume();
        }
    }

    pub fn suspend_terminal_guard(&self) -> TerminalProgressSuspendGuard<'_> {
        // 这是一个很典型的 RAII 用法：
        // - 创建 guard 时先 suspend
        // - guard drop 时自动 resume
        // 这样调用方即使中途 return / ? 提前退出，也不容易忘记恢复终端进度条。
        self.suspend_terminal();
        TerminalProgressSuspendGuard { progress: self }
    }

    pub fn sink_count(&self) -> usize {
        self.state
            .lock()
            .expect("progress store poisoned")
            .sinks
            .len()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.state
            .lock()
            .expect("progress store poisoned")
            .items
            .contains_key(id)
    }

    pub fn task_sink(&self) -> Arc<TaskProgressSink> {
        // 任务系统和进度系统并不是直接耦合的。
        // 这里通过一个适配器 `TaskProgressSink` 把 task event 翻译成 progress event，
        // 让两套系统保持解耦。
        Arc::new(TaskProgressSink::new(self.clone()))
    }

    pub fn begin(&self, id: impl Into<String>, total: Option<u64>) {
        self.begin_group(id, total, default_kind(total));
    }

    pub fn begin_group(&self, id: impl Into<String>, total: Option<u64>, kind: ProgressKind) {
        self.begin_node(id.into(), None, ProgressLevel::Group, total, kind);
    }

    pub fn begin_step(
        &self,
        id: impl Into<String>,
        parent_id: impl Into<String>,
        total: Option<u64>,
        kind: ProgressKind,
    ) {
        self.begin_node(
            id.into(),
            Some(parent_id.into()),
            ProgressLevel::Step,
            total,
            kind,
        );
    }

    pub fn begin_item(
        &self,
        id: impl Into<String>,
        parent_id: impl Into<String>,
        total: Option<u64>,
        kind: ProgressKind,
    ) {
        self.begin_node(
            id.into(),
            Some(parent_id.into()),
            ProgressLevel::Item,
            total,
            kind,
        );
    }

    pub fn advance(&self, id: &str, delta: u64) {
        self.update_with_event(id, ProgressEventKind::Advanced, |snapshot| {
            snapshot.current = snapshot.current.saturating_add(delta);
            snapshot.status = ProgressStatus::Running;
        });
    }

    pub fn update_total(&self, id: &str, total: Option<u64>) {
        self.update_with_event(id, ProgressEventKind::TotalUpdated, |snapshot| {
            snapshot.total = total;
        });
    }

    pub fn message(&self, id: &str, message: impl Into<String>) {
        let message = message.into();
        self.update_with_event(id, ProgressEventKind::Message, |snapshot| {
            snapshot.message = Some(message.clone());
            snapshot.status = ProgressStatus::Running;
        });
    }

    pub fn detail(&self, id: &str, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_with_event(id, ProgressEventKind::Detail, |snapshot| {
            snapshot.detail = Some(detail.clone());
            snapshot.status = ProgressStatus::Running;
        });
    }

    pub fn link_task(&self, id: &str, task_id: impl Into<String>) {
        let task_id = task_id.into();
        self.update_with_event(id, ProgressEventKind::LinkedTask, |snapshot| {
            snapshot.task_id = Some(task_id.clone());
        });
    }

    pub fn finish(&self, id: &str) {
        self.update_with_event(id, ProgressEventKind::Finished, |snapshot| {
            if let Some(total) = snapshot.total {
                snapshot.current = total;
            }
            snapshot.status = ProgressStatus::Completed;
            snapshot.finished_at_ms = Some(now_ms());
        });
    }

    pub fn fail(&self, id: &str, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_with_event(id, ProgressEventKind::Failed, |snapshot| {
            snapshot.detail = Some(detail.clone());
            snapshot.status = ProgressStatus::Failed;
            snapshot.finished_at_ms = Some(now_ms());
        });
    }

    pub fn cancel(&self, id: &str, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_with_event(id, ProgressEventKind::Cancelled, |snapshot| {
            snapshot.detail = Some(detail.clone());
            snapshot.status = ProgressStatus::Cancelled;
            snapshot.finished_at_ms = Some(now_ms());
        });
    }

    pub fn reset(&self, id: &str) {
        let (snapshot, sinks, event) = {
            let mut state = self.state.lock().expect("progress store poisoned");
            let mut snapshot = match state.items.remove(id) {
                Some(snapshot) => snapshot,
                None => return,
            };
            snapshot.status = ProgressStatus::Cancelled;
            snapshot.detail = Some("reset".into());
            snapshot.finished_at_ms = Some(now_ms());
            let event = push_event(&mut state, &snapshot, ProgressEventKind::Reset);
            (snapshot, state.sinks.clone(), event)
        };
        for sink in sinks {
            sink.on_event(&snapshot, &event);
        }
    }

    pub fn reset_all(&self) {
        let ids = self
            .state
            .lock()
            .expect("progress store poisoned")
            .items
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.reset(&id);
        }
    }

    pub fn complete_all(&self) {
        let ids = self
            .state
            .lock()
            .expect("progress store poisoned")
            .items
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.finish(&id);
        }
    }

    pub fn fail_all(&self, detail: impl Into<String>) {
        let detail = detail.into();
        let ids = self
            .state
            .lock()
            .expect("progress store poisoned")
            .items
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.fail(&id, detail.clone());
        }
    }

    pub fn snapshot(&self) -> Vec<ProgressSnapshot> {
        self.state
            .lock()
            .expect("progress store poisoned")
            .items
            .values()
            .cloned()
            .collect()
    }

    pub fn events(&self) -> Vec<ProgressEvent> {
        self.state
            .lock()
            .expect("progress store poisoned")
            .events
            .clone()
    }

    pub fn summary(&self) -> ProgressSummary {
        let state = self.state.lock().expect("progress store poisoned");
        ProgressSummary {
            items: state.items.values().cloned().collect(),
            events: state.events.clone(),
        }
    }

    pub fn render<R: ProgressRenderer>(&self, renderer: &R) -> Vec<String> {
        renderer.render_summary(&self.summary())
    }

    fn begin_node(
        &self,
        id: String,
        parent_id: Option<String>,
        level: ProgressLevel,
        total: Option<u64>,
        kind: ProgressKind,
    ) {
        let (snapshot, sinks, event) = {
            let mut state = self.state.lock().expect("progress store poisoned");
            // 这里采用“锁内更新状态，锁外通知 sink”的常见写法：
            // - 锁内只做内存修改，尽快结束临界区
            // - sink 回调可能打印日志、刷新终端，甚至调用别的服务，不适合在持锁状态下做
            let snapshot = ProgressSnapshot {
                id: id.clone(),
                parent_id,
                level,
                kind,
                status: ProgressStatus::Running,
                task_id: None,
                current: 0,
                total,
                message: None,
                detail: None,
                started_at_ms: now_ms(),
                finished_at_ms: None,
            };
            state.items.insert(id.clone(), snapshot.clone());
            let event = push_event(&mut state, &snapshot, ProgressEventKind::Began);
            (snapshot, state.sinks.clone(), event)
        };

        for sink in sinks {
            sink.on_event(&snapshot, &event);
        }
    }

    fn update_with_event<F>(&self, id: &str, kind: ProgressEventKind, update: F)
    where
        F: FnOnce(&mut ProgressSnapshot),
    {
        let (snapshot, sinks, event) = {
            let mut state = self.state.lock().expect("progress store poisoned");
            let snapshot = match state.items.get_mut(id) {
                Some(snapshot) => snapshot,
                None => return,
            };
            update(snapshot);
            let snapshot = snapshot.clone();
            // 这里 clone 一份快照再释放锁，是为了让 sink 看到一个稳定、完整的只读视图。
            // 如果直接把 `&mut snapshot` 借给外面，就会把内部锁生命周期拖得很长。
            let event = push_event(&mut state, &snapshot, kind);
            (snapshot, state.sinks.clone(), event)
        };

        for sink in sinks {
            sink.on_event(&snapshot, &event);
        }
    }
}

pub struct TerminalProgressSuspendGuard<'a> {
    progress: &'a ProgressService,
}

impl Drop for TerminalProgressSuspendGuard<'_> {
    fn drop(&mut self) {
        self.progress.resume_terminal();
    }
}

fn default_kind(total: Option<u64>) -> ProgressKind {
    if total.is_some() {
        ProgressKind::ProgressBar
    } else {
        ProgressKind::Spinner
    }
}

fn push_event(
    state: &mut ProgressState,
    snapshot: &ProgressSnapshot,
    kind: ProgressEventKind,
) -> ProgressEvent {
    state.next_sequence += 1;
    let event = ProgressEvent {
        sequence: state.next_sequence,
        progress_id: snapshot.id.clone(),
        kind,
        current: snapshot.current,
        total: snapshot.total,
        message: snapshot.message.clone(),
        detail: snapshot.detail.clone(),
    };
    state.events.push(event.clone());
    event
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as u64
}
