//! Dart 运行时 bridge（`lan-dart-bridge`，§3.6）。
//!
//! 职责边界（与 `lania-py-bridge` 同构）：
//! - **本 crate 只负责「定位 dart SDK + 分发脚本 + 决定启动参数」**；
//! - 协议实现在 `assets/lan_dart_bridge.dart`（单文件、只用 `dart:io`），
//!   它说与 node-bridge 完全相同的 jsonl 协议，因此 Rust 侧复用
//!   `lania_node_bridge::NodeBridgeClient`（`BridgeLaunchSpec::for_program`）。
//!
//! 为什么是「脚本 + `dart run`」而不是 §3.6 设想的 AOT 二进制：
//! `dart compile exe` 无法交叉编译（§9.1 约束 2），为它引入 3 个平台包不划算。
//! 脚本形态零平台包，与 `lan-py-bridge` 完全同构；`Platform.resolvedExecutable`
//! 保证 bridge 与 `dart analyze` / `dart compile` 用同一个 SDK。
//!
//! Dart bridge 的存在意义仍然是「去掉 Node 中转」：
//! 进程链从 `Rust → node bridge → compiler-worker → dart compile`
//! 变成 `Rust → dart bridge → dart compile`（§11 B9 验收）。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use lania_bridge_support::{ensure_asset, find_in_path, launch_program, LaunchProgram};

/// 随二进制打包的脚本名（`LANIA_BRIDGE_HOME` 下的落盘文件名，也是 `doctor` 展示的名字）。
pub const BRIDGE_SCRIPT_NAME: &str = "lan_dart_bridge.dart";

/// 内嵌的 bridge 脚本。`include_str!` 而非 `include_bytes!`：
/// 脚本是纯文本，落盘时按 UTF-8 写入，跨平台无换行/编码差异。
pub const BRIDGE_SCRIPT: &str = include_str!("../assets/lan_dart_bridge.dart");

/// 覆盖 dart 可执行文件（测试断言「dart 不可用 ⇒ 回落」）。
pub const DART_ENV: &str = "LANIA_DART";

/// 读取/修改进程环境变量的测试必须串行。
#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn dart_executable_name() -> &'static str {
    if cfg!(windows) {
        "dart.exe"
    } else {
        "dart"
    }
}

/// 候选顺序（§6.4：项目工具链优先，其次 PATH）。
///
/// Dart 没有 `.venv` 这种「项目内解释器」约定，但有 FVM（Flutter Version
/// Management）：`.fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart` 才是项目锁定的
/// 那个 SDK。项目里存在它就优先用它，否则走 PATH。
fn project_candidates(cwd: &Path) -> Vec<PathBuf> {
    let executable = dart_executable_name();
    let mut candidates = Vec::new();
    for root in [".fvm/flutter_sdk", ".fvm/dart_sdk", ".flutter"] {
        let base = cwd.join(root);
        candidates.push(base.join("bin").join(executable));
        candidates.push(base.join("bin/cache/dart-sdk/bin").join(executable));
    }
    candidates
}

/// `dart.bat` / `dart.cmd` 旁边往往就是真正的 `dart.exe`
/// （Flutter：`bin/dart.bat` 与 `bin/cache/dart-sdk/bin/dart.exe`）。
///
/// 优先拿 `.exe`：批处理脚本在 Windows 上不能被 `CreateProcess` 直接拉起，
/// 而 Flutter 的 `dart.bat` 会再多起一层 cmd + shared.bat。
fn sibling_real_dart(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let candidate = dir.join("cache/dart-sdk/bin").join(dart_executable_name());
    candidate.is_file().then_some(candidate)
}

/// 在 `cwd` 下找可用的 dart 可执行文件。
pub fn resolve_dart(cwd: &Path) -> Option<LaunchProgram> {
    resolve_dart_unlocked(cwd)
}

fn resolve_dart_unlocked(cwd: &Path) -> Option<LaunchProgram> {
    if let Some(explicit) = std::env::var_os(DART_ENV).filter(|value| !value.is_empty()) {
        // 显式指定时尊重用户选择（即使路径不存在，也让启动阶段把真实错误暴露出来）。
        return Some(launch_program(PathBuf::from(explicit)));
    }
    for candidate in project_candidates(cwd) {
        if candidate.is_file() {
            return Some(launch_program(candidate));
        }
    }
    let found = find_in_path("dart")?;
    if let Some(real) = sibling_real_dart(&found) {
        return Some(launch_program(real));
    }
    Some(launch_program(found))
}

/// bridge 脚本的落盘位置（`LANIA_BRIDGE_HOME` 下的固定文件名）。
pub fn script_path() -> Result<PathBuf> {
    ensure_asset(BRIDGE_SCRIPT_NAME, BRIDGE_SCRIPT)
}

/// 启动方式：`program + args + dir`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DartBridgeLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

/// 生成 `lan-dart-bridge` 的启动参数。
///
/// `dart run <script>`：脚本直接跑在当前 VM 里，不多一层子进程，
/// 因此「Rust → dart bridge → dart analyze」的层数就是 2。
pub fn launch_spec(cwd: &Path) -> Result<DartBridgeLaunch> {
    let dart = resolve_dart(cwd).ok_or_else(|| {
        anyhow!(
            "no dart sdk found (looked for .fvm/flutter_sdk and PATH); \
             set {DART_ENV} to override"
        )
    })?;
    let script = script_path()?;
    let args = vec!["run".to_string(), script.display().to_string()];
    let argv = dart.argv(&args);
    Ok(DartBridgeLaunch {
        program: dart.program,
        // `.bat` 包装的前置参数必须排在 `run <script>` 之前。
        args: argv,
        dir: cwd.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lania-dart-bridge-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn embedded_script_is_the_dart_bridge() {
        assert!(BRIDGE_SCRIPT.contains("_bridgeName = 'lan-dart-bridge'"));
        assert!(BRIDGE_SCRIPT.contains("lint.dart-analyze"));
        assert!(BRIDGE_SCRIPT.contains("build.dart"));
        // 协议约束：所有 envelope 必须经 `_emit`（串行队列）写到 stdout。
        assert!(BRIDGE_SCRIPT.contains("Future<void> _outbox"));
        assert!(BRIDGE_SCRIPT.contains("stdout.writeln(line)"));
    }

    #[test]
    fn launch_runs_the_script_with_dart_run() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let dir = temp_dir("launch");
        std::env::set_var("LANIA_BRIDGE_HOME", dir.join("bridge-home"));
        std::env::set_var(DART_ENV, "dart");
        let launch = launch_spec(&dir).expect("launch spec");
        // 显式覆盖时不做存在性检查：真实错误留到启动阶段。
        assert_eq!(launch.program, PathBuf::from("dart"));
        assert_eq!(launch.args.first().map(String::as_str), Some("run"));
        assert!(launch.args[1].ends_with(BRIDGE_SCRIPT_NAME));
        assert!(script_path().expect("script path").exists());
        std::env::remove_var(DART_ENV);
        std::env::remove_var("LANIA_BRIDGE_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn project_fvm_sdk_wins_over_path() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let dir = temp_dir("fvm");
        let bin = dir.join(".fvm/flutter_sdk/bin/cache/dart-sdk/bin");
        std::fs::create_dir_all(&bin).expect("fvm bin");
        let executable = bin.join(dart_executable_name());
        std::fs::write(&executable, "").expect("fake dart");
        std::env::remove_var(DART_ENV);
        let resolved = resolve_dart_unlocked(&dir).expect("dart resolved");
        assert_eq!(resolved.program, executable);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_dart_sdk_reports_an_actionable_error() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let dir = temp_dir("missing");
        // 显式空值不算覆盖：桥会继续走 PATH 探测。
        std::env::set_var(DART_ENV, "");
        let path = std::env::var_os("PATH");
        // 清掉 PATH 之后一定找不到 dart；先把两次调用都跑完再恢复，
        // 避免中途断言失败污染其它用例（PATH 是进程级全局状态）。
        std::env::set_var("PATH", "");
        let resolved = resolve_dart_unlocked(&dir);
        let launch_error = launch_spec(&dir).err();
        match path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        std::env::remove_var(DART_ENV);
        assert!(resolved.is_none(), "dart must not resolve without PATH");
        let error = launch_error.expect("no dart sdk");
        assert!(error.to_string().contains("no dart sdk found"), "{error}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
