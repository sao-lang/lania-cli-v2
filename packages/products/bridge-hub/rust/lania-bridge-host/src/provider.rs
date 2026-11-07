//! `BridgeProvider` 的 bridge-host 实现（文档 §5.1 A.5(3) / §5.5 E.1）。
//!
//! `load_bridge_provider()` 返回一个进程内嵌 Hub 的 provider：`call` 只按 capability
//! 发帧，由 Hub 路由到声明该能力的端点，帧里没有语言字段。
//!
//! 阶段 8 接线前这里只是「往 BTreeMap 记表」：Hub 里除了 `host` 请求端点之外
//! 没有任何 provider，`call_sync` 恒回 `E_METHOD_NOT_FOUND`。现在 `register_language`
//! 会真正 `attach_channel` 一个语言端点并 `announce` 能力，`call_sync` 在 pump 之间
//! 驱动这些端点处理请求帧，从而让「声明 capability → 路由 → 响应」闭环成立。
//!
//! 端点走**进程内 channel**（`BridgeHost::attach` → `BridgeHub::attach_channel`），
//! 而不是 `attach_node/attach_python/...` 的 stdio 子进程：注册发生在每个命令二进制
//! 的装配阶段，若在那里 spawn 解释器会让「装了更好、不装也能用」的命令额外拉起
//! 多个进程，也会让单元测试依赖本机工具链。`BridgeHost` 仍保留 stdio 装配函数，
//! 供将来真正需要独立语言端点时按需调用。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use lania_bridge_bus::rust_adaptor::{EchoHandler, PROVIDES};
use lania_bridge_bus::BusFrame;
use lania_host_core::{BridgeLaunchSpec, BridgeProvider};
use serde_json::Value;

use crate::{BridgeHost, Endpoint};

/// 一个已 attach 的语言端点 + 它的业务回调。
///
/// `Endpoint` 与 `EchoHandler` 都是 `Send`，因此 `ProviderState` 仍是 `Send`，
/// `Mutex<ProviderState>` 保持 `Sync`（`BridgeProvider: Send + Sync` 的要求）。
struct ProviderSlot {
    endpoint: Endpoint,
    handler: EchoHandler,
}

struct ProviderState {
    host: BridgeHost,
    client: Endpoint,
    languages: BTreeMap<String, BridgeLaunchSpec>,
    providers: Vec<ProviderSlot>,
}

/// 进程内嵌的桥接 provider。所有状态在一个 `Mutex` 内，pump 是同步循环。
pub struct HubBridgeProvider {
    state: Mutex<ProviderState>,
}

impl HubBridgeProvider {
    pub fn new() -> anyhow::Result<Self> {
        let mut host = BridgeHost::new()?;
        // "host" 是发起请求的端点（§5.5 E.1）。它是通过 attach/attach_channel 真正
        // 接入 Hub 的进程内端点；语言端点由 register_language 按需追加。
        // 此时 Hub 里没有任何 capability provider —— 任何 call 都回 E_METHOD_NOT_FOUND。
        let client = host.attach("host");
        Ok(Self {
            state: Mutex::new(ProviderState {
                host,
                client,
                languages: BTreeMap::new(),
                providers: Vec::new(),
            }),
        })
    }

    /// 已声明的语言 id（opaque 字符串，不做语言枚举）。
    pub fn registered_languages(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("bridge provider poisoned")
            .languages
            .keys()
            .cloned()
            .collect()
    }

    fn lock(&self) -> anyhow::Result<std::sync::MutexGuard<'_, ProviderState>> {
        self.state.lock().map_err(|err| anyhow::anyhow!("{err}"))
    }

    fn call_sync(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let mut state = self.lock()?;
        let state = &mut *state;
        let id = state.client.request(method, params);
        for _ in 0..200 {
            // 1) 把 host 的请求帧路由到声明该 capability 的端点；
            //    无 provider 时 Hub 直接把 E_METHOD_NOT_FOUND 响应送回 host 端点。
            state.host.pump_until_idle(50);
            // 2) 驱动已 attach 的语言端点：处理 Request 帧并回 Response 帧。
            for slot in state.providers.iter_mut() {
                slot.endpoint.serve_until_idle(&mut slot.handler, 16);
            }
            // 3) 把端点的响应帧路由回 host 端点。
            state.host.pump_until_idle(50);
            let mut outcome = None;
            while let Some(frame) = state.client.try_recv() {
                if let BusFrame::Response {
                    id: response_id,
                    result,
                    error,
                } = frame
                {
                    if response_id == id {
                        outcome = Some((result, error));
                        break;
                    }
                }
            }
            if let Some((result, error)) = outcome {
                if let Some(detail) = error {
                    anyhow::bail!("{}: {}", detail.code, detail.message);
                }
                return Ok(result.unwrap_or(Value::Null));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        anyhow::bail!("E_TIMEOUT: no response for capability {method}")
    }
}

#[async_trait]
impl BridgeProvider for HubBridgeProvider {
    async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        // 同步方法不跨 await 持有 MutexGuard，保证 async_trait 生成的 future 是 Send。
        self.call_sync(method, params)
    }

    fn register_language(&self, id: &str, spec: BridgeLaunchSpec) -> anyhow::Result<()> {
        let mut state = self.lock()?;
        // 仍保留「opaque 记表」语义（host-core 不引用语言枚举）。
        let is_new = state.languages.insert(id.to_string(), spec).is_none();
        if !is_new {
            // 同一语言重复登记：只更新 opaque spec，不重复 attach 端点，
            // 否则会在 providers 里留下断开的旧端点。
            return Ok(());
        }

        // 真正 attach 一个语言端点并声明 capability —— 让 Hub 不再是空的。
        // 这里复用生成骨架的进程内 channel（attach_channel），不 spawn 子进程。
        let mut endpoint = state.host.attach(format!("lang-{id}"));
        endpoint.announce_str(PROVIDES, &[]);
        state.providers.push(ProviderSlot {
            endpoint,
            handler: EchoHandler::default(),
        });
        // pump 一轮把 Announce 登记进 Hub 的 capability_index，之后 call_sync 才能路由。
        state.host.pump_until_idle(50);
        Ok(())
    }
}

/// 命令二进制的装配入口（§5.5 E.1）：`host.attach_bridge(load_bridge_provider())`。
/// env / config 等纯 Rust 命令不调用它，`host.bridge` 保持未注入。
pub fn load_bridge_provider() -> anyhow::Result<Arc<dyn BridgeProvider>> {
    Ok(Arc::new(HubBridgeProvider::new()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> BridgeLaunchSpec {
        BridgeLaunchSpec {
            program: "python".into(),
            args: vec![],
            dir: None,
        }
    }

    #[test]
    fn provider_routes_without_naming_a_language() {
        let provider = HubBridgeProvider::new().unwrap();
        // 无 provider 时 Hub 直接回 E_METHOD_NOT_FOUND（bus 行为不变）。
        let result = provider.call_sync("missing.cap", Value::Null);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("E_METHOD_NOT_FOUND"));
    }

    #[test]
    fn language_registration_is_opaque() {
        let provider = HubBridgeProvider::new().unwrap();
        provider
            .register_language("python", spec())
            .unwrap();
        assert_eq!(provider.registered_languages(), vec!["python"]);
    }

    /// 阶段 8 接线：register_language 之后 Hub 不再是空的 —— 端点声明的
    /// capability 能被 call_sync 路由到并拿到响应（而不是恒回 E_METHOD_NOT_FOUND）。
    #[test]
    fn declared_capability_is_routable_after_language_registration() {
        let provider = HubBridgeProvider::new().unwrap();
        provider
            .register_language("python", spec())
            .unwrap();
        let result = provider
            .call_sync("echo.ping", serde_json::json!({ "text": "hi" }))
            .expect("declared capability is routable");
        assert_eq!(result["pong"], serde_json::json!(true));
        assert_eq!(result["endpointId"], serde_json::json!("rust-endpoint"));
    }
}
