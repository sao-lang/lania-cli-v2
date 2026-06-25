//! 运行时 Hook 总线、事件模型，以及 v2.1 的 `onXxx` hook key 定义。
//!
//! 之所以从 `lania-host` 中抽出来，是为了让 workflows/fs/exec 等 crate
//! 可以依赖 hook 运行时能力，而不会与 `lania-host` 形成循环依赖。
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookKind {
    Waterfall,
    Parallel,
}

/// 对外公开的 HookKey 列表（字符串形式），用于 schema、线协议以及日志记录。
pub mod hook_keys {
    pub const ON_INITIALIZE: &str = "onInitialize";
    pub const ON_COMMAND_PRE_INIT: &str = "onCommandPreInit";
    pub const ON_ARGS_PARSED: &str = "onArgsParsed";
    pub const ON_FILES_PREPARE: &str = "onFilesPrepare";
    pub const ON_CONFIG_GET: &str = "onConfigGet";
    pub const ON_CONFIG_RESOLVE: &str = "onConfigResolve";
    pub const ON_FILE_WRITE: &str = "onFileWrite";
    pub const ON_TEMPLATE_PARSE: &str = "onTemplateParse";
    pub const ON_DEPENDENCIES_MODIFY: &str = "onDependenciesModify";
    pub const ON_DEPENDENCIES_INSTALL: &str = "onDependenciesInstall";
    pub const ON_INTERACTION_PROMPT: &str = "onInteractionPrompt";
    pub const ON_SHELL_COMMAND: &str = "onShellCommand";
    pub const ON_PLUGIN_API_CALL: &str = "onPluginApiCall";
    pub const ON_SUCCESS: &str = "onSuccess";
    pub const ON_ERROR: &str = "onError";
    pub const ON_PLUGIN_LOADED: &str = "onPluginLoaded";
    pub const ON_COMMAND_REGISTER: &str = "onCommandRegister";
    pub const ON_WORKFLOW_START: &str = "onWorkflowStart";
    pub const ON_WORKFLOW_COMPLETE: &str = "onWorkflowComplete";
    pub const ON_SHUTDOWN: &str = "onShutdown";

    pub const ALL: [&str; 20] = [
        ON_INITIALIZE,
        ON_COMMAND_PRE_INIT,
        ON_ARGS_PARSED,
        ON_FILES_PREPARE,
        ON_CONFIG_GET,
        ON_CONFIG_RESOLVE,
        ON_FILE_WRITE,
        ON_TEMPLATE_PARSE,
        ON_DEPENDENCIES_MODIFY,
        ON_DEPENDENCIES_INSTALL,
        ON_INTERACTION_PROMPT,
        ON_SHELL_COMMAND,
        ON_PLUGIN_API_CALL,
        ON_SUCCESS,
        ON_ERROR,
        ON_PLUGIN_LOADED,
        ON_COMMAND_REGISTER,
        ON_WORKFLOW_START,
        ON_WORKFLOW_COMPLETE,
        ON_SHUTDOWN,
    ];
}

pub fn is_known_hook_key(key: &str) -> bool {
    hook_keys::ALL.contains(&key)
}

pub fn default_hook_kind(key: &str) -> HookKind {
    match key {
        hook_keys::ON_COMMAND_PRE_INIT
        | hook_keys::ON_ARGS_PARSED
        | hook_keys::ON_FILES_PREPARE
        | hook_keys::ON_CONFIG_GET
        | hook_keys::ON_CONFIG_RESOLVE
        | hook_keys::ON_TEMPLATE_PARSE
        | hook_keys::ON_DEPENDENCIES_MODIFY
        | hook_keys::ON_INTERACTION_PROMPT => HookKind::Waterfall,
        // 默认归类为 Parallel 的语义是：
        // “除非某个 hook 明确需要改写输入，否则把它当成旁路观察/通知点”。
        // 这样新增 hook key 时，不容易因为误用 waterfall 而产生隐式数据改写。
        _ => HookKind::Parallel,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRegistration {
    pub key: String,
    pub kind: HookKind,
    pub plugin: String,
    pub handler: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEvent {
    pub sequence: u64,
    pub key: String,
    pub source: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookSnapshot {
    pub registrations: Vec<HookRegistration>,
    pub events: Vec<HookEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInvokeOutcome {
    /// 仅对 Waterfall hook 有意义：
    /// 当返回 Some(payload) 时，后续链路看到的 payload 会被替换为新值。
    /// Parallel hook 即使返回 payload，也不会被后续调用方消费。
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookErrorPolicy {
    #[default]
    Throw,
    Collect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HookInvokerOptions {
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub on_error: HookErrorPolicy,
}

#[derive(Debug, Clone, Default)]
pub struct HookCallOptions {
    pub cancellation: Option<CancellationToken>,
}

#[async_trait]
pub trait HookInvoker: Send + Sync {
    async fn invoke(
        &self,
        source: &str,
        hook_key: &str,
        kind: HookKind,
        payload: &Value,
    ) -> Result<HookInvokeOutcome>;
}

#[async_trait]
pub trait HookRuntime: Send + Sync {
    fn register(&mut self, registration: HookRegistration);
    fn register_invoker(&mut self, hook_key: String, invoker: Arc<dyn HookInvoker>);
    fn register_invoker_with_options(
        &mut self,
        hook_key: String,
        invoker: Arc<dyn HookInvoker>,
        options: HookInvokerOptions,
    );
    fn record_event(&self, source: String, hook_key: String, payload: Value);
    async fn call_parallel(&self, source: String, hook_key: String, payload: Value) -> Result<()>;
    async fn call_parallel_with_options(
        &self,
        source: String,
        hook_key: String,
        payload: Value,
        options: HookCallOptions,
    ) -> Result<()>;
    async fn call_waterfall(
        &self,
        source: String,
        hook_key: String,
        payload: Value,
    ) -> Result<Value>;
    async fn call_waterfall_with_options(
        &self,
        source: String,
        hook_key: String,
        payload: Value,
        options: HookCallOptions,
    ) -> Result<Value>;
    fn snapshot(&self) -> HookSnapshot;
}

#[derive(Clone)]
struct HookInvokerEntry {
    hook_key: String,
    invoker: Arc<dyn HookInvoker>,
    options: HookInvokerOptions,
}

pub struct HookBusImpl {
    registrations: Vec<HookRegistration>,
    invokers: Vec<HookInvokerEntry>,
    // `events` 保存的是“发生过哪些 hook 调用”，主要用于调试/快照输出，
    // 而不是驱动实时执行逻辑。
    events: Arc<Mutex<Vec<HookEvent>>>,
    // sequence 用原子计数器生成全局递增序号，让快照里的事件顺序稳定可追踪。
    sequence: Arc<AtomicU64>,
}

impl HookBusImpl {
    pub fn new() -> Self {
        Self {
            registrations: Vec::new(),
            invokers: Vec::new(),
            events: Arc::new(Mutex::new(Vec::new())),
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Default for HookBusImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HookRuntime for HookBusImpl {
    fn register(&mut self, registration: HookRegistration) {
        self.registrations.push(registration);
    }

    fn register_invoker(&mut self, hook_key: String, invoker: Arc<dyn HookInvoker>) {
        self.register_invoker_with_options(hook_key, invoker, HookInvokerOptions::default());
    }

    fn register_invoker_with_options(
        &mut self,
        hook_key: String,
        invoker: Arc<dyn HookInvoker>,
        options: HookInvokerOptions,
    ) {
        self.invokers.push(HookInvokerEntry {
            hook_key,
            invoker,
            options,
        });
    }

    fn record_event(&self, source: String, hook_key: String, payload: Value) {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        // 这里 sequence 和 events 分开存的组合很常见：
        // - sequence 负责“唯一递增编号”
        // - Vec 负责“按发生顺序保留历史”
        // 即使未来事件结构扩展，序号生成策略也能独立保持稳定。
        self.events
            .lock()
            .expect("hook store poisoned")
            .push(HookEvent {
                sequence,
                key: hook_key,
                source,
                payload,
            });
    }

    async fn call_parallel(&self, source: String, hook_key: String, payload: Value) -> Result<()> {
        self.call_parallel_with_options(source, hook_key, payload, HookCallOptions::default())
            .await
    }

    async fn call_parallel_with_options(
        &self,
        source: String,
        hook_key: String,
        payload: Value,
        options: HookCallOptions,
    ) -> Result<()> {
        self.record_event(source.clone(), hook_key.clone(), payload.clone());
        // `parallel` 的语义重点不是“并行执行得多快”，而是“每个 invoker 都看到同一份原始 payload”。
        // 也就是说：
        // - 某个 hook 的输出不会成为下一个 hook 的输入
        // - 更适合日志、观测、上报、通知这类旁路副作用
        for entry in &self.invokers {
            if entry.hook_key == hook_key {
                let invoke_fut = async {
                    let fut =
                        entry
                            .invoker
                            .invoke(&source, &hook_key, HookKind::Parallel, &payload);
                    if let Some(token) = options.cancellation.as_ref() {
                        tokio::select! {
                            // 取消优先级高于 hook 完成：一旦上层决定停止，这一轮 hook 结果就不再重要。
                            _ = token.cancelled() => Err(anyhow::anyhow!("hook cancelled")),
                            out = fut => out,
                        }
                    } else {
                        fut.await
                    }
                };
                let result = if let Some(timeout_ms) = entry.options.timeout_ms {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(timeout_ms),
                        invoke_fut,
                    )
                    .await
                    {
                        Ok(out) => out,
                        Err(_) => Err(anyhow::anyhow!("hook timed out after {timeout_ms}ms")),
                    }
                } else {
                    invoke_fut.await
                };
                if let Err(error) = result {
                    if entry.options.on_error == HookErrorPolicy::Throw {
                        return Err(error);
                    }
                }
            }
        }
        tokio::task::yield_now().await;
        Ok(())
    }

    async fn call_waterfall(
        &self,
        source: String,
        hook_key: String,
        payload: Value,
    ) -> Result<Value> {
        self.call_waterfall_with_options(source, hook_key, payload, HookCallOptions::default())
            .await
    }

    async fn call_waterfall_with_options(
        &self,
        source: String,
        hook_key: String,
        payload: Value,
        options: HookCallOptions,
    ) -> Result<Value> {
        self.record_event(source.clone(), hook_key.clone(), payload.clone());
        // `waterfall` 和 `parallel` 最大的区别在这里：
        // `current` 会被前一个 hook 的输出不断改写，然后传给下一个 hook。
        //
        // 所以 waterfall 更像“管道变换”：
        // 原始 payload -> hook1 改写 -> hook2 继续改写 -> ... -> 最终 payload
        let mut current = payload;
        for entry in &self.invokers {
            if entry.hook_key == hook_key {
                let invoke_fut = async {
                    let fut =
                        entry
                            .invoker
                            .invoke(&source, &hook_key, HookKind::Waterfall, &current);
                    if let Some(token) = options.cancellation.as_ref() {
                        tokio::select! {
                            _ = token.cancelled() => Err(anyhow::anyhow!("hook cancelled")),
                            out = fut => out,
                        }
                    } else {
                        fut.await
                    }
                };
                let result = if let Some(timeout_ms) = entry.options.timeout_ms {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(timeout_ms),
                        invoke_fut,
                    )
                    .await
                    {
                        Ok(out) => out,
                        Err(_) => Err(anyhow::anyhow!("hook timed out after {timeout_ms}ms")),
                    }
                } else {
                    invoke_fut.await
                };
                match result {
                    Ok(outcome) => {
                        if let Some(next) = outcome.payload {
                            // 只有显式返回 payload 时才覆盖 `current`；
                            // 没返回就表示“我观察了，但不修改输入”。
                            current = next;
                        }
                    }
                    Err(error) => {
                        if entry.options.on_error == HookErrorPolicy::Throw {
                            return Err(error);
                        }
                    }
                }
            }
        }
        Ok(current)
    }

    fn snapshot(&self) -> HookSnapshot {
        HookSnapshot {
            registrations: self.registrations.clone(),
            events: self.events.lock().expect("hook store poisoned").clone(),
        }
    }
}
