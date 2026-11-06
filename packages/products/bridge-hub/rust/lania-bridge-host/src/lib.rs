//! Bridge Hub 的进程内装配层（bridge-hub 包的一部分）。
//!
//! 决策（`package-split-command-atomization.zh-CN.md` §5.5）：Hub 静态链接进命令进程，
//! Rust 端点走进程内 channel，TS/Py/Go/Dart 端点走 stdio 子进程。本 crate 负责
//! 创建 Hub、把内嵌的生成物与手写 adaptor 落盘、接入子进程端点、暴露诊断。
//! 宿主代码永远不 import 任何具体语言 bridge crate。

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use lania_bridge_bus::{BridgeHub, Endpoint, HubDiagnostics, StdioChannel};
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
        ("adaptors/dart/endpoint.dart", DART_ENDPOINT),
        ("adaptors/go/endpoint.go", GO_ENDPOINT),
        // Go / Dart 需要与 endpoint 同目录才能按单包编译运行。
        ("adaptors/go/protocol.go", GO_PROTOCOL),
        ("adaptors/go/runtime.go", GO_RUNTIME),
        ("adaptors/go/go.mod", GO_MOD),
        ("adaptors/dart/protocol.dart", DART_PROTOCOL),
        ("adaptors/dart/runtime.dart", DART_RUNTIME),
        ("adaptors/dart/pubspec.yaml", DART_PUBSPEC),
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

    pub fn attach_python(&mut self, id: &str) -> io::Result<()> {
        let root = self.stage_assets()?;
        let script = root.join("adaptors/py/endpoint.py");
        let module_path = root.join("protocol/gen/py");
        let program = std::env::var("LANIA_BRIDGE_PYTHON").unwrap_or_else(|_| "python".to_string());
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

    pub fn attach_go(&mut self, id: &str) -> io::Result<()> {
        let root = self.stage_assets()?;
        let mut command = Command::new("go");
        command
            .current_dir(root.join("adaptors/go"))
            .arg("run")
            .arg(".")
            .arg(format!("--endpoint-id={id}"))
            .env("LANIA_ENDPOINT_ID", id);
        self.attach_command(id, command)
    }

    pub fn attach_dart(&mut self, id: &str) -> io::Result<()> {
        let root = self.stage_assets()?;
        let script = root.join("adaptors/dart/endpoint.dart");
        let mut command = Command::new("dart");
        command
            .current_dir(root.join("adaptors/dart"))
            .arg("run")
            .arg(script)
            .arg(format!("--endpoint-id={id}"))
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
