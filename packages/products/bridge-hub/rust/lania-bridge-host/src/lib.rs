//! Bridge Hub 的进程内装配层（bridge-hub 包的一部分）。
//!
//! 决策（`package-split-command-atomization.zh-CN.md` §5.5）：Hub 静态链接进命令进程，
//! Rust 端点走进程内 channel，TS/Py/Go/Dart 端点走 stdio 子进程。本 crate 负责
//! 创建 Hub、把内嵌的生成物与手写 adaptor 落盘、接入子进程端点、暴露诊断。
//! 宿主代码永远不 import 任何具体语言 bridge crate。

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub use lania_bridge_bus::{
    BridgeHub, BusFrame, Endpoint, Handler, HubDiagnostics, StdioChannel,
};
use lania_bridge_core::AssetStore;

macro_rules! embedded {
    ($path:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $path))
    };
}

const TS_PROTOCOL: &str = embedded!("protocol/gen/ts/protocol.ts");
const TS_RUNTIME: &str = embedded!("protocol/gen/ts/runtime.ts");
const PY_PROTOCOL: &str = embedded!("protocol/gen/py/lania_bridge_protocol.py");
const PY_RUNTIME: &str = embedded!("protocol/gen/py/lania_bridge_runtime.py");
const GO_PROTOCOL: &str = embedded!("protocol/gen/go/protocol.go");
const GO_RUNTIME: &str = embedded!("protocol/gen/go/runtime.go");
const DART_PROTOCOL: &str = embedded!("protocol/gen/dart/protocol.dart");
const DART_RUNTIME: &str = embedded!("protocol/gen/dart/runtime.dart");
const TS_ENDPOINT: &str = embedded!("adaptors/ts/endpoint.ts");
const PY_ENDPOINT: &str = embedded!("adaptors/py/endpoint.py");
/// 真实业务端点（echo 骨架 endpoint.py 保留给协议契约测试）。
const PY_HUB_ENDPOINT: &str = embedded!("adaptors/py/hub_endpoint.py");
const GO_ENDPOINT: &str = embedded!("adaptors/go/endpoint.go");
const DART_ENDPOINT: &str = embedded!("adaptors/dart/endpoint.dart");

const GO_MOD: &str = "module lania-bridge-endpoint\n\ngo 1.21\n";
const DART_PUBSPEC: &str = "name: lania_bridge_endpoint\nenvironment:\n  sdk: \">=3.0.0 <4.0.0\"\n";

/// 内嵌资产清单。生成物与手写 adaptor 都随 crate 编译进二进制，落盘时按仓库相对结构摆放，
/// 这样 `endpoint.ts` 的 `../../protocol/gen/ts/runtime.ts` 在运行时依旧成立。
fn embedded_assets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("protocol/gen/ts/protocol.ts", TS_PROTOCOL),
        ("protocol/gen/ts/runtime.ts", TS_RUNTIME),
        ("protocol/gen/py/lania_bridge_protocol.py", PY_PROTOCOL),
        ("protocol/gen/py/lania_bridge_runtime.py", PY_RUNTIME),
        ("protocol/gen/go/protocol.go", GO_PROTOCOL),
        ("protocol/gen/go/runtime.go", GO_RUNTIME),
        ("protocol/gen/dart/protocol.dart", DART_PROTOCOL),
        ("protocol/gen/dart/runtime.dart", DART_RUNTIME),
        ("adaptors/ts/endpoint.ts", TS_ENDPOINT),
        ("adaptors/py/endpoint.py", PY_ENDPOINT),
        ("adaptors/py/hub_endpoint.py", PY_HUB_ENDPOINT),
        ("adaptors/dart/endpoint.dart", DART_ENDPOINT),
        ("adaptors/go/endpoint.go", GO_ENDPOINT),
        // Go / Dart 需要与 endpoint 同目录才能按单包编译运行。
        ("adaptors/go/protocol.go", GO_PROTOCOL),
        ("adaptors/go/runtime.go", GO_RUNTIME),
        ("adaptors/go/go.mod", GO_MOD),
        ("adaptors/dart/protocol.dart", DART_PROTOCOL),
        ("adaptors/dart/runtime.dart", DART_RUNTIME),
        ("adaptors/dart/pubspec.yaml", DART_PUBSPEC),
        // 真实业务脚本：Hub 端点（endpoint.*）复用它们的 lint/build 实现。
        // 与 1:1 bridge 共用同一份内容，避免「协议知识写两遍」。
        ("adaptors/py/lan_py_bridge.py", lania_py_bridge::BRIDGE_SCRIPT),
        ("adaptors/go/lan_go_bridge.go", lania_go_bridge::BRIDGE_SOURCE),
        ("adaptors/dart/lan_dart_bridge.dart", lania_dart_bridge::BRIDGE_SCRIPT),
    ]
}

/// 默认落盘目录：workspace 内的 `rust/target/.tmp/bridge-assets`（不写系统 Temp）。
pub fn default_asset_root() -> PathBuf {
    if let Ok(dir) = std::env::var("LANIA_BRIDGE_ASSET_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../../rust/target/.tmp/bridge-assets")
}

pub struct BridgeHost {
    hub: BridgeHub,
    assets: AssetStore,
    children: Vec<Child>,
}

impl BridgeHost {
    pub fn new() -> io::Result<Self> {
        Self::with_asset_root(default_asset_root())
    }

    pub fn with_asset_root(root: PathBuf) -> io::Result<Self> {
        Ok(Self {
            hub: BridgeHub::new(),
            assets: AssetStore::new(root)?,
            children: Vec::new(),
        })
    }

    pub fn asset_root(&self) -> &std::path::Path {
        self.assets.root()
    }

    /// 把生成绑定 + 手写 adaptor 落盘，返回根目录。
    pub fn stage_assets(&self) -> io::Result<PathBuf> {
        self.assets.stage_all(&embedded_assets())?;
        Ok(self.assets.root().to_path_buf())
    }

    /// 进程内 Rust / CLI 端点。
    pub fn attach(&mut self, id: impl Into<String>) -> Endpoint {
        self.hub.attach(id)
    }

    /// 用任意子进程的 stdio 接入一个端点；调用方决定解释器与参数。
    pub fn attach_command(&mut self, id: &str, mut command: Command) -> io::Result<()> {
        self.stage_assets()?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("child stdout is not piped"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("child stdin is not piped"))?;
        self.hub
            .attach_channel(id, Box::new(StdioChannel::new(stdout, stdin)));
        self.children.push(child);
        Ok(())
    }

    /// echo 骨架端点（协议契约测试用）。
    pub fn attach_python(&mut self, id: &str) -> io::Result<()> {
        self.attach_python_script(id, "endpoint.py")
    }

    /// 真实业务端点：lint.mypy/ruff/black/flake8、build.python（Hub 单轨）。
    pub fn attach_python_hub(&mut self, id: &str) -> io::Result<()> {
        self.attach_python_script(id, "hub_endpoint.py")
    }

    fn attach_python_script(&mut self, id: &str, script_name: &str) -> io::Result<()> {
        let root = self.stage_assets()?;
        let script = root.join("adaptors/py").join(script_name);
        let module_path = root.join("protocol/gen/py");
        // 复用 py-bridge 的解释器解析（项目 venv → PATH 的 python/python3），
        // 不硬编码 "python"（很多 Linux 环境只有 python3）；尊重显式覆盖。
        let cwd = std::env::current_dir().unwrap_or_else(|_| root.clone());
        let program = if let Some(explicit) = std::env::var_os("LANIA_BRIDGE_PYTHON") {
            std::path::PathBuf::from(explicit)
        } else {
            lania_py_bridge::resolve_python(&cwd).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "no python interpreter found for hub python endpoint (set LANIA_BRIDGE_PYTHON)",
                )
            })?
        };
        let mut command = Command::new(program);
        command
            .arg(script)
            .arg(format!("--endpoint-id={id}"))
            .env("LANIA_ENDPOINT_ID", id)
            .env("PYTHONPATH", module_path)
            .env("PYTHONUNBUFFERED", "1");
        self.attach_command(id, command)
    }

    pub fn attach_node(&mut self, id: &str) -> io::Result<()> {
        let root = self.stage_assets()?;
        let script = root.join("adaptors/ts/endpoint.ts");
        let program = std::env::var("LANIA_BRIDGE_NODE").unwrap_or_else(|_| "node".to_string());
        let mut command = Command::new(program);
        command
            .arg(script)
            .arg(format!("--endpoint-id={id}"))
            .env("LANIA_ENDPOINT_ID", id);
        self.attach_command(id, command)
    }

    /// 以 Hub 端点身份启动真实 Go bridge（lint.go-vet / build.go 等）。
    /// 复用 go-bridge 缓存二进制，避免同目录 endpoint.go/runtime.go 的多 main 冲突。
    pub fn attach_go(&mut self, id: &str) -> io::Result<()> {
        // 复用 go-bridge 的缓存二进制（首次编译后缓存），避免每次 `go run` 重编。
        // 同一脚本以 LANIA_BRIDGE_MODE=hub 启动即走 BusFrame 端点模式。
        let binary = lania_go_bridge::ensure_binary()
            .map_err(|err| io::Error::other(format!("ensure go bridge binary: {err}")))?;
        let mut command = Command::new(binary);
        command
            .env("LANIA_BRIDGE_MODE", "hub")
            .env("LANIA_ENDPOINT_ID", id);
        self.attach_command(id, command)
    }

    /// 以 Hub 端点身份启动真实 Dart bridge（lint.dart-analyze / build.dart 等）。
    pub fn attach_dart(&mut self, id: &str) -> io::Result<()> {
        let root = self.stage_assets()?;
        let script = root.join("adaptors/dart/lan_dart_bridge.dart");
        // 复用 dart-bridge 的解释器解析（FVM / Flutter / .bat 包装），不硬编码 "dart"。
        let cwd = std::env::current_dir().unwrap_or_else(|_| root.clone());
        let dart = lania_dart_bridge::resolve_dart(&cwd).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no dart sdk found for hub dart endpoint (set LANIA_DART to override)",
            )
        })?;
        let dart_args = dart.argv(&["run".into(), script.display().to_string()]);
        let mut command = Command::new(dart.program);
        command
            .current_dir(root.join("adaptors/dart"))
            .args(dart_args)
            .env("LANIA_BRIDGE_MODE", "hub")
            .env("LANIA_ENDPOINT_ID", id);
        self.attach_command(id, command)
    }

    /// 以 Hub 端点身份启动 Rust bridge（lint.clippy / build.rust）。
    /// 复用 rust-bridge 的 launch_spec（current_exe + __lania_rs_bridge，
    /// 可被 LANIA_RS_BRIDGE 覆盖），加 hub env 走 BusFrame 协议。
    pub fn attach_rust_hub(&mut self, id: &str) -> io::Result<()> {
        let cwd = std::env::current_dir()?;
        let spec = lania_rs_bridge::launch_spec(&cwd)
            .map_err(|err| io::Error::other(format!("rust bridge launch spec: {err}")))?;
        let mut command = Command::new(spec.program);
        command
            .args(spec.args)
            .current_dir(spec.dir)
            .env("LANIA_BRIDGE_MODE", "hub")
            .env("LANIA_ENDPOINT_ID", id);
        self.attach_command(id, command)
    }

    pub fn pump_until_idle(&mut self, max_rounds: usize) -> usize {
        self.hub.pump_until_idle(max_rounds)
    }

    pub fn hub(&mut self) -> &mut BridgeHub {
        &mut self.hub
    }

    pub fn diagnostics(&self) -> &HubDiagnostics {
        self.hub.diagnostics()
    }

    pub fn shutdown(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.children.clear();
    }
}

impl Drop for BridgeHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---------------------------------------------------------------- 宿主装配入口
// §5.1 A.1 / §5.5 E.1：宿主与命令只依赖本 crate，永远不 import 具体语言 bridge crate。
// 5 个 adaptor 的类型/启动函数在这里统一重新导出，作为「1:1 直连 → Hub 端点」过渡期的兼容面。
pub mod provider;

pub use lania_host_core::{
    capability_not_linked, BridgeLaunchSpec, BridgeProvider, BridgeSlot, ConfigSource,
    NullConfigSource,
};
pub use provider::{load_bridge_provider, HubBridgeProvider};

pub use lania_node_bridge as node_bridge;
pub use lania_rs_bridge as rust_bridge;

/// 各语言端点的懒启动装配函数（原 `lania_{py,go,dart,rs}_bridge::launch_spec`）。
pub mod adaptors {
    pub mod py {
        pub use lania_py_bridge::launch_spec;
    }
    pub mod go {
        pub use lania_go_bridge::launch_spec;
    }
    pub mod dart {
        pub use lania_dart_bridge::launch_spec;
    }
    pub mod rust {
        pub use lania_rs_bridge::launch_spec;
    }
}
