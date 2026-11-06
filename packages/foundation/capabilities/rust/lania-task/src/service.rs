//! 任务状态服务：负责保存任务列表、事件历史，并向各类 sink 广播状态变化。
//!
//! 可以把这个文件理解成“任务系统的状态中心”：
//! - `TaskExecutor` 决定任务什么时候跑、按什么顺序跑
//! - `TaskService` 负责把“任务当前处于什么状态”保存下来
//! - UI/日志/测试探针通过 `TaskSink` 订阅这些变化
//!
//! 新手阅读建议：
//! 1. 先看 `TaskStateStore`，理解内部到底保存了哪些东西
//! 2. 再看 `register/start/update/complete/fail` 这些状态变更入口
//! 3. 最后看 `run_all()`，理解它如何把状态服务和执行器接起来

use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::executor::TaskExecutor;
use crate::helpers::{emit_event, notify_sinks, set_task_state};
use crate::types::{
    TaskDefinition, TaskEvent, TaskEventKind, TaskPriority, TaskRecord, TaskRunOptions,
    TaskRunReport, TaskSink, TaskState,
};

#[derive(Default)]
pub(crate) struct TaskStateStore {
    pub(crate) next_sequence: u64,
    // `tasks` 是“当前每个任务的最新快照”。
    // 调用 `snapshot()` 时，调用方拿到的就是这里。
    pub(crate) tasks: Vec<TaskRecord>,
    // `events` 是“按时间追加的历史流”。
    // 它和 `tasks` 不是重复数据：前者回答“发生过什么”，后者回答“现在是什么状态”。
    pub(crate) events: Vec<TaskEvent>,
    pub(crate) sinks: Vec<Arc<dyn TaskSink>>,
}

impl std::fmt::Debug for TaskStateStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskStateStore")
            .field("next_sequence", &self.next_sequence)
            .field("tasks", &self.tasks.len())
            .field("events", &self.events.len())
            .field("sinks", &self.sinks.len())
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskService {
    // `TaskService` 本质上是一个“共享状态 + 事件广播中心”。
    //
    // 为什么又是 `Arc<Mutex<...>>`？
    // - 整个 CLI 里会有很多地方同时持有 `TaskService` 的 clone；
    // - clone 之后大家应该看到同一份任务状态，而不是各自维护副本；
    // - 因此外层需要 `Arc` 做共享所有权；
    // - 内层需要 `Mutex` 保护 `tasks/events/sinks` 这些可变集合。
    //
    // 仍然使用 `std::sync::Mutex`，因为这里的锁定范围只覆盖纯内存操作，
    // `notify_sinks(...)` 会在解锁之后调用，避免把锁带过潜在的慢操作。
    state: Arc<Mutex<TaskStateStore>>,
}

impl TaskService {
    pub fn add_sink(&self, sink: Arc<dyn TaskSink>) {
        // sink 是“事件订阅者”（UI/日志/测试探针等），TaskService 只负责广播，不关心具体表现形式。
        self.state
            .lock()
            .expect("task store poisoned")
            .sinks
            .push(sink);
    }

    pub fn sink_count(&self) -> usize {
        self.state.lock().expect("task store poisoned").sinks.len()
    }

    pub fn register(
        &self,
        id: impl Into<String>,
        title: impl Into<String>,
        group: impl Into<String>,
        priority: TaskPriority,
    ) {
        let id = id.into();
        let title = title.into();
        let group = group.into();
        let (record, sinks, event) = {
            let mut state = self.state.lock().expect("task store poisoned");
            if state.tasks.iter().any(|task| task.id == id) {
                // register 是幂等的：重复注册不会发 event，避免 UI 闪烁/重复输出。
                return;
            }
            state.tasks.push(TaskRecord {
                id: id.clone(),
                title,
                group,
                priority,
                state: TaskState::Pending,
                detail: None,
                attempts: 0,
            });
            let record = state
                .tasks
                .iter()
                .find(|task| task.id == id)
                .cloned()
                .expect("registered task exists");
            let event = emit_event(&mut state, id, TaskEventKind::Registered, None);
            (record, state.sinks.clone(), event)
        };
        // 这里先把 `record/sinks/event` clone 出来，再在锁外通知。
        // 这是一个常见 Rust 并发写法：
        // - 锁内只做最小必要的数据修改；
        // - 锁外再做广播/渲染/日志等可能较慢的操作；
        // - 这样能降低锁竞争，也能避免 sink 回调里再次访问 TaskService 造成死锁。
        notify_sinks(&sinks, &record, &event);
    }

    pub fn start(&self, id: impl Into<String>, title: impl Into<String>) {
        let id = id.into();
        let title = title.into();
        let (record, sinks, events) = {
            let mut state = self.state.lock().expect("task store poisoned");
            let mut emitted = Vec::new();
            if state.tasks.iter().all(|task| task.id != id) {
                // start 允许“隐式注册”：
                // 有些调用点只关心 task id + 标题，不想显式先 register。
                state.tasks.push(TaskRecord {
                    id: id.clone(),
                    title,
                    group: "default".into(),
                    priority: TaskPriority::Medium,
                    state: TaskState::Pending,
                    detail: None,
                    attempts: 0,
                });
                emitted.push(emit_event(
                    &mut state,
                    id.clone(),
                    TaskEventKind::Registered,
                    None,
                ));
            }
            set_task_state(&mut state, &id, TaskState::Running, None, None);
            emitted.push(emit_event(
                &mut state,
                id.clone(),
                TaskEventKind::Started,
                None,
            ));
            let record = state
                .tasks
                .iter()
                .find(|task| task.id == id)
                .cloned()
                .expect("started task exists");
            (record, state.sinks.clone(), emitted)
        };
        // 一个 `start` 可能产生两个事件：
        // 1. 如果任务之前不存在，先补一个 `Registered`
        // 2. 再发真正的 `Started`
        for event in events {
            notify_sinks(&sinks, &record, &event);
        }
    }

    pub fn update(&self, id: &str, detail: impl Into<String>) {
        let detail = detail.into();
        let (record, sinks, event) = {
            let mut state = self.state.lock().expect("task store poisoned");
            // `update` 只改 detail，不强制推进状态机。
            // 常见用法是 Running 期间不断刷新“当前做到哪一步了”的文字说明。
            set_task_state(&mut state, id, None, Some(detail.clone()), None);
            let record = match state.tasks.iter().find(|task| task.id == id).cloned() {
                Some(record) => record,
                None => return,
            };
            let event = emit_event(
                &mut state,
                id.to_string(),
                TaskEventKind::Updated,
                Some(detail),
            );
            (record, state.sinks.clone(), event)
        };
        notify_sinks(&sinks, &record, &event);
    }

    pub fn complete(&self, id: &str, detail: impl Into<String>) {
        let detail = detail.into();
        let (record, sinks, event) = {
            let mut state = self.state.lock().expect("task store poisoned");
            set_task_state(
                &mut state,
                id,
                TaskState::Completed,
                Some(detail.clone()),
                None,
            );
            let record = match state.tasks.iter().find(|task| task.id == id).cloned() {
                Some(record) => record,
                None => return,
            };
            let event = emit_event(
                &mut state,
                id.to_string(),
                TaskEventKind::Completed,
                Some(detail),
            );
            (record, state.sinks.clone(), event)
        };
        notify_sinks(&sinks, &record, &event);
    }

    pub fn fail(&self, id: &str, detail: impl Into<String>) {
        let detail = detail.into();
        let (record, sinks, event) = {
            let mut state = self.state.lock().expect("task store poisoned");
            set_task_state(
                &mut state,
                id,
                TaskState::Failed,
                Some(detail.clone()),
                None,
            );
            let record = match state.tasks.iter().find(|task| task.id == id).cloned() {
                Some(record) => record,
                None => return,
            };
            let event = emit_event(
                &mut state,
                id.to_string(),
                TaskEventKind::Failed,
                Some(detail),
            );
            (record, state.sinks.clone(), event)
        };
        notify_sinks(&sinks, &record, &event);
    }

    pub fn cancel(&self, id: &str, detail: impl Into<String>) {
        let detail = detail.into();
        let (record, sinks, event) = {
            let mut state = self.state.lock().expect("task store poisoned");
            set_task_state(
                &mut state,
                id,
                TaskState::Cancelled,
                Some(detail.clone()),
                None,
            );
            let record = match state.tasks.iter().find(|task| task.id == id).cloned() {
                Some(record) => record,
                None => return,
            };
            let event = emit_event(
                &mut state,
                id.to_string(),
                TaskEventKind::Cancelled,
                Some(detail),
            );
            (record, state.sinks.clone(), event)
        };
        notify_sinks(&sinks, &record, &event);
    }

    pub fn snapshot(&self) -> Vec<TaskRecord> {
        // 返回整份 clone，而不是暴露内部借用：
        // - 调用方可以在锁外自由遍历/排序/渲染
        // - TaskService 不需要把内部锁生命周期泄露给外部
        self.state
            .lock()
            .expect("task store poisoned")
            .tasks
            .clone()
    }

    pub fn events(&self) -> Vec<TaskEvent> {
        // 对测试和调试来说，`events()` 往往比 `snapshot()` 更有价值，
        // 因为它保留了时序信息，例如“先 Registered 再 Started 再 Failed”。
        self.state
            .lock()
            .expect("task store poisoned")
            .events
            .clone()
    }

    pub async fn run_all(
        &self,
        mut tasks: Vec<TaskDefinition>,
        options: TaskRunOptions,
    ) -> Result<TaskRunReport> {
        // 先做一次稳定排序：
        // - 保证同优先级任务在不同运行时也有一致顺序（更利于测试与日志对齐）
        tasks.sort_by_key(|task| (task.priority.rank(), task.id.clone()));
        // 注意：这里的排序只是“初始入队顺序”。
        // 真正运行时仍会受到 executor 里的 group limit / pause / cancel / stop_on_error 影响，
        // 所以最终完成顺序不一定等于这里的排序结果。
        // `TaskExecutor` 专注“调度和执行”，`TaskService` 专注“状态和事件”。
        // 两者拆开后，任务系统更容易测试，也更容易替换 UI/sink 层。
        let executor = TaskExecutor::new(self.clone(), options);
        executor.add_tasks(tasks);
        executor.run().await
    }

    pub(crate) fn emit_rollback_event(
        &self,
        task_id: &str,
        kind: TaskEventKind,
        detail: Option<String>,
    ) {
        let (record, sinks, event) = {
            let mut state = self.state.lock().expect("task store poisoned");
            let record = match state.tasks.iter().find(|task| task.id == task_id).cloned() {
                Some(record) => record,
                None => return,
            };
            // rollback 事件只追加到 event stream，不直接覆写 `TaskRecord.state`。
            // 原因是：
            // - 任务的主状态仍然是 Completed/Failed/Cancelled 之一
            // - rollback 更像“围绕主状态发生的附加生命周期事件”
            // 这样 UI/调用方既能知道主结果，也能知道后续是否做过补偿动作。
            let event = emit_event(&mut state, task_id.to_string(), kind, detail);
            (record, state.sinks.clone(), event)
        };
        notify_sinks(&sinks, &record, &event);
    }
}
