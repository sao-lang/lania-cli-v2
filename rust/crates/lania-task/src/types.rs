//! 任务系统的数据模型与构建器入口。
//!
//! 这个文件主要回答两个问题：
//! - “一个任务在 Rust 里被表示成什么？”
//! - “为什么任务 runner 的类型会写得这么复杂？”
//!
//! 这里定义了：
//! - `TaskDefinition`：用户真正创建和配置的任务定义
//! - `TaskRecord` / `TaskEvent`：任务运行过程中对外暴露的状态和事件
//! - `TaskRunOptions` / `TaskRunReport`：一次批量执行的输入输出
//! - 一批和异步闭包相关的 type alias，例如 `TaskFuture` / `TaskRunner`

use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc, time::Duration};

use anyhow::Result;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

// 这些 type alias 是任务系统最核心、也最“Rust 味”的一部分。
//
// 为什么这里会出现 `Pin<Box<dyn Future<...>>>`？
// - `async fn`/`async move` 实际会编译成一个匿名 Future 类型；
// - 这个匿名类型很长，而且每个闭包/函数生成的具体类型都不同；
// - 任务系统希望“把不同来源的异步任务统一放进一个容器里”，所以不能直接写具体类型，
//   只能做成 trait object，也就是 `dyn Future<...>`；
// - trait object 需要放在堆上，因此用 `Box`；
// - 大多数 Future 在被 poll 之后不能再随便移动，Rust 因此要求外层再包一层 `Pin`。
//
// 另外这里刻意没有要求 `TaskFuture: Send`：
// - 这个 CLI 里有些异步链路（尤其 prompt / bridge / UI 句柄）会捕获 `!Send` 资源；
// - 因此任务系统必须允许“只能在当前线程运行”的 Future；
// - 对应地，执行器会用 `tokio::task::LocalSet + spawn_local` 来保证这些 Future
//   不会被 Tokio 调度到别的线程上。
pub(crate) type TaskFuture = Pin<Box<dyn Future<Output = Result<Value>> + 'static>>;
pub(crate) type RollbackFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
// `TaskRunner`/`TaskRollback` 再包一层 `Arc`，是为了让任务定义可以被 clone。
// 这里 clone 的不是“复制一份闭包逻辑”，而是多个地方共享同一个闭包对象的所有权。
pub(crate) type TaskRunner = Arc<dyn Fn(CancellationToken) -> TaskFuture + Send + Sync>;
pub(crate) type TaskRollback = Arc<dyn Fn() -> RollbackFuture + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    High,
    Medium,
    Low,
}

impl TaskPriority {
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Self::High => 0,
            Self::Medium => 1,
            Self::Low => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub group: String,
    pub priority: TaskPriority,
    pub state: TaskState,
    pub detail: Option<String>,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEventKind {
    Registered,
    Started,
    Updated,
    Completed,
    Failed,
    Cancelled,
    RollbackStarted,
    RollbackCompleted,
    RollbackFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEvent {
    pub sequence: u64,
    pub task_id: String,
    pub kind: TaskEventKind,
    pub detail: Option<String>,
}

pub trait TaskSink: Send + Sync {
    fn on_event(&self, record: &TaskRecord, event: &TaskEvent);
}

#[derive(Clone)]
pub struct TaskDefinition {
    pub id: String,
    pub title: String,
    pub group: String,
    pub priority: TaskPriority,
    pub timeout: Option<Duration>,
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub(crate) runner: TaskRunner,
    pub(crate) rollback: Option<TaskRollback>,
}

impl std::fmt::Debug for TaskDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskDefinition")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("group", &self.group)
            .field("priority", &self.priority)
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .field("retry_delay", &self.retry_delay)
            .field("has_rollback", &self.rollback.is_some())
            .finish()
    }
}

impl TaskDefinition {
    pub fn new<F, Fut>(id: impl Into<String>, title: impl Into<String>, runner: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + 'static,
    {
        Self {
            id: id.into(),
            title: title.into(),
            group: "default".into(),
            priority: TaskPriority::Medium,
            timeout: None,
            max_retries: 0,
            retry_delay: Duration::from_millis(50),
            // `runner(token)` 返回的是某个具体 Future 类型；
            // 这里立刻 `Box::pin(...)`，把它擦除成统一的 `TaskFuture`，
            // 这样调度器就不用知道每个任务真实返回的 Future 长什么样。
            runner: Arc::new(move |token| Box::pin(runner(token))),
            rollback: None,
        }
    }

    pub fn new_text<F, Fut>(id: impl Into<String>, title: impl Into<String>, runner: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<String>> + 'static,
    {
        // 大多数“文本任务”只关心最终返回一段文字，而任务系统内部统一使用 `serde_json::Value`
        // 保存输出。这里做一层轻量适配，让上层 API 更好写。
        let runner = Arc::new(runner);
        Self::new(id, title, move |token| {
            let runner = Arc::clone(&runner);
            async move { runner(token).await.map(Value::String) }
        })
    }

    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }

    pub fn priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn retries(mut self, max_retries: u32, retry_delay: Duration) -> Self {
        self.max_retries = max_retries;
        self.retry_delay = retry_delay;
        self
    }

    pub fn rollback<F, Fut>(mut self, rollback: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        // rollback 要求 `Send`，因为回滚通常更偏“基础设施动作”（删文件、撤销状态等），
        // 不希望再依赖只能待在单线程里的 UI/交互资源。
        self.rollback = Some(Arc::new(move || Box::pin(rollback())));
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRunMode {
    Serial,
    Parallel,
}

#[derive(Debug, Clone)]
pub struct TaskRunOptions {
    pub mode: TaskRunMode,
    pub max_concurrency: usize,
    pub group_concurrency: BTreeMap<String, usize>,
    pub stop_on_error: bool,
    pub rollback_on_error: bool,
    pub cancellation: Option<CancellationToken>,
}

impl Default for TaskRunOptions {
    fn default() -> Self {
        Self {
            mode: TaskRunMode::Parallel,
            max_concurrency: 64,
            group_concurrency: BTreeMap::new(),
            stop_on_error: true,
            rollback_on_error: false,
            cancellation: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOutcome {
    pub id: String,
    pub state: TaskState,
    pub detail: Option<String>,
    pub data: Option<Value>,
    pub attempts: u32,
    pub group: String,
    pub priority: TaskPriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFailure {
    pub id: String,
    pub message: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskRunReport {
    pub outcomes: Vec<TaskOutcome>,
    pub failures: Vec<TaskFailure>,
    pub rolled_back: Vec<String>,
    pub cancelled: bool,
}

pub(crate) struct TaskExecutionResult {
    pub(crate) outcome: TaskOutcome,
    pub(crate) failure: Option<TaskFailure>,
    pub(crate) rollback: Option<(String, TaskRollback)>,
}
