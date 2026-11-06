//! Python 运行时 bridge（`lan-py-bridge`，§3.3）。
//!
//! 职责边界：
//! - **本 crate 只负责「定位 python + 定位脚本 + 决定启动参数」**，不实现 lint 逻辑；
//! - 真正的协议实现是 `assets/lan_py_bridge.py`（单文件、零第三方依赖），
//!   它说与 node-bridge 完全相同的 jsonl 协议，因此 Rust 侧可以复用
//!   `lania_node_bridge::NodeBridgeClient`（`BridgeLaunchSpec::for_program`）。
//!
//! 为什么不是 Rust 直接解析 mypy 输出：Python bridge 的价值在于
//! `mypy.api.run()` 进程内调用（省掉解释器启动，§3.3）；协议一旦落到 Rust，
//! 就等于把「Python 工具知识」写了两份（见 §5.3 回落要求）。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use lania_bridge_support::ensure_asset;

/// 随二进制打包的脚本名（`include_str!` 的落盘文件名，也是 `doctor` 展示的名字）。
pub const BRIDGE_SCRIPT_NAME: &str = "lan_py_bridge.py";

/// 内嵌的 bridge 脚本。`include_str!` 而非 `include_bytes!`：
/// 脚本是纯文本，落盘时按 UTF-8 写入，跨平台无换行/编码差异。
pub const BRIDGE_SCRIPT: &str = include_str!("../assets/lan_py_bridge.py");

/// Python 解释器的候选顺序（§6.4：项目虚拟环境优先，其次 PATH）。
fn python_candidates(cwd: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let executables: &[&str] = if cfg!(windows) {
        &["python.exe", "python3.exe", "py.exe"]
    } else {
        &["python3", "python"]
    };
    for venv in [".venv", "venv"] {
        for sub in ["Scripts", "bin"] {
            for executable in executables {
                candidates.push(cwd.join(venv).join(sub).join(executable));
            }
        }
    }
    for executable in executables {
        candidates.push(PathBuf::from(executable));
    }
    candidates
}

/// 在 `cwd` 下找可用的 Python 解释器。
///
/// 只做**存在性**判断，不执行 `<python> --version`：探测本身不该拉起进程，
/// 真正的失败由 bridge 启动阶段暴露（`NodeBridgeClient` 会把 spawn 错误带上来）。
fn resolve_python_unlocked(cwd: &Path) -> Option<PathBuf> {
    let from_env = std::env::var("LANIA_PYTHON")
        .ok()
        .filter(|value| !value.is_empty());
    if let Some(value) = from_env {
        return Some(PathBuf::from(value));
    }
    python_candidates(cwd)
        .into_iter()
        .find(|candidate| candidate.is_file() || !candidate.is_absolute())
}

/// 在 `cwd` 下找可用的 Python 解释器（读 `LANIA_PYTHON`，需持 env 锁）。
pub fn resolve_python(cwd: &Path) -> Option<PathBuf> {
    resolve_python_unlocked(cwd)
}

/// 读取/修改进程环境变量的测试必须串行：`LANIA_PYTHON` / `LANIA_BRIDGE_HOME`
/// 是进程级全局状态，并发测试会互相覆盖。
#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// bridge 脚本的落盘位置。
///
/// 放在 `cwd` 下会污染用户仓库，放在 `$TMPDIR` 又可能被清理；
/// 因此用 `LANIA_BRIDGE_HOME`（默认用户缓存目录）下的固定文件名，
/// 每次启动比对内容，脚本随 CLI 升级自动刷新。
///
/// 「资产目录 + 幂等落盘」的实现与 `lan-go-bridge` / `lan-dart-bridge` 共用，
/// 见 `lania_bridge_support`（三处各写一遍必然漂移）。
pub fn script_path() -> Result<PathBuf> {
    ensure_asset(BRIDGE_SCRIPT_NAME, BRIDGE_SCRIPT)
}

/// 启动方式：`program + args + dir`，交给 `NodeBridgeClient` 复用同一套传输层。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonBridgeLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

/// 生成 `lan-py-bridge` 的启动参数。
///
/// `-u` 是必须的：Python 默认块缓冲，事件会滞后到缓冲区写满才发出，
/// 长任务（dev/watch）下表现为「日志不实时」（§10 风险表的行缓冲条目）。
pub fn launch_spec(cwd: &Path) -> Result<PythonBridgeLaunch> {
    let python = resolve_python(cwd).ok_or_else(|| {
        anyhow!(
            "no python interpreter found (looked for .venv/venv binaries and PATH); \
             set LANIA_PYTHON to override"
        )
    })?;
    let script = script_path()?;
    Ok(PythonBridgeLaunch {
        program: python,
        args: vec!["-u".into(), script.display().to_string()],
        dir: cwd.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_script_is_the_python_bridge() {
        assert!(BRIDGE_SCRIPT.contains("BRIDGE_NAME = \"lan-py-bridge\""));
        assert!(BRIDGE_SCRIPT.contains("lint.mypy"));
        assert!(BRIDGE_SCRIPT.contains("build.python"));
    }

    #[test]
    fn launch_uses_unbuffered_python_and_the_script() {
        let _guard = super::ENV_LOCK.lock().expect("env lock");
        let dir = std::env::temp_dir().join(format!("lania-py-launch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::env::set_var("LANIA_BRIDGE_HOME", dir.join("bridge-home"));
        std::env::set_var("LANIA_PYTHON", "python3");
        let launch = launch_spec(&dir).expect("launch spec");
        assert_eq!(launch.program, PathBuf::from("python3"));
        assert_eq!(launch.args.first().map(String::as_str), Some("-u"));
        assert!(launch.args[1].ends_with(BRIDGE_SCRIPT_NAME));
        assert!(script_path().expect("script path").exists());
        std::env::remove_var("LANIA_PYTHON");
        std::env::remove_var("LANIA_BRIDGE_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn project_virtualenv_wins_over_path() {
        let _guard = super::ENV_LOCK.lock().expect("env lock");
        let dir = std::env::temp_dir().join(format!("lania-py-venv-{}", std::process::id()));
        let bin = if cfg!(windows) {
            dir.join(".venv").join("Scripts")
        } else {
            dir.join(".venv").join("bin")
        };
        std::fs::create_dir_all(&bin).expect("venv dir");
        let executable = bin.join(if cfg!(windows) {
            "python.exe"
        } else {
            "python"
        });
        std::fs::write(&executable, "").expect("fake python");
        // `LANIA_PYTHON` 优先级最高：测试里先清掉，否则开发者环境会干扰结果。
        std::env::remove_var("LANIA_PYTHON");
        let resolved = resolve_python_unlocked(&dir).expect("python resolved");
        assert_eq!(resolved, executable);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ensure_script_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("lania-py-script-{}", std::process::id()));
        let path = dir.join(BRIDGE_SCRIPT_NAME);
        lania_bridge_support::ensure_asset_in(&dir, BRIDGE_SCRIPT_NAME, BRIDGE_SCRIPT)
            .expect("first write");
        lania_bridge_support::ensure_asset_in(&dir, BRIDGE_SCRIPT_NAME, BRIDGE_SCRIPT)
            .expect("second write");
        assert_eq!(
            std::fs::read_to_string(&path).expect("readable"),
            BRIDGE_SCRIPT
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// §12 验收：同一份 ruff 输出经 **Python bridge** 与 **JS 簇参考解析器**
    /// 解析出来的 `Diagnostic` 必须深度相等。
    ///
    /// 实现方式：两边读同一份 fixture 与同一份 golden。
    /// - Python 侧：本用例调用 bridge 脚本的 `parse_ruff_json`（需要 python 解释器）；
    /// - TS 侧：`ts/packages/node-bridge/src/plugins/lint/ruff-fixture.test.ts` 断言同一份 golden。
    #[test]
    fn ruff_parser_matches_the_shared_golden() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fixtures");
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/lan_py_bridge.py");
        let fixture = fixtures.join("ruff-sample.json");
        let golden = fixtures.join("ruff-diagnostics.json");
        let expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&golden).expect("golden readable"))
                .expect("golden is json");

        let Some(_guard) = super::ENV_LOCK.try_lock().ok() else {
            // 其它用例正在改写进程级 env（LANIA_PYTHON）；串行执行时不会走到这里。
            return;
        };
        let Some(python) = super::resolve_python_unlocked(Path::new(".")) else {
            eprintln!("skipping ruff golden test: no python interpreter found");
            return;
        };

        // 只做「导入 + 解析」，不启动 bridge 主循环（`__name__ != "__main__"`）。
        let code = format!(
            "import importlib.util, json\n\
             spec = importlib.util.spec_from_file_location('lan_py_bridge', r'{script}')\n\
             module = importlib.util.module_from_spec(spec)\n\
             spec.loader.exec_module(module)\n\
             raw = open(r'{fixture}', encoding='utf-8').read()\n\
             print(json.dumps(module.parse_ruff_json(raw), ensure_ascii=False))\n",
            script = script.display(),
            fixture = fixture.display(),
        );
        let output = std::process::Command::new(&python)
            .arg("-c")
            .arg(code)
            .output()
            .expect("python runs");
        assert!(
            output.status.success(),
            "python bridge parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("bridge output is json");
        assert_eq!(
            actual, expected,
            "python bridge ruff parser diverged from the shared golden"
        );
    }
}
