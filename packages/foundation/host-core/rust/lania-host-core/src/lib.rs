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

/// 桥接层视图（host-core 定义，bridge-host 实现）。
///
/// `call` 只发「方法名 + JSON 参数」，与 `RuntimeKind` / 具体语言无关；
/// `register_language` 的语言 id 是字符串，host-core 不引用语言枚举。
#[async_trait]
pub trait BridgeProvider: Send + Sync + 'static {
    async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value>;

    fn register_language(&self, id: &str, spec: BridgeLaunchSpec) -> anyhow::Result<()>;
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
        async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
            Ok(Value::Object(
                [("method".to_string(), Value::from(method)), ("params".to_string(), params)]
                    .into_iter()
                    .collect(),
            ))
        }

        fn register_language(&self, _id: &str, _spec: BridgeLaunchSpec) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn slot_is_optional_and_injectable() {
        let slot = BridgeSlot::default();
        assert!(!slot.is_attached());
        assert!(slot.get().is_none());
        slot.attach(Arc::new(Stub));
        assert!(slot.is_attached());
        let provider = slot.get().unwrap();
        let value = provider.call("config.loadLan", Value::Null).await.unwrap();
        assert_eq!(value["method"], "config.loadLan");
    }

    #[test]
    fn capability_error_is_explicit() {
        assert!(capability_not_linked("lania-exec").to_string().contains("lania-exec"));
    }
}
