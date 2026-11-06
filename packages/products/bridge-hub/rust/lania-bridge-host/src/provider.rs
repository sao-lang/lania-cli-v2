//! `BridgeProvider` 的 bridge-host 实现（文档 §5.1 A.5(3) / §5.5 E.1）。
//!
//! `load_bridge_provider()` 返回一个进程内嵌 Hub 的 provider：`call` 只按 capability
//! 发帧，由 Hub 路由到声明该能力的端点，帧里没有语言字段。语言注册目前记录为
//! opaque `BridgeLaunchSpec` 表，供后续过渡阶段把 1:1 stdio 桥替换成 Hub 端点。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use lania_bridge_bus::BusFrame;
use lania_host_core::{BridgeLaunchSpec, BridgeProvider};
use serde_json::Value;

use crate::{BridgeHost, Endpoint};

struct ProviderState {
    host: BridgeHost,
    client: Endpoint,
    languages: BTreeMap<String, BridgeLaunchSpec>,
}

/// 进程内嵌的桥接 provider。所有状态在一个 `Mutex` 内，pump 是同步循环。
pub struct HubBridgeProvider {
    state: Mutex<ProviderState>,
}

impl HubBridgeProvider {
    pub fn new() -> anyhow::Result<Self> {
        let mut host = BridgeHost::new()?;
        let client = host.attach("host");
        Ok(Self {
            state: Mutex::new(ProviderState {
                host,
                client,
                languages: BTreeMap::new(),
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
        let id = state.client.request(method, params);
        for _ in 0..200 {
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
        self.lock()?.languages.insert(id.to_string(), spec);
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
            .register_language(
                "python",
                BridgeLaunchSpec {
                    program: "python".into(),
                    args: vec![],
                    dir: None,
                },
            )
            .unwrap();
        assert_eq!(provider.registered_languages(), vec!["python"]);
    }
}
