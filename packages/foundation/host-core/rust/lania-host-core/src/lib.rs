//! 宿主层的桥接接缝（`package-split-command-atomization.zh-CN.md` §5.1 A.1 / A.5(3)(5)）。
//!
//! 本 crate 是 **host-core 的桥无关部分**：
//! - 不依赖任何语言 bridge crate，也不依赖 bridge-hub 的任何 crate；
//! - 只定义 trait 与 opaque 数据，具体实现由 `lania-bridge-host` 注入；
//! - `HostRuntime` 的桥接字段在过渡期仍持有具体 client，但对外统一经 `BridgeSlot`
//!   暴露 `attach_bridge()` / `bridge()`（`§5.5 E.1` 的接线形状），env / config 不调用即可保持零桥接。

use std::path::Path;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::Value;

/// 语言运行时的启动描述。host-core 只把它当 opaque 数据转发，不解析、不推断语言。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeLaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub dir: Option<String>,
}

/// 桥接层单条事件（零依赖，与具体语言的 `BridgeEventMethod` 解耦）。
///
/// `method` 是稳定事件名（如 `event.progress`），`params` 为事件载荷。
/// bridge-host 负责把 node-bridge 的 `BridgeEvent` 转成此类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeProviderEvent {
    pub method: String,
    pub params: Value,
}

impl BridgeProviderEvent {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            method: method.into(),
            params,
        }
    }
}

/// 桥接层调用结果：响应值 + 有序事件流。
///
/// Hub 单轨化后，`BridgeProvider::call` 必须把端点产生的事件一并返回，
/// 否则宿主无法渲染进度/日志（原 1:1 链路的 events 会因此丢失）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeProviderExchange {
    pub result: Value,
    pub events: Vec<BridgeProviderEvent>,
}

impl BridgeProviderExchange {
    pub fn new(result: Value, events: Vec<BridgeProviderEvent>) -> Self {
        Self { result, events }
    }

    /// 无事件的便捷构造（配置加载等纯请求/响应场景）。
    pub fn plain(result: Value) -> Self {
        Self {
            result,
            events: Vec::new(),
        }
    }
}

/// 桥接层视图（host-core 定义，bridge-host 实现）。
///
/// `call` 只发「方法名 + JSON 参数」，与 `RuntimeKind` / 具体语言无关；
/// `register_language` 的语言 id 是字符串，host-core 不引用语言枚举。
#[async_trait]
pub trait BridgeProvider: Send + Sync + 'static {
    /// 发起调用并返回响应值 + 有序事件流。
    ///
    /// 事件按端点产生顺序返回；宿主须在最终 response 之前消费它们，
    /// 以保持与 1:1 链路「events 先于 response」一致的顺序语义。
    async fn call(
        &self,
        method: &str,
        params: Value,
    ) -> anyhow::Result<BridgeProviderExchange>;

    /// 流式调用：每条事件到达时立即回调（agent.prompt 的逐 token 渲染）。
    /// 默认实现退化为先收集再回放；有实时通道的 provider 应覆盖以保序保真。
    async fn call_stream(
        &self,
        method: &str,
        params: Value,
        on_event: &mut (dyn FnMut(BridgeProviderEvent) + Send),
    ) -> anyhow::Result<Value> {
        let exchange = self.call(method, params).await?;
        for event in exchange.events {
            on_event(event);
        }
        Ok(exchange.result)
    }

    fn register_language(&self, id: &str, spec: BridgeLaunchSpec) -> anyhow::Result<()>;

    /// 释放 provider 持有的资源（如 spawn 的语言子进程）。
    ///
    /// 默认 no-op 兼容无进程的实现（如配置加载 Stub）；持有子进程的 provider
    /// 必须覆盖此方法，让宿主 `shutdown_async` 能在 `process::exit` 前显式清理。
    fn shutdown(&self) {}

    /// 排空 provider 侧新到达的事件（长任务运行期日志/进度）。
    ///
    /// 默认空集兼容一次性请求/响应实现。Hub 单轨下，长任务（dev/watch）的
    /// response 先返回，运行期事件随后经 Hub 到达；宿主在等待 Ctrl-C 期间
    /// 反复调用本方法把这些事件泵到渲染链，等价于 1:1 的 BridgeActiveCall。
    fn drain_events(&self) -> Vec<BridgeProviderEvent> {
        Vec::new()
    }
}

/// 项目配置读取接缝（§5.1 A.5(5)）。
/// 纯 `.yaml/.json` 由 `lania-config`（纯 Rust、零 bridge）实现并注入；
/// `.ts` 配置才经 `BridgeProvider::call("config.loadLan", ...)`。
#[async_trait]
pub trait ConfigSource: Send + Sync + 'static {
    async fn load(&self, cwd: &Path) -> Option<Value>;
}

/// 无桥接 / 无配置源时的回落实现（env、config 命令保持零语言运行时）。
pub struct NullConfigSource;

#[async_trait]
impl ConfigSource for NullConfigSource {
    async fn load(&self, _cwd: &Path) -> Option<Value> {
        None
    }
}

/// 命令未在 manifest 声明 hostDependencies 时由 ctx 能力访问器返回。
pub fn capability_not_linked(name: &str) -> anyhow::Error {
    anyhow::anyhow!("CapabilityNotLinked: {name}")
}

/// 可选的桥接注入槽（§5.1 A.5(3) 的 `Option<Arc<dyn BridgeProvider>>`）。
/// 用 `RwLock` 而非 `Rc<RefCell<...>>`，保证 `HostRuntime` 可跨线程共享。
#[derive(Clone, Default)]
pub struct BridgeSlot {
    inner: Arc<RwLock<Option<Arc<dyn BridgeProvider>>>>,
}

impl BridgeSlot {
    pub fn attach(&self, provider: Arc<dyn BridgeProvider>) {
        let mut guard = self.inner.write().expect("bridge slot poisoned");
        *guard = Some(provider);
    }

    pub fn get(&self) -> Option<Arc<dyn BridgeProvider>> {
        self.inner.read().expect("bridge slot poisoned").clone()
    }

    pub fn is_attached(&self) -> bool {
        self.inner.read().expect("bridge slot poisoned").is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub;

    #[async_trait]
    impl BridgeProvider for Stub {
        async fn call(
            &self,
            method: &str,
            params: Value,
        ) -> anyhow::Result<BridgeProviderExchange> {
            Ok(BridgeProviderExchange::plain(Value::Object(
                [("method".to_string(), Value::from(method)), ("params".to_string(), params)]
                    .into_iter()
                    .collect(),
            )))
        }

        fn register_language(&self, _id: &str, _spec: BridgeLaunchSpec) -> anyhow::Result<()> {
            Ok(())
        }
    }

    /// 事件承载契约：provider 必须能把有序事件流连同响应一起返回，
    /// 否则 Hub 单轨化后 dev/build/lint 的进度与日志会丢失。
    #[tokio::test]
    async fn provider_exchange_carries_ordered_events() {
        let exchange = BridgeProviderExchange::new(
            Value::from("ok"),
            vec![
                BridgeProviderEvent::new("event.progress", serde_json::json!({"current": 1})),
                BridgeProviderEvent::new("event.log", serde_json::json!({"message": "done"})),
            ],
        );
        assert_eq!(exchange.events.len(), 2);
        assert_eq!(exchange.events[0].method, "event.progress");
        assert_eq!(exchange.events[1].method, "event.log");
        assert_eq!(exchange.events[0].params["current"], 1);
    }

    #[tokio::test]
    async fn slot_is_optional_and_injectable() {
        let slot = BridgeSlot::default();
        assert!(!slot.is_attached());
        assert!(slot.get().is_none());
        slot.attach(Arc::new(Stub));
        assert!(slot.is_attached());
        let provider = slot.get().unwrap();
        let exchange = provider.call("config.loadLan", Value::Null).await.unwrap();
        assert_eq!(exchange.result["method"], "config.loadLan");
        assert!(exchange.events.is_empty());
    }

    #[test]
    fn capability_error_is_explicit() {
        assert!(capability_not_linked("lania-exec").to_string().contains("lania-exec"));
    }
}
