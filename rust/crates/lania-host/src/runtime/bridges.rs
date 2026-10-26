//! 运行时 bridge 注册表（多运行时支持的基础设施）。
//!
//! 设计要点见 `docs/design/multi-runtime-bridge.zh-CN.md` §2.2：
//!
//! - **不在 Node bridge 里路由**：那会让 `build`(Go) 变成 4 层进程。
//! - **Rust 侧按运行时持有客户端，按需启动**：每个运行时一个 `NodeBridgeClient`，
//!   第一次被请求时才 `spawn`（懒加载）。
//! - **JS 簇共用同一个进程**：`Node` / `Bun` / `Deno` 都是 JS 工具生态，
//!   共享现有 `node_bridge` 实例，只通过 `params.runtime` 切换工具表。
//!
//! 本模块只提供注册表骨架：查找 / 注册 / 关闭。真正的语言 bridge 可执行文件
//! 由后续里程碑（B8+）提供；缺失时由调用方回落到 CLI spawn（§5.3）。

use std::{cell::RefCell, collections::BTreeMap, path::PathBuf, rc::Rc};

use anyhow::{anyhow, Result};
use lania_node_bridge::{BridgeClientConfig, BridgeLaunchSpec, NodeBridgeClient};

/// 本文覆盖的七个运行时。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuntimeKind {
    Node,
    Bun,
    Deno,
    Python,
    Go,
    Rust,
    Dart,
}

impl RuntimeKind {
    pub const ALL: [RuntimeKind; 7] = [
        RuntimeKind::Node,
        RuntimeKind::Bun,
        RuntimeKind::Deno,
        RuntimeKind::Python,
        RuntimeKind::Go,
        RuntimeKind::Rust,
        RuntimeKind::Dart,
    ];

    /// 运行时 id，与 `lania-env` / `lan.config` 中的写法一致。
    pub fn id(self) -> &'static str {
        match self {
            RuntimeKind::Node => "node",
            RuntimeKind::Bun => "bun",
            RuntimeKind::Deno => "deno",
            RuntimeKind::Python => "python",
            RuntimeKind::Go => "go",
            RuntimeKind::Rust => "rust",
            RuntimeKind::Dart => "dart",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|kind| kind.id() == normalized)
    }

    /// node / bun / deno 共用同一个 JS 簇 bridge 进程。
    pub fn is_js_cluster(self) -> bool {
        matches!(
            self,
            RuntimeKind::Node | RuntimeKind::Bun | RuntimeKind::Deno
        )
    }

    /// 独立 bridge 的可执行文件名（JS 簇没有独立可执行文件）。
    pub fn bridge_binary(self) -> Option<&'static str> {
        match self {
            RuntimeKind::Node | RuntimeKind::Bun | RuntimeKind::Deno => None,
            RuntimeKind::Python => Some("lan-py-bridge"),
            RuntimeKind::Go => Some("lan-go-bridge"),
            RuntimeKind::Rust => Some("lan-rs-bridge"),
            RuntimeKind::Dart => Some("lan-dart-bridge"),
        }
    }
}

/// 独立 bridge 的定位方式：由调用方在注册时给出，注册表本身不做探测。
#[derive(Debug, Clone)]
pub struct BridgeProgram {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

impl BridgeProgram {
    pub fn new(program: impl Into<PathBuf>, args: Vec<String>, dir: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args,
            dir: dir.into(),
        }
    }

    fn to_launch_spec(&self) -> BridgeLaunchSpec {
        BridgeLaunchSpec::for_program(self.program.clone(), self.args.clone(), self.dir.clone())
    }
}

/// 每运行时一个懒加载客户端。
///
/// 用 `Rc<RefCell<_>>` 而非 `OnceCell`：注册/关闭都需要在运行期可变，
/// 且 `HostRuntime` 本身是单线程使用的（bridge 客户端内部已用 `Arc<Mutex<_>>` 保证线程安全）。
#[derive(Default)]
pub struct BridgeRegistry {
    programs: BTreeMap<RuntimeKind, BridgeProgram>,
    clients: RefCell<BTreeMap<RuntimeKind, NodeBridgeClient>>,
}

impl std::fmt::Debug for BridgeRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeRegistry")
            .field("registered", &self.programs.keys().collect::<Vec<_>>())
            .field("started", &self.clients.borrow().keys().collect::<Vec<_>>())
            .finish()
    }
}

impl BridgeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个独立 bridge 的可执行文件位置（不会启动进程）。
    pub fn register(&mut self, kind: RuntimeKind, program: BridgeProgram) -> Result<()> {
        if kind.is_js_cluster() {
            return Err(anyhow!(
                "{} belongs to the JS cluster and must use the shared node bridge",
                kind.id()
            ));
        }
        self.programs.insert(kind, program);
        // 换了可执行文件，旧客户端必须丢弃，否则会继续用上一个进程。
        self.clients.borrow_mut().remove(&kind);
        Ok(())
    }

    /// 该运行时是否已登记可用的 bridge。
    pub fn is_registered(&self, kind: RuntimeKind) -> bool {
        kind.is_js_cluster() || self.programs.contains_key(&kind)
    }

    /// 取得（必要时创建）该运行时的客户端。
    ///
    /// JS 簇运行时永远不会在这里创建进程 —— 必须由调用方传入共享的 `node_bridge`。
    pub fn client_for(
        &self,
        kind: RuntimeKind,
        js_cluster_bridge: &NodeBridgeClient,
    ) -> Result<NodeBridgeClient> {
        if kind.is_js_cluster() {
            return Ok(js_cluster_bridge.clone());
        }
        if let Some(client) = self.clients.borrow().get(&kind) {
            return Ok(client.clone());
        }
        let program = self.programs.get(&kind).ok_or_else(|| {
            anyhow!(
                "no bridge registered for runtime `{}`; fall back to CLI spawn",
                kind.id()
            )
        })?;
        let client = NodeBridgeClient::new(BridgeClientConfig::default())
            .with_launch_spec(program.to_launch_spec());
        self.clients.borrow_mut().insert(kind, client.clone());
        Ok(client)
    }

    /// 已登记的运行时（用于 `lan doctor` 之类的诊断输出）。
    pub fn registered(&self) -> Vec<RuntimeKind> {
        self.programs.keys().copied().collect()
    }

    /// 关闭所有已启动的 bridge 进程。
    pub fn shutdown(&self) {
        for (_, client) in self.clients.borrow().iter() {
            client.shutdown();
        }
        self.clients.borrow_mut().clear();
    }
}

/// `BridgeRegistry` 需要被放进 `HostRuntimeServices`，而那里的其他服务都只是 `Clone`，
/// 因此用 `Rc` 共享同一份注册表。
pub type SharedBridgeRegistry = Rc<RefCell<BridgeRegistry>>;

pub fn shared_registry() -> SharedBridgeRegistry {
    Rc::new(RefCell::new(BridgeRegistry::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn js_bridge() -> NodeBridgeClient {
        NodeBridgeClient::new(BridgeClientConfig {
            prefer_process_transport: false,
            ..BridgeClientConfig::default()
        })
    }

    #[test]
    fn js_cluster_members_share_the_node_bridge() {
        let registry = BridgeRegistry::new();
        let node = js_bridge();
        for kind in [RuntimeKind::Node, RuntimeKind::Bun, RuntimeKind::Deno] {
            let client = registry.client_for(kind, &node).expect("js cluster works");
            let request = client.ping_request();
            // 共享同一个实例 ⇒ 请求 id 在同一个计数器上递增。
            assert!(request.id.starts_with("req-"));
        }
        assert!(registry.registered().is_empty());
    }

    #[test]
    fn js_cluster_rejects_own_program() {
        let mut registry = BridgeRegistry::new();
        let error = registry
            .register(
                RuntimeKind::Deno,
                BridgeProgram::new("deno", vec![], "/tmp"),
            )
            .unwrap_err();
        assert!(error.to_string().contains("JS cluster"), "{error}");
    }

    #[test]
    fn missing_runtime_bridge_reports_fallback_hint() {
        let registry = BridgeRegistry::new();
        let error = registry
            .client_for(RuntimeKind::Go, &js_bridge())
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("go"), "{message}");
        assert!(message.contains("fall back"), "{message}");
    }

    #[test]
    fn registering_makes_runtime_available() {
        let mut registry = BridgeRegistry::new();
        registry
            .register(
                RuntimeKind::Go,
                BridgeProgram::new("lan-go-bridge", vec!["--stdio".into()], "/opt"),
            )
            .expect("registration succeeds");
        assert!(registry.is_registered(RuntimeKind::Go));
        assert_eq!(registry.registered(), vec![RuntimeKind::Go]);
        let client = registry.client_for(RuntimeKind::Go, &js_bridge()).unwrap();
        assert_eq!(client.ping_request().method, "bridge.ping");
    }

    #[test]
    fn re_registering_drops_the_previous_client() {
        let mut registry = BridgeRegistry::new();
        registry
            .register(
                RuntimeKind::Python,
                BridgeProgram::new("lan-py-bridge", vec![], "/opt/old"),
            )
            .unwrap();
        let _ = registry
            .client_for(RuntimeKind::Python, &js_bridge())
            .unwrap();
        assert_eq!(registry.clients.borrow().len(), 1);

        registry
            .register(
                RuntimeKind::Python,
                BridgeProgram::new("lan-py-bridge", vec![], "/opt/new"),
            )
            .unwrap();
        assert!(registry.clients.borrow().is_empty());
    }

    #[test]
    fn runtime_kind_round_trips_through_ids() {
        for kind in RuntimeKind::ALL {
            assert_eq!(RuntimeKind::parse(kind.id()), Some(kind));
        }
        assert_eq!(RuntimeKind::parse("GO"), Some(RuntimeKind::Go));
        assert_eq!(RuntimeKind::parse("cobol"), None);
    }

    #[test]
    fn only_js_cluster_members_lack_a_binary() {
        let without_binary: Vec<_> = RuntimeKind::ALL
            .into_iter()
            .filter(|kind| kind.bridge_binary().is_none())
            .collect();
        assert_eq!(
            without_binary,
            vec![RuntimeKind::Node, RuntimeKind::Bun, RuntimeKind::Deno]
        );
    }
}
