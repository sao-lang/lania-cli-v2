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
//! 真实业务端点以 hub 模式 spawn 自 Node bridge 子进程（`hub_mode_command` →
//! `BridgeHost::attach_command` → stdio Channel），announce 完整业务 method，
//! 由 Hub 精确路由。其余语言端点（python/go/dart/rust）仍以进程内 channel
//! 做 best-effort 登记，待后续批次按同样方式接入。持有子进程的 provider
//! 覆盖 `shutdown()`，供宿主在 `process::exit` 前显式清理。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use lania_bridge_bus::rust_adaptor::{EchoHandler, PROVIDES};
use lania_bridge_bus::BusFrame;
use lania_host_core::{
    BridgeLaunchSpec, BridgeProvider, BridgeProviderEvent, BridgeProviderExchange,
};
use serde_json::Value;

use crate::{BridgeHost, Endpoint};

/// 真实 Node bridge 端点 id：attach 名与子进程 LANIA_ENDPOINT_ID 必须一致，
/// 否则 Hub 的 announce 来源校验会判为坏帧（hub.rs:227-230）。
const NODE_ENDPOINT_ID: &str = "node-hub";

/// 宿主反向能力的进程内端点 id（声明 `host.*`）。
const HOST_RPC_ENDPOINT_ID: &str = "host-rpc";

/// 多语言业务端点 id：各语言的 endpoint.* 已从 echo 换成真实 lint/build 业务，
/// 以 Hub 端点身份接入，method 由 Hub 精确路由。
const PY_ENDPOINT_ID: &str = "py-hub";
const GO_ENDPOINT_ID: &str = "go-hub";
const DART_ENDPOINT_ID: &str = "dart-hub";
const RS_ENDPOINT_ID: &str = "rs-hub";

/// 等待真实 Node 端点完成 announce 的超时。stdio 子进程 + tsx 加载通常 <5s，
/// 30s 留足安装态与慢盘余量；超时即报错，不降级。
const ANNOUNCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 单次桥接调用的真实截止时长，与宿主 `policy.timeout_ms` 默认值（30s）对齐。
/// 慢业务（lint/build/dev）可能跑数秒，不能用固定轮数近似。
const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 把 Hub 的 `host.*` request 转接到注入的 HostRpcHandler。
///
/// Node 经 Hub 反向调用宿主 fs/git/exec 时，请求以 capability=`host.fs.read`
/// 的 BusFrame request 进来，这里调用 1:1 链路同一份 handler，保持语义一致。
struct HostRpcHandlerBridge(crate::node_bridge::HostRpcHandlerRef);

impl lania_bridge_bus::Handler for HostRpcHandlerBridge {
    fn on_request(
        &mut self,
        capability: &str,
        params: Value,
    ) -> Result<Value, lania_bridge_protocol::BusErrorDetail> {
        match self.0.handle(capability, params) {
            Ok((result, _events)) => Ok(result),
            Err(err) => Err(lania_bridge_core::bus_error(
                lania_bridge_protocol::E_INTERNAL,
                err.to_string(),
            )),
        }
    }

    fn on_event(&mut self, _topic: &str, _payload: Value, _from: &str) {}
}

/// 一个已 attach 的语言端点 + 它的业务回调。
///
/// `Endpoint` 与 `EchoHandler` 都是 `Send`，因此 `ProviderState` 仍是 `Send`，
/// `Mutex<ProviderState>` 保持 `Sync`（`BridgeProvider: Send + Sync` 的要求）。
struct ProviderSlot {
    endpoint: Endpoint,
    handler: Box<dyn lania_bridge_bus::Handler + Send>,
}

struct ProviderState {
    host: BridgeHost,
    client: Endpoint,
    languages: BTreeMap<String, BridgeLaunchSpec>,
    /// 已尝试惰性 attach 的语言端点 id（避免每个请求重复 spawn/编译）。
    attached_languages: BTreeSet<String>,
    providers: Vec<ProviderSlot>,
}

/// 进程内嵌的桥接 provider。所有状态在一个 `Mutex` 内，pump 是同步循环。
pub struct HubBridgeProvider {
    state: Mutex<ProviderState>,
}

impl HubBridgeProvider {
    /// 无 host RPC handler 的构造（仅测试 / 不需要反向调用的场景）。
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_handler(None)
    }

    /// 带 host RPC handler 的构造：让 Node 经 Hub 反向调用宿主 fs/git/exec。
    pub fn new_with_handler(
        host_rpc_handler: Option<crate::node_bridge::HostRpcHandlerRef>,
    ) -> anyhow::Result<Self> {
        let mut host = BridgeHost::new()?;
        // "host" 是发起请求的端点（§5.5 E.1）。它是通过 attach/attach_channel 真正
        // 接入 Hub 的进程内端点；语言端点由 register_language 按需追加。
        let mut client = host.attach("host");
        // Hub 单轨化：host 端点必须订阅事件主题，否则语言端点 publish 的 Event 帧
        // 不会被 Hub 广播到 client，call_sync 的事件收集分支将成为死代码。
        // topic 前缀通配与 Hub 的 topic_matches("event.*", ...) 对齐。
        client.subscribe("event.*");

        // 进程内业务 handler 槽（host.* 反向能力 + register_language 的 echo 端点）。
        let mut state_providers: Vec<ProviderSlot> = Vec::new();

        // 宿主反向能力端点：Node 经 Hub 发 `host.*` request 时，由注入的
        // HostRpcHandler 调用宿主 fs/git/exec（与 1:1 链路同一份实现）。
        // 用通配 `host.*` 声明：method 开放集合无法逐一 announce。
        if let Some(handler) = host_rpc_handler {
            let mut rpc_endpoint = host.attach(HOST_RPC_ENDPOINT_ID);
            rpc_endpoint.announce_str(&["host.*"], &[]);
            state_providers.push(ProviderSlot {
                endpoint: rpc_endpoint,
                handler: Box::new(HostRpcHandlerBridge(handler)),
            });
        }

        // Hub 100% 承接：以 hub 模式 spawn 真实 Node bridge 子进程，
        // 它 announce 完整业务 method（compiler.dev / lint.run / config.loadLan …），
        // 由 Hub 精确路由。这是承接真实业务的核心端点，而非 echo 占位。
        let node_command = crate::node_bridge::hub_mode_command(NODE_ENDPOINT_ID)?;
        host.attach_command(NODE_ENDPOINT_ID, node_command)?;

        // pump 直到真实 Node 端点完成 announce（stdio 子进程启动 + tsx 加载需要时间）。
        // 用「该端点已 announce」而非 has_any_provider：后者会被上面 host.* 端点
        // 立即满足，导致还没等真实 Node 就绪就返回。
        let deadline = std::time::Instant::now() + ANNOUNCE_TIMEOUT;
        while !host.hub().is_announced(NODE_ENDPOINT_ID) && std::time::Instant::now() < deadline {
            host.pump_until_idle(50);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // 失败即报错，不静默返回空 Hub 让上层兜底（用户硬要求：不做兼容、不打补丁、不兜底）。
        anyhow::ensure!(
            host.hub().is_announced(NODE_ENDPOINT_ID),
            "E_BRIDGE_ANNOUNCE_TIMEOUT: real node bridge did not announce within {}s",
            ANNOUNCE_TIMEOUT.as_secs()
        );

        // 多语言真实端点：Python 的 endpoint.py 已换成真实 lint/build 业务，
        // 以 Hub 端点身份接入。best-effort：没装 Python 时仅缺 lint.python 能力，
        // 不影响 Node 主路径（这不是“失败回落”，是可选能力提供者）。
        if host.attach_python_hub(PY_ENDPOINT_ID).is_ok() {
            let deadline = std::time::Instant::now() + ANNOUNCE_TIMEOUT;
            while !host.hub().is_announced(PY_ENDPOINT_ID) && std::time::Instant::now() < deadline {
                host.pump_until_idle(50);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        // Go / Dart 端点按需 attach：ensure_binary 首次编译 Go bridge 可能耗时数十秒，
        // 不能让每个命令（包括纯 Node 命令）都付这个成本。见 ensure_language_endpoint。

        Ok(Self {
            state: Mutex::new(ProviderState {
                host,
                client,
                languages: BTreeMap::new(),
                attached_languages: BTreeSet::new(),
                providers: state_providers,
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

    fn call_sync(&self, method: &str, params: Value) -> anyhow::Result<BridgeProviderExchange> {
        let mut events: Vec<BridgeProviderEvent> = Vec::new();
        let result = self.call_sync_stream(method, params, &mut |event| events.push(event))?;
        Ok(BridgeProviderExchange::new(result, events))
    }

    /// 流式版本：事件到达即回调，不等 response（供 agent.prompt 的逐 token 渲染）。
    fn call_sync_stream(
        &self,
        method: &str,
        params: Value,
        on_event: &mut dyn FnMut(BridgeProviderEvent),
    ) -> anyhow::Result<Value> {
        let mut state = self.lock()?;
        let state = &mut *state;
        // 惰性 attach：Go/Dart 端点首次被对应 capability 命中时才 spawn
        // （ensure_binary 首次编译代价只在真正需要时付出）。
        ensure_language_endpoint(state, method);
        let id = state.client.request(method, params);
        // Hub 单轨化：端点 publish 的事件帧实时交给回调（流式）或由外层收集。
        // 用真实 deadline 而非固定轮数：lint/build/dev 等秒级方法需要秒级等待，
        // 固定 200 轮（≈200ms）会误杀并在上层触发 1:1 重复执行。
        let deadline = std::time::Instant::now() + CALL_TIMEOUT;
        while std::time::Instant::now() < deadline {
            // 1) 把 host 的请求帧路由到声明该 capability 的端点；
            //    无 provider 时 Hub 直接把 E_METHOD_NOT_FOUND 响应送回 host 端点。
            state.host.pump_until_idle(50);
            // 2) 驱动已 attach 的语言端点：处理 Request 帧并回 Response 帧。
            for slot in state.providers.iter_mut() {
                slot.endpoint.serve_until_idle(&mut *slot.handler, 16);
            }
            // 3) 把端点的响应/事件帧路由回 host 端点。
            state.host.pump_until_idle(50);
            let mut outcome = None;
            while let Some(frame) = state.client.try_recv() {
                match frame {
                    BusFrame::Response {
                        id: response_id,
                        result,
                        error,
                    } => {
                        if response_id == id {
                            outcome = Some((result, error));
                        }
                    }
                    // 事件帧：topic 即稳定事件名，payload 为载荷。
                    BusFrame::Event { topic, payload, .. } => {
                        on_event(BridgeProviderEvent::new(topic, payload));
                    }
                    _ => {}
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
        anyhow::bail!(
            "E_TIMEOUT: no response for capability {method} within {}s",
            CALL_TIMEOUT.as_secs()
        )
    }
}

/// 惰性 attach 多语言端点：根据 capability 前缀判断所需语言，首次命中时 spawn。
/// Go 的 ensure_binary 首次编译代价只在此刻付出，纯 Node 命令完全不受影响。
fn ensure_language_endpoint(state: &mut ProviderState, method: &str) {
    let (id, attach) = if method.starts_with("lint.go-")
        || method.starts_with("lint.golangci")
        || method == "build.go"
    {
        (GO_ENDPOINT_ID, BridgeHost::attach_go as fn(&mut BridgeHost, &str) -> std::io::Result<()>)
    } else if method.starts_with("lint.dart-") || method == "build.dart" {
        (DART_ENDPOINT_ID, BridgeHost::attach_dart as fn(&mut BridgeHost, &str) -> std::io::Result<()>)
    } else if method == "lint.clippy" || method == "lint.rust" || method == "build.rust" {
        (RS_ENDPOINT_ID, BridgeHost::attach_rust_hub as fn(&mut BridgeHost, &str) -> std::io::Result<()>)
    } else {
        return; // Python/Node 端点已在构造时 attach。
    };
    if state.attached_languages.contains(id) {
        return;
    }
    // 标记已尝试（即使失败也不每个请求重试，避免反复编译/启动）。
    state.attached_languages.insert(id.to_string());
    if attach(&mut state.host, id).is_err() {
        return; // 能力不可用；call_sync 会拿到 E_METHOD_NOT_FOUND，由上层处理。
    }
    // 等 announce 完成，否则 request 会因 capability 未注册而失败。
    let deadline = std::time::Instant::now() + ANNOUNCE_TIMEOUT;
    while !state.host.hub().is_announced(id) && std::time::Instant::now() < deadline {
        state.host.pump_until_idle(50);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[async_trait]
impl BridgeProvider for HubBridgeProvider {
    async fn call(
        &self,
        method: &str,
        params: Value,
    ) -> anyhow::Result<BridgeProviderExchange> {
        // 同步方法不跨 await 持有 MutexGuard，保证 async_trait 生成的 future 是 Send。
        self.call_sync(method, params)
    }

    async fn call_stream(
        &self,
        method: &str,
        params: Value,
        on_event: &mut (dyn FnMut(BridgeProviderEvent) + Send),
    ) -> anyhow::Result<Value> {
        self.call_sync_stream(method, params, on_event)
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
            handler: Box::new(EchoHandler::default()),
        });
        // pump 一轮把 Announce 登记进 Hub 的 capability_index，之后 call_sync 才能路由。
        state.host.pump_until_idle(50);
        Ok(())
    }

    fn shutdown(&self) {
        // 显式 kill Hub 持有的真实 Node 子进程：总包 main 走 process::exit 会跳过
        // BridgeHost 的 Drop，必须靠宿主 shutdown_async 主动清理，避免孤儿进程。
        if let Ok(mut state) = self.state.lock() {
            state.host.shutdown();
        }
    }

    fn drain_events(&self) -> Vec<BridgeProviderEvent> {
        // 长任务（dev/watch）的 response 先返回，运行期事件随后经 Hub 到达。
        // pump 一轮并排空 host 端点收到的 Event 帧，等价于 1:1 的 next_event。
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        state.host.pump_until_idle(50);
        let mut events = Vec::new();
        while let Some(frame) = state.client.try_recv() {
            if let BusFrame::Event { topic, payload, .. } = frame {
                events.push(BridgeProviderEvent::new(topic, payload));
            }
        }
        events
    }
}

/// 命令二进制的装配入口（§5.5 E.1）：`host.attach_bridge(load_bridge_provider())`。
/// env / config 等纯 Rust 命令不调用它，`host.bridge` 保持未注入。
pub fn load_bridge_provider(
    host_rpc_handler: Option<crate::node_bridge::HostRpcHandlerRef>,
) -> anyhow::Result<Arc<dyn BridgeProvider>> {
    Ok(Arc::new(HubBridgeProvider::new_with_handler(host_rpc_handler)?))
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

    /// 不 spawn 真实 Node 的测试专用构造：用于验证 call_sync 的等待语义，
    /// 避免把慢方法测试与本机 Node/tsx  availability 绑定。
    fn provider_without_node() -> HubBridgeProvider {
        let mut host = BridgeHost::new().unwrap();
        let mut client = host.attach("host");
        client.subscribe("event.*");
        host.pump_until_idle(50);
        HubBridgeProvider {
            state: Mutex::new(ProviderState {
                host,
                client,
                languages: BTreeMap::new(),
                attached_languages: BTreeSet::new(),
                providers: Vec::new(),
            }),
        }
    }

    /// 端到端：注入 handler 的 provider 收到 host.* request（经 Hub 通配路由），
    /// 应转接到 handler 并把结果作为 response 回到 client。
    #[test]
    fn host_rpc_request_roundtrips_through_hub() {
        let mock = Arc::new(RecordingHostRpc { calls: Mutex::new(Vec::new()) });
        let provider = HubBridgeProvider::new_with_handler(Some(mock.clone()))
            .expect("spawn provider with host rpc handler");
        let mut state = provider.state.lock().unwrap();
        let _id = state.client.request(
            "host.fs.read",
            serde_json::json!({"path": "a.txt"}),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            state.host.pump_until_idle(50);
            for slot in state.providers.iter_mut() {
                slot.endpoint.serve_until_idle(&mut *slot.handler, 16);
            }
            state.host.pump_until_idle(50);
            let frames: Vec<_> = std::iter::from_fn(|| state.client.try_recv()).collect();
            if let Some(BusFrame::Response { result, error, .. }) =
                frames.into_iter().find(|f| matches!(f, BusFrame::Response { .. }))
            {
                assert!(error.is_none(), "unexpected error: {error:?}");
                assert_eq!(result.unwrap()["method"], "host.fs.read");
                break;
            }
            assert!(std::time::Instant::now() < deadline, "host rpc roundtrip timed out");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(mock.calls.lock().unwrap()[0].0, "host.fs.read");
    }

    /// 记录调用的 host RPC mock，验证 Hub request 被转接到注入的 handler。
    struct RecordingHostRpc {
        calls: Mutex<Vec<(String, Value)>>,
    }

    impl crate::node_bridge::HostRpcHandler for RecordingHostRpc {
        fn handle(
            &self,
            method: &str,
            params: Value,
        ) -> anyhow::Result<(Value, Vec<crate::node_bridge::BridgeEvent>)> {
            self.calls.lock().unwrap().push((method.to_string(), params.clone()));
            Ok((serde_json::json!({"ok": true, "method": method}), Vec::new()))
        }
    }

    #[test]
    fn host_rpc_bridge_forwards_request_to_handler() {
        use lania_bridge_bus::Handler;

        let mock = Arc::new(RecordingHostRpc { calls: Mutex::new(Vec::new()) });
        let mut bridge = HostRpcHandlerBridge(mock.clone());
        let result = bridge
            .on_request("host.fs.read", serde_json::json!({"path": "a.txt"}))
            .expect("handler should succeed");
        assert_eq!(result["ok"], true);
        assert_eq!(result["method"], "host.fs.read");
        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "host.fs.read");
    }

    /// 延迟 400ms 才响应的 handler：旧实现 200 轮 ×1ms（≈200ms）会误杀，
    /// deadline 实现（30s）必须能等到。这是 B2 的可证伪回归测试。
    struct SlowHandler;

    impl lania_bridge_bus::Handler for SlowHandler {
        fn on_request(
            &mut self,
            capability: &str,
            _params: Value,
        ) -> Result<Value, lania_bridge_protocol::BusErrorDetail> {
            std::thread::sleep(std::time::Duration::from_millis(400));
            Ok(serde_json::json!({ "capability": capability, "slow": true }))
        }

        fn on_event(&mut self, _topic: &str, _payload: Value, _from: &str) {}
    }

    /// 端到端：真实 Node bridge 处理 system.listCommands 时 emit 的 event.log
    /// 必须经 Hub 广播到 host 端点，出现在 call_sync 返回的 events 中。
    /// 这验证「插件事件 → publish → Hub → host 订阅 → exchange.events」全链路。
    #[test]
    fn real_node_events_reach_host_exchange() {
        let provider = HubBridgeProvider::new().expect("spawn real node bridge");
        let exchange = provider
            .call_sync("system.listCommands", serde_json::json!({}))
            .expect("system.listCommands should succeed");
        assert!(
            exchange.events.iter().any(|e| e.method == "event.log"),
            "真实 Node 端点 emit 的 event.log 必须经 Hub 到达 host exchange，实际 events={:?}",
            exchange.events
        );
    }

    /// 流式调用：真实 Node 端点 emit 的 event.log 必须在 call_sync_stream 的
    /// 回调中实时到达（agent.prompt 逐 token 渲染依赖此路径）。
    #[test]
    fn call_stream_delivers_events_via_callback() {
        let provider = HubBridgeProvider::new().expect("spawn real node bridge");
        let mut seen: Vec<String> = Vec::new();
        let result = provider
            .call_sync_stream("system.listCommands", serde_json::json!({}), &mut |event| {
                seen.push(event.method)
            })
            .expect("stream call should succeed");
        assert!(!result.is_null(), "stream call must return result");
        assert!(
            seen.iter().any(|method| method == "event.log"),
            "流式回调必须收到 event.log，实际={seen:?}"
        );
    }

    /// B2 回归：慢方法（>200ms）必须能拿到响应，旧固定 200 轮会误杀。
    #[test]
    fn call_sync_waits_for_slow_provider_beyond_fixed_rounds() {
        let provider = provider_without_node();
        {
            let mut state = provider.state.lock().unwrap();
            let mut endpoint = state.host.attach("slow-endpoint");
            endpoint.announce_str(&["slow.echo"], &[]);
            state.providers.push(ProviderSlot {
                endpoint,
                handler: Box::new(SlowHandler),
            });
            state.host.pump_until_idle(50);
        }
        let start = std::time::Instant::now();
        let exchange = provider
            .call_sync("slow.echo", serde_json::json!({}))
            .expect("slow provider must not be killed by a 200ms ceiling");
        assert_eq!(exchange.result["capability"], "slow.echo");
        assert_eq!(exchange.result["slow"], true);
        assert!(
            start.elapsed() >= std::time::Duration::from_millis(400),
            "slow handler should actually have delayed the response"
        );
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

    /// B1 回归：host 端点订阅 event.* 后，端点 publish 的事件能被 Hub 广播到
    /// client，从而让 call_sync 的事件收集分支可达（不再恒空）。
    #[test]
    fn host_receives_published_events_via_subscription() {
        let provider = HubBridgeProvider::new().unwrap();
        let mut state = provider.state.lock().unwrap();
        // attach 一个测试端点并显式订阅/发布（模拟语言端点 publish 事件）。
        let mut emitter = state.host.attach("emitter");
        emitter.announce_str(&[], &[]);
        emitter.publish("event.progress", serde_json::json!({"current": 1}));
        state.host.pump_until_idle(50);
        // host 端点已订阅 event.*，应收到 Event 帧。
        let received = state.client.try_recv();
        assert!(
            matches!(received, Some(BusFrame::Event { .. })),
            "host 端点必须能收到 publish 的 Event 帧，否则事件收集是死代码"
        );
    }

    /// toolchain.describe 经 Hub 路由到 Node 端点（不再走 1:1 NodeBridgeClient）。
    #[test]
    fn toolchain_describe_routes_through_hub() {
        let provider = HubBridgeProvider::new().expect("spawn hub");
        let exchange = provider
            .call_sync("toolchain.describe", serde_json::json!({}))
            .expect("toolchain.describe must be routable via hub");
        let runtimes = exchange.result["runtimes"]
            .as_array()
            .expect("toolchain.describe should return runtimes array");
        assert!(!runtimes.is_empty(), "runtimes should not be empty");
    }

    /// 多语言单轨：真实 Python 端点（endpoint.py 复用 lan_py_bridge 业务）以 Hub
    /// 端点身份接入，lint.mypy 经 Hub 精确路由并返回结构化诊断（而非 E_METHOD_NOT_FOUND）。
    #[test]
    fn python_lint_capability_routes_through_hub() {
        let provider = HubBridgeProvider::new().expect("spawn hub with python endpoint");
        // 用专用空目录 + flake8（未安装时快速 fallback），避免 mypy 扫描整个 temp 目录。
        let cwd = std::env::temp_dir().join("lania-py-hub-test");
        std::fs::create_dir_all(&cwd).ok();
        let exchange = provider
            .call_sync("lint.flake8", serde_json::json!({ "cwd": cwd.display().to_string() }))
            .expect("lint.flake8 must be routable via hub");
        // 业务返回统一诊断形状（adaptor 字段），无论 flake8 是否安装。
        assert_eq!(exchange.result["adaptor"], serde_json::json!("flake8"));
    }

    /// 阶段 8 接线：register_language 之后 Hub 不再是空的 —— 端点声明的
    /// capability 能被 call_sync 路由到并拿到响应（而不是恒回 E_METHOD_NOT_FOUND）。
    #[test]
    fn declared_capability_is_routable_after_language_registration() {
        let provider = HubBridgeProvider::new().unwrap();
        provider
            .register_language("python", spec())
            .unwrap();
        let exchange = provider
            .call_sync("echo.ping", serde_json::json!({ "text": "hi" }))
            .expect("declared capability is routable");
        assert_eq!(exchange.result["pong"], serde_json::json!(true));
        assert_eq!(exchange.result["endpointId"], serde_json::json!("rust-endpoint"));
        assert!(exchange.events.is_empty());
    }
}
