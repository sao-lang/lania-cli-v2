//! 命令执行上下文：把 host 运行一次命令所需的所有能力打包在一起。
//!
//! 可以把 `CommandExecutionContext` 理解成 workflow 的“运行时背包”：
//! - 当前命令是谁（`CommandContext`）
//! - 可以调用哪些基础服务（exec/fs/git/prompt/progress/tasks）
//! - 可以访问哪些跨边界能力（node bridge / hooks / capability resolver）
//! - 本次执行的策略参数（超时、重试、自动中断）
//!
//! 这样设计的好处是：
//! - workflow 函数不需要接收一长串零散参数
//! - 测试时更容易替换局部能力
//! - “命令元信息”和“基础设施依赖”被清晰地收口在同一个入口里

use std::{env, future::Future, pin::Pin, sync::Arc};

use anyhow::{anyhow, Result};
use lania_command::CommandContext;
use lania_config::LanConfigSnapshot;
use lania_exec::ExecService;
use lania_fs::FsService;
use lania_git::GitService;
use lania_hooks::HookRuntime;
use lania_logger::LoggerService;
use lania_node_bridge::NodeBridgeClient;
use lania_pm::PackageManagerService;
use lania_progress::ProgressService;
use lania_prompt::PromptService;
use lania_task::{TaskDefinition, TaskRunMode, TaskRunOptions, TaskService};
use lania_workflows::{WorkflowExecution, WorkflowServices};
use serde::Serialize;

use crate::{CapabilityName, CapabilityResolver};

use super::types::EXIT_RUNTIME_ERROR;

type WorkflowRunnerFuture = Pin<Box<dyn Future<Output = Result<WorkflowExecution>>>>;
type WorkflowRunnerOnce = dyn FnOnce(WorkflowServices) -> WorkflowRunnerFuture + Send;
type WorkflowRunnerSlot = Arc<std::sync::Mutex<Option<Box<WorkflowRunnerOnce>>>>;
type WorkflowOutcomeSlot = Arc<tokio::sync::Mutex<Option<Result<WorkflowExecution>>>>;

pub struct HostExecutionServices<'a> {
    pub capabilities: &'a dyn CapabilityResolver,
    pub logger: &'a LoggerService,
    pub exec: &'a ExecService,
    pub fs: &'a FsService,
    pub tasks: &'a TaskService,
    pub progress: &'a ProgressService,
    pub prompt: &'a PromptService,
    pub git: &'a GitService,
    pub package_manager: &'a PackageManagerService,
    pub node_bridge: &'a NodeBridgeClient,
    pub hooks: Arc<dyn HookRuntime>,
    pub project_config: Option<LanConfigSnapshot>,
    pub locale: String,
}

pub struct CommandExecutionContext<'a> {
    pub(super) command: &'a CommandContext,
    // 这里大量使用 `&dyn Trait`，是 Rust 中很常见的“面向接口编程”写法。
    //
    // 为什么不是直接把具体类型写死？
    // - `CommandExecutionContext` 想表达的是“我需要某种能力”，而不是“我必须依赖某个具体实现”；
    // - 例如这里只要求一个 `CapabilityResolver`，至于背后是 HashMap、注册中心，还是测试 mock，
    //   context 并不关心；
    // - `dyn Trait` 会发生动态分发，性能上略有成本，但换来更清晰的抽象边界和更好的可替换性。
    pub(super) capabilities: &'a dyn CapabilityResolver,
    pub(super) logger: &'a LoggerService,
    pub(super) exec: &'a ExecService,
    pub(super) fs: &'a FsService,
    pub(super) tasks: &'a TaskService,
    pub(super) progress: &'a ProgressService,
    pub(super) prompt: &'a PromptService,
    pub(super) git: &'a GitService,
    pub(super) package_manager: &'a PackageManagerService,
    pub(super) node_bridge: &'a NodeBridgeClient,
    // `hooks` 使用 `Arc<dyn HookRuntime>` 而不是 `&dyn HookRuntime`：
    // - workflow / task 往往会跨异步边界、跨闭包 move；
    // - 普通借用 `&T` 生命周期更短，不容易安全地传进 `'static` 异步任务；
    // - `Arc` 让多个异步分支共享同一个 HookRuntime，并把生命周期问题转成所有权问题。
    pub(super) hooks: Arc<dyn HookRuntime>,
    pub(super) policy: ExecutionPolicy,
    pub(super) project_config: Option<LanConfigSnapshot>,
    pub(super) locale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPolicy {
    pub timeout_ms: u64,
    pub retry_attempts: usize,
    pub auto_interrupt_after_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ExecutionError {
    pub exit_code: i32,
    pub message: String,
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExecutionError {}

impl ExecutionPolicy {
    pub fn from_env(node_bridge: &NodeBridgeClient) -> Self {
        // 运行策略支持通过环境变量覆盖，便于 CI/调试：
        // - `LANIA_TIMEOUT_MS`: 单次 bridge 调用超时
        // - `LANIA_RETRY_ATTEMPTS`: bridge 调用失败后的重试次数
        // - `LANIA_INTERRUPT_AFTER_MS`: 自动模拟 Ctrl-C，用于测试/避免挂死
        Self {
            timeout_ms: env::var("LANIA_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(|| node_bridge.timeout().as_millis() as u64),
            retry_attempts: env::var("LANIA_RETRY_ATTEMPTS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            auto_interrupt_after_ms: env::var("LANIA_INTERRUPT_AFTER_MS")
                .ok()
                .and_then(|value| value.parse().ok()),
        }
    }
}

impl<'a> CommandExecutionContext<'a> {
    pub fn new(command: &'a CommandContext, services: HostExecutionServices<'a>) -> Self {
        // Context 的目标是“把执行所需的一切能力收拢到一个对象里”，避免 handler 参数爆炸；
        // 同时把 policy / locale / project_config 等运行时信息固定下来，保证一次命令执行内一致。
        Self {
            command,
            capabilities: services.capabilities,
            logger: services.logger,
            exec: services.exec,
            fs: services.fs,
            tasks: services.tasks,
            progress: services.progress,
            prompt: services.prompt,
            git: services.git,
            package_manager: services.package_manager,
            node_bridge: services.node_bridge,
            policy: ExecutionPolicy::from_env(services.node_bridge),
            hooks: services.hooks,
            project_config: services.project_config,
            locale: services.locale,
        }
    }

    pub fn command(&self) -> &CommandContext {
        self.command
    }

    pub fn prompt(&self) -> &PromptService {
        self.prompt
    }

    pub fn progress(&self) -> &ProgressService {
        self.progress
    }

    pub fn project_config(&self) -> Option<&LanConfigSnapshot> {
        self.project_config.as_ref()
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn has_capability(&self, name: CapabilityName) -> bool {
        self.capabilities.get(name).is_some()
    }

    pub fn require_capability(&self, name: CapabilityName) -> Result<()> {
        if self.has_capability(name) {
            return Ok(());
        }

        // 这里返回 ExecutionError 是为了让上层保留 exit code，
        // 否则会被 anyhow 折叠成“运行时错误”统一退出码，不利于脚本/CI 判断失败原因。
        Err(ExecutionError {
            exit_code: EXIT_RUNTIME_ERROR,
            message: format!(
                "missing capability {:?} for handler {}",
                name, self.command.handler_id
            ),
        }
        .into())
    }

    pub fn node_bridge(&self) -> &NodeBridgeClient {
        self.node_bridge
    }

    pub fn exec(&self) -> &ExecService {
        self.exec
    }

    pub fn hooks(&self) -> &dyn HookRuntime {
        self.hooks.as_ref()
    }

    pub fn policy(&self) -> &ExecutionPolicy {
        &self.policy
    }

    pub fn workflow_services(&self) -> WorkflowServices {
        // WorkflowServices 是“可 clone 的服务集合”，供 workflow 逻辑跨 async/任务边界传递。
        // 注意：里面很多字段是轻量 handle（Arc/clone），不是深拷贝。
        WorkflowServices {
            logger: self.logger.clone(),
            prompt: self.prompt.clone(),
            fs: self.fs.clone(),
            git: self.git.clone(),
            package_manager: self.package_manager.clone(),
            exec: self.exec.clone(),
            tasks: self.tasks.clone(),
            progress: self.progress.clone(),
            bridge: self.node_bridge.clone(),
            hooks: Arc::clone(&self.hooks),
            hook_cwd: self.command.cwd.clone(),
            hook_trace_id: self.command.trace_id.clone(),
            hook_command_handler_id: self.command.handler_id.clone(),
            locale: self.locale.clone(),
        }
    }

    pub async fn run_workflow_with_tasks<F, Fut>(
        &self,
        workflow_name: &str,
        title: &str,
        runner: F,
    ) -> Result<WorkflowExecution>
    where
        F: FnOnce(WorkflowServices) -> Fut + Send + 'static,
        Fut: Future<Output = Result<WorkflowExecution>> + 'static,
    {
        let services = self.workflow_services();
        let services_for_task = services.clone();

        // TaskDefinition 的回调签名是 Fn（可被调用多次），
        // 但 workflow runner 语义上是 FnOnce（只能跑一次）。
        // 这里用 Mutex<Option<...>> 把 FnOnce “装进” Fn，并确保只消费一次。
        //
        // 这是一种很典型的 Rust 技巧：
        // - `FnOnce` 不能直接塞进要求 `Fn` 的位置
        // - 于是先放进 `Option`
        // - 真正执行时通过 `take()` 把它“拿出来并置空”
        // 这样类型系统就能表达“外表像可重复调用的回调，但内部资源只能被消费一次”。
        let runner_slot: WorkflowRunnerSlot = Arc::new(std::sync::Mutex::new(Some(Box::new(move |services| {
            Box::pin(runner(services))
        }))));

        // 这里是另一个很值得学习的点：
        // - workflow 的真实返回值是 `Result<WorkflowExecution>`
        // - 但任务系统的 runner 只能返回 `Result<Value>`
        // 因此我们额外放一个 `outcome` 槽位，把真正的 workflow 结果暂存起来，
        // 等 `run_all()` 结束后再取回。
        //
        // 这里改用 `tokio::sync::Mutex`，而不是 `std::sync::Mutex`，原因是：
        // - 对 `outcome` 的读写发生在 async 代码里；
        // - 锁获取后附近马上就有 `.await`，更适合异步 Mutex；
        // - 异步 Mutex 在等待锁时不会阻塞整个线程，只会让当前 Future 挂起。
        //
        // 还有一点容易忽略：
        // `outcome` 不是为了“共享很多次写入”，而是为了跨越两层抽象边界传值：
        // - 内层 task runner 必须返回 `Result<Value>`
        // - 外层真正想拿到的是 `Result<WorkflowExecution>`
        // 所以这里相当于临时搭了一个“旁路返回值通道”。
        let outcome: WorkflowOutcomeSlot = Arc::new(tokio::sync::Mutex::new(None));
        let outcome_slot = Arc::clone(&outcome);
        let runner_slot_for_task = Arc::clone(&runner_slot);

        let task_id = format!("workflow.{workflow_name}");
        services
            .tasks
            .run_all(
                vec![TaskDefinition::new_text(task_id, title, move |_| {
                    let services = services_for_task.clone();
                    let outcome = Arc::clone(&outcome_slot);
                    let runner_slot = Arc::clone(&runner_slot_for_task);
                    async move {
                        // 只允许取出一次 runner：重复执行代表逻辑错误（任务系统重试需要显式设计）。
                        // 这里用的是 `std::sync::Mutex<Option<_>>` 组合：
                        // - `Mutex` 负责互斥，保证不会有两个执行分支同时取 runner；
                        // - `Option::take()` 负责“消费一次后就变成 None”，非常适合实现一次性语义。
                        let runner = runner_slot
                            .lock()
                            .expect("workflow runner poisoned")
                            .take()
                            .expect("workflow runner already consumed");
                        // 到这里拿到的 runner 已经拥有完整所有权，因此可以安全地 move `services`
                        // 并等待整个 workflow future 完成。
                        let result = runner(services).await;
                        match result {
                            Ok(execution) => {
                                *outcome.lock().await = Some(Ok(execution));
                                Ok("done".into())
                            }
                            Err(error) => {
                                // Task 需要返回 Err 才会标记任务失败；
                                // 但 workflow 的“真实错误”通过 outcome 传回给外层。
                                *outcome.lock().await = Some(Err(anyhow!("{error}")));
                                Err(error)
                            }
                        }
                    }
                })
                .group(workflow_name)],
                TaskRunOptions {
                    mode: TaskRunMode::Serial,
                    ..TaskRunOptions::default()
                },
            )
            .await?;

        // run_all 成功只代表 task 系统跑完，不代表 workflow 成功；
        // workflow 的 Ok/Err 由 outcome 决定。
        // 这一点很容易让新手误会：
        // - `run_all()` 关注的是“任务调度器有没有把这轮任务跑完”
        // - 但 task 内部包着的 workflow 仍然可能以业务错误结束
        // 所以这里必须再从 outcome 槽位取一次真正结果。
        let result = outcome
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow!("workflow task did not produce a result"))?;
        result
    }
}
