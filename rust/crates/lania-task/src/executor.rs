//! 任务执行器：负责调度任务队列、控制并发、处理取消与回滚。
//!
//! 这个文件是任务系统里“最像调度器”的部分：
//! - 它维护一个待执行队列 `queue`
//! - 根据优先级、group concurrency、暂停/停止状态挑选可运行任务
//! - 通过 Tokio `LocalSet + spawn_local` 执行任务
//! - 在失败或取消时，按配置决定是否执行 rollback
//!
//! 新手可以把它和 `TaskService` 对照着看：
//! - `TaskExecutor` 关心“怎么跑”
//! - `TaskService` 关心“跑完后状态怎么记、事件怎么发”

use std::{
    sync::{Arc, Mutex},
};

use anyhow::Result;
use tokio::sync::{mpsc, Notify};
use tokio::task::LocalSet;
use tokio_util::sync::CancellationToken;

use crate::helpers::execute_task_no_semaphore;
use crate::service::TaskService;
use crate::types::{
    TaskDefinition, TaskEventKind, TaskExecutionResult, TaskRollback, TaskRunMode, TaskRunOptions,
    TaskRunReport, TaskState,
};

#[derive(Debug, Clone)]
pub struct TaskExecutor {
    service: TaskService,
    options: TaskRunOptions,
    // 为什么这里是 `Arc<Mutex<Vec<TaskDefinition>>>`？
    // - `Vec<TaskDefinition>` 本身是调度队列，调度器需要不断从里面挑任务；
    // - `run_inner` 会把任务派发到异步任务里，因此执行器自身会被多个闭包共享；
    // - `Arc` 负责“多处共享所有权”，让多个异步分支都能拿到同一份队列句柄；
    // - `Mutex` 负责“同一时刻只允许一个地方修改队列”，否则多个任务同时 pop/remove
    //   会造成数据竞争。
    //
    // 这里使用的是 `std::sync::Mutex`，而不是 `tokio::sync::Mutex`，原因是：
    // - 临界区非常短，只是做本地内存读写；
    // - 锁不会跨 `.await` 持有；
    // - 同步 Mutex 开销更低，也更容易表达“这只是一个普通共享内存保护锁”。
    queue: Arc<Mutex<Vec<TaskDefinition>>>,
    paused: Arc<std::sync::atomic::AtomicBool>,
    stopped: Arc<std::sync::atomic::AtomicBool>,
    notify: Arc<Notify>,
    cancellation: CancellationToken,
    // “按组取消”和“运行中的任务集合”同样需要被多个异步分支共享，因此也用 `Arc<Mutex<...>>`。
    group_cancellation: Arc<Mutex<std::collections::HashMap<String, CancellationToken>>>,
    running_ids: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl TaskExecutor {
    pub fn new(service: TaskService, options: TaskRunOptions) -> Self {
        let cancellation = options.cancellation.clone().unwrap_or_default();
        Self {
            service,
            options,
            queue: Arc::new(Mutex::new(Vec::new())),
            paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
            cancellation,
            group_cancellation: Arc::new(Mutex::new(std::collections::HashMap::new())),
            running_ids: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }

    pub fn add_task(&self, task: TaskDefinition) {
        // 任务注册（TaskService）与任务排队（queue）分离：
        // - register 负责让 sink/UI “看见”任务（即使还没开始跑）
        // - queue 负责调度顺序与并发控制
        self.service.register(
            task.id.clone(),
            task.title.clone(),
            task.group.clone(),
            task.priority,
        );
        self.queue.lock().expect("task queue poisoned").push(task);
        self.notify.notify_waiters();
    }

    pub fn add_tasks(&self, tasks: Vec<TaskDefinition>) {
        for task in tasks {
            self.add_task(task);
        }
    }

    pub fn pause(&self) {
        // pause 只阻止“调度新任务”，不会强制中断已运行任务。
        self.paused.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn resume(&self) {
        if self.paused.swap(false, std::sync::atomic::Ordering::SeqCst) {
            // resume 后显式唤醒调度循环，
            // 否则它可能还在等 `notify` / `rx.recv()`，不能立刻发现“现在可以继续派发任务了”。
            self.notify.notify_waiters();
        }
    }

    pub fn cancel_all(&self) {
        // cancel_all 触发全局 cancellation token：
        // - 已运行任务如果尊重 token，会尽快退出
        // - 调度器会停止继续派发
        self.stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.cancellation.cancel();
        // 这里同样要唤醒调度器，让它尽快从 `select!` 中醒来，进入 stopped/cancelled 分支。
        self.notify.notify_waiters();
    }

    pub fn cancel_group(&self, group: &str) {
        // group cancel 使用 child token：既能按组停止，又能继承全局 cancel（cancel_all 时也会触发）。
        let token = {
            let mut map = self
                .group_cancellation
                .lock()
                .expect("task group cancellation poisoned");
            map.entry(group.to_string())
                .or_insert_with(|| self.cancellation.child_token())
                .clone()
        };
        token.cancel();
        // group cancel 不一定会立刻带来 `rx.recv()` 事件，因此主动 notify 一次，
        // 让调度器重新检查队列里同组任务是否还应该继续派发。
        self.notify.notify_waiters();
    }

    fn group_token(&self, group: &str) -> CancellationToken {
        let mut map = self
            .group_cancellation
            .lock()
            .expect("task group cancellation poisoned");
        // 同一 group 始终复用同一个 child token。
        // 这样“之前已经 cancel 过的组”后来再取 token 时，状态也会保持已取消。
        map.entry(group.to_string())
            .or_insert_with(|| self.cancellation.child_token())
            .clone()
    }

    pub fn queued_task_ids(&self) -> Vec<String> {
        self.queue
            .lock()
            .expect("task queue poisoned")
            .iter()
            .map(|task| task.id.clone())
            .collect()
    }

    pub fn running_task_ids(&self) -> Vec<String> {
        self.running_ids
            .lock()
            .expect("task running set poisoned")
            .iter()
            .cloned()
            .collect()
    }

    pub async fn run(&self) -> Result<TaskRunReport> {
        // Run the scheduler and all spawned tasks on the current thread so task
        // runners are allowed to be !Send.
        // LocalSet 的意义：
        // - 允许 task runner 捕获 !Send 资源（例如某些 UI handle）
        // - 代价是必须在同一个线程上调度 spawn_local
        // 可以把 `LocalSet` 理解成 Tokio 提供的“单线程异步沙盒”。
        let local = LocalSet::new();
        local.run_until(self.run_inner()).await
    }

    async fn run_inner(&self) -> Result<TaskRunReport> {
        let global_limit = match self.options.mode {
            TaskRunMode::Serial => 1,
            TaskRunMode::Parallel => self.options.max_concurrency.max(1),
        };

        // 调度器和已启动任务之间通过 channel 通信：
        // - 子任务结束时，把 `(group, result)` 发回调度器；
        // - 调度器据此更新运行计数、决定是否继续派发新任务。
        //
        // 这里选 `unbounded_channel`，是因为每个任务理论上只会回传一次最终结果，
        // 消息量非常有限，使用无界队列能让发送端更简单，不需要在任务收尾阶段再等待容量。
        let (tx, mut rx) = mpsc::unbounded_channel::<(String, TaskExecutionResult)>();
        let mut report = TaskRunReport::default();
        let mut rollback_stack: Vec<(String, TaskRollback)> = Vec::new();

        let mut global_running: usize = 0;
        let mut group_running: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        loop {
            let paused = self.paused.load(std::sync::atomic::Ordering::SeqCst);
            let stopped = self.stopped.load(std::sync::atomic::Ordering::SeqCst);

            // Schedule more work when possible.
            if !paused && !stopped {
                loop {
                    if global_running >= global_limit {
                        break;
                    }

                    let maybe_next = {
                        let mut queue = self.queue.lock().expect("task queue poisoned");
                        // Find the highest-priority task that can run within group limit.
                        // 选择策略：
                        // - 优先级越高越先跑（rank 越小越优先）
                        // - 同优先级用 id 作为稳定排序键，避免调度抖动
                        // - 同时满足 group_concurrency 限制（某些组可以更严格）
                        let mut best_index: Option<usize> = None;
                        let mut best_key: Option<(u8, String)> = None;
                        for (idx, task) in queue.iter().enumerate() {
                            let group_limit = self
                                .options
                                .group_concurrency
                                .get(&task.group)
                                .copied()
                                .unwrap_or(global_limit)
                                .max(1);
                            let running = group_running.get(&task.group).copied().unwrap_or(0);
                            if running >= group_limit {
                                continue;
                            }
                            let key = (task.priority.rank(), task.id.clone());
                            if best_key.as_ref().map(|k| key < *k).unwrap_or(true) {
                                best_key = Some(key);
                                best_index = Some(idx);
                            }
                        }
                        // 注意这里不是简单地 `pop()`，而是先扫描、再 `remove(best_index)`：
                        // 调度器想表达的是“从所有可运行任务里选最合适的那个”，
                        // 而不是“谁先入队谁先跑”。
                        best_index.map(|idx| queue.remove(idx))
                    };

                    let Some(task) = maybe_next else {
                        break;
                    };

                    let group = task.group.clone();
                    self.running_ids
                        .lock()
                        .expect("task running set poisoned")
                        .insert(task.id.clone());
                    // 这两个计数器分别回答两个不同问题：
                    // - `global_running`: 全局还有多少任务在跑，用来卡总并发
                    // - `group_running`: 某个 group 里还有多少任务在跑，用来卡组内并发
                    //
                    // 两者都在“真正 spawn 之前”增加，这样 select loop 下一轮看到的状态
                    // 才能准确反映“这个任务已经占用了并发槽位”。
                    *group_running.entry(group.clone()).or_insert(0) += 1;
                    global_running += 1;

                    let token = self.group_token(&group);
                    let service = self.service.clone();
                    let tx = tx.clone();
                    let running_ids = Arc::clone(&self.running_ids);
                    tokio::task::spawn_local(async move {
                        // execute_task_no_semaphore 内部会：
                        // - 发送 task events（started/updated/completed/failed/cancelled）
                        // - 处理 retries/backoff（如果 task 定义了）
                        // - 尊重 cancellation token
                        let result = execute_task_no_semaphore(service, task, token).await;
                        let task_id = result.outcome.id.clone();
                        let _ = tx.send((group, result));
                        // `running_ids` 的更新放在任务结束的最后一刻，
                        // 这样外部读取时看到的“正在运行集合”会更接近真实状态。
                        if let Ok(mut set) = running_ids.lock() {
                            set.remove(&task_id);
                        }
                    });
                }
            }

            // Exit condition: nothing queued and nothing running.
            let queue_empty = self.queue.lock().expect("task queue poisoned").is_empty();
            if queue_empty && global_running == 0 {
                // 必须同时满足“队列空”且“没有任务仍在运行”才能退出。
                // 只看 queue_empty 不够，因为很多任务已经被 spawn 出去了，不再待在队列里。
                break;
            }

            tokio::select! {
                // `select!` 的意思是“谁先准备好就先处理谁”。
                // 这个调度循环同时等待三类信号：
                // - notify：外界新增任务 / resume / cancel_group
                // - cancellation：全局取消
                // - rx.recv()：某个已运行任务结束
                _ = self.notify.notified() => {},
                _ = self.cancellation.cancelled(), if !stopped => {
                    self.stopped.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                Some((group, result)) = rx.recv() => {
                    // 这里用 saturating_sub 避免在极端情况下（逻辑错误/重复回调）下溢。
                    global_running = global_running.saturating_sub(1);
                    if let Some(count) = group_running.get_mut(&group) {
                        *count = count.saturating_sub(1);
                    }

                    if let Some(failure) = result.failure.clone() {
                        report.failures.push(failure);
                        if self.options.stop_on_error {
                            // stop_on_error 采用“尽快停”的策略：触发全局 cancellation，
                            // 让其它任务自行退出；调度器也会停止继续派发。
                            // 注意这不是“立刻强杀所有任务”，而是 cooperative cancellation：
                            // 任务是否能快速停下，还取决于 runner 是否尊重 token。
                            self.cancellation.cancel();
                            self.stopped.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                    if let Some(rollback) = result.rollback {
                        // rollback 先收集，不在这里立刻执行，
                        // 是因为当前可能还有其它任务在收尾；统一在主循环退出后逆序处理更安全。
                        rollback_stack.push(rollback);
                    }
                    report.cancelled |= result.outcome.state == TaskState::Cancelled;
                    // report.outcomes 保存“每个任务的最终结果快照”，
                    // 它和 TaskService 里的 live snapshot 是两套用途不同的数据：
                    // - TaskService 面向运行时观察
                    // - report 面向这次 `run_all()` 调用的最终返回值
                    report.outcomes.push(result.outcome);

                    // Wake the scheduler to potentially start more work immediately.
                    self.notify.notify_waiters();
                }
            }
        }

        if (!report.failures.is_empty() || report.cancelled) && self.options.rollback_on_error {
            // rollback 逆序执行：后开始的任务先回滚，符合“栈式资源释放”的直觉。
            // 这和函数调用栈/RAII 的释放顺序很像：越晚拿到的资源，越早释放更安全。
            for (task_id, rollback) in rollback_stack.into_iter().rev() {
                self.service
                    .emit_rollback_event(&task_id, TaskEventKind::RollbackStarted, None);
                match rollback().await {
                    Ok(()) => {
                        // rollback 成功/失败都只记录为事件，不改变前面已经确定的主结果。
                        self.service.emit_rollback_event(
                            &task_id,
                            TaskEventKind::RollbackCompleted,
                            None,
                        );
                        report.rolled_back.push(task_id);
                    }
                    Err(error) => {
                        self.service.emit_rollback_event(
                            &task_id,
                            TaskEventKind::RollbackFailed,
                            Some(error.to_string()),
                        );
                    }
                }
            }
        }

        // 走到这里时，report 已经是这次 run_all 的“最终结算单”：
        // - `outcomes`：每个任务最后落在什么状态
        // - `failures`：哪些任务以失败结束
        // - `rolled_back`：哪些任务的补偿动作成功执行
        // - `cancelled`：整轮执行里是否出现过取消
        Ok(report)
    }
}
