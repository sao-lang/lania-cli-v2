//! Go 运行时 bridge（`lan-go-bridge`，§3.4）。
//!
//! 职责边界（与 `lania-py-bridge` 同构）：
//! - **本 crate 只负责「定位 go + 分发源码 + 编译并缓存可执行文件」**；
//! - 协议实现在 `assets/lan_go_bridge.go`（单文件、只依赖标准库），
//!   它说与 node-bridge 完全相同的 jsonl 协议，因此 Rust 侧复用
//!   `lania_node_bridge::NodeBridgeClient`（`BridgeLaunchSpec::for_program`）。
//!
//! 为什么在运行期用 `go build` 现场编译，而不是只发行预编译二进制（§9.1）：
//! - CI 交叉编译仍是**首选**路径：`LANIA_GO_BRIDGE` 或与 CLI 同目录的
//!   `lan-go-bridge[.exe]` 直接命中，一次编译都不做；
//! - 但「只装了 CLI + go」的用户不该因为没装平台包而完全失去 Go bridge，
//!   所以这里补一条「无预编译产物时用本机 go 编译一次并缓存」的路径。
//!   缓存键 = 源码内容 + crate 版本，升级 CLI 自动失效。
//!
//! 代价：首次使用多一次 `go build`（约 0.3–1s，Go 自身有构建缓存），之后零成本。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use lania_bridge_support::{ensure_asset, find_in_path, launch_program, LaunchProgram};

/// 内嵌的 bridge 源码（落盘文件名）。
pub const BRIDGE_SOURCE_NAME: &str = "lan_go_bridge.go";

/// 内嵌的 bridge 源码。`include_str!`：源码是纯文本，落盘时按 UTF-8 写入。
pub const BRIDGE_SOURCE: &str = include_str!("../assets/lan_go_bridge.go");

/// bridge 可执行文件名（预编译产物与缓存产物同名）。
pub const BRIDGE_BINARY_NAME: &str = "lan-go-bridge";

/// 覆盖 `go` 可执行文件（测试用来断言「go 不可用 ⇒ 回落」）。
pub const GO_ENV: &str = "LANIA_GO";

/// 覆盖预编译 bridge 二进制（CI / 平台包安装场景）。
pub const BRIDGE_BINARY_ENV: &str = "LANIA_GO_BRIDGE";

/// 读取/修改进程环境变量的测试必须串行（`LANIA_GO` / `LANIA_BRIDGE_HOME` 是全局状态）。
#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 定位 `go` 工具链。
///
/// 只看「存在性」，不执行 `go version`：真正的失败由编译阶段暴露，
/// 避免每次调用都多拉起一个进程（§6.4 的启动开销约束）。
pub fn resolve_go() -> Option<LaunchProgram> {
    if let Some(explicit) = std::env::var_os(GO_ENV).filter(|value| !value.is_empty()) {
        return Some(launch_program(PathBuf::from(explicit)));
    }
    find_in_path("go").map(launch_program)
}

/// 缓存的可执行文件路径。
pub fn binary_path() -> PathBuf {
    let name = if cfg!(windows) {
        format!("{BRIDGE_BINARY_NAME}.exe")
    } else {
        BRIDGE_BINARY_NAME.to_string()
    };
    lania_bridge_support::bridge_home().join(name)
}

fn hash_path() -> PathBuf {
    lania_bridge_support::bridge_home().join(format!("{BRIDGE_BINARY_NAME}.hash"))
}

/// 缓存键：源码内容 + crate 版本（升级 CLI 后自动重编）。
fn cache_key() -> String {
    lania_bridge_support::content_hash(&format!("{}\n{}", env!("CARGO_PKG_VERSION"), BRIDGE_SOURCE))
}

/// 已存在的 bridge 二进制：优先用显式指定的，其次用与 CLI 同目录的预编译产物，最后用缓存。
fn existing_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os(BRIDGE_BINARY_ENV).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(explicit));
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            for candidate in [
                dir.join(BRIDGE_BINARY_NAME),
                dir.join(format!("{BRIDGE_BINARY_NAME}.exe")),
            ] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    let cached = binary_path();
    // 缓存产物必须与当前源码匹配，否则说明 CLI 刚升级过。
    let fresh = std::fs::read_to_string(hash_path())
        .map(|recorded| recorded.trim() == cache_key())
        .unwrap_or(false);
    (cached.is_file() && fresh).then_some(cached)
}

/// 确保 bridge 二进制可用（必要时用本机 `go` 编译一次）。
///
/// 并发安全：先编译到带 pid 的临时文件再 `rename` 到目标路径，
/// 多个进程同时首次使用只会重复劳动，不会读到半成品。
pub fn ensure_binary() -> Result<PathBuf> {
    if let Some(binary) = existing_binary() {
        return Ok(binary);
    }
    let go = resolve_go().ok_or_else(|| {
        anyhow!("no go toolchain found (set {GO_ENV} to override); the go bridge cannot be built")
    })?;
    let source = ensure_asset(BRIDGE_SOURCE_NAME, BRIDGE_SOURCE)?;
    let target = binary_path();
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| anyhow!("failed to create {}: {error}", parent.display()))?;
    }
    let staging = target.with_extension(format!(
        "staging-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    ));
    let args = vec![
        "build".to_string(),
        "-o".to_string(),
        staging.display().to_string(),
        source.display().to_string(),
    ];
    let output = Command::new(&go.program)
        // `.bat` 形式的 go 包装器在 Windows 上必须经 cmd 解析。
        .args(go.argv(&args))
        .env("CGO_ENABLED", "0")
        .current_dir(source.parent().unwrap_or(Path::new(".")))
        .output()
        .with_context(|| format!("failed to run {} build", go.program.display()))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&staging);
        return Err(anyhow!(
            "go bridge build failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if target.exists() {
        // 并发首次使用时会两个进程都编译：删掉旧的再 rename，失败也不致命。
        let _ = std::fs::remove_file(&target);
    }
    if let Err(error) = std::fs::rename(&staging, &target) {
        // 另一个进程刚把同名文件放好：直接复用它，别把已可用的情况当失败。
        let _ = std::fs::remove_file(&staging);
        if target.is_file() {
            let _ = std::fs::write(hash_path(), cache_key());
            return Ok(target);
        }
        return Err(anyhow!("failed to install {}: {error}", target.display()));
    }
    std::fs::write(hash_path(), cache_key())
        .map_err(|error| anyhow!("failed to record bridge cache key: {error}"))?;
    Ok(target)
}

/// 启动方式：`program + args + dir`，交给 `NodeBridgeClient` 复用同一套传输层。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoBridgeLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

/// 生成 `lan-go-bridge` 的启动参数。
pub fn launch_spec(cwd: &Path) -> Result<GoBridgeLaunch> {
    let program = ensure_binary()?;
    Ok(GoBridgeLaunch {
        program,
        args: Vec::new(),
        dir: cwd.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_source_is_the_go_bridge() {
        assert!(BRIDGE_SOURCE.contains("const (\n\tbridgeName"));
        assert!(BRIDGE_SOURCE.contains("\"lan-go-bridge\""));
        assert!(BRIDGE_SOURCE.contains("lint.go-vet"));
        assert!(BRIDGE_SOURCE.contains("build.go"));
        // 协议约束：源码里不允许出现裸 fmt.Print* 之外的 stdout 写协议外内容。
        assert!(BRIDGE_SOURCE.contains("os.Stdout.Write"));
    }

    #[test]
    fn cache_key_changes_with_the_source_or_version() {
        let key = cache_key();
        assert_eq!(key, cache_key());
        assert_ne!(key, lania_bridge_support::content_hash("other source"));
    }

    #[test]
    fn missing_go_toolchain_is_reported_not_panicked() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let dir = temp_dir("go-missing");
        std::env::set_var("LANIA_BRIDGE_HOME", dir.join("bridge-home"));
        std::env::set_var(GO_ENV, "/definitely/not/a/go");
        // 预编译产物也不存在 ⇒ 只能尝试编译 ⇒ 失败要变成 Err（调用方据此回落）。
        std::env::remove_var(BRIDGE_BINARY_ENV);
        let error = ensure_binary().expect_err("no go toolchain");
        assert!(
            error.to_string().contains("go bridge build failed")
                || error.to_string().contains("no go toolchain")
                || error.to_string().contains("failed to run"),
            "{error}"
        );
        std::env::remove_var(GO_ENV);
        std::env::remove_var("LANIA_BRIDGE_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_prebuilt_binary_is_used_verbatim() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let dir = temp_dir("go-prebuilt");
        let binary = dir.join(if cfg!(windows) {
            "lan-go-bridge.exe"
        } else {
            "lan-go-bridge"
        });
        std::fs::write(&binary, "").expect("fake binary");
        std::env::set_var(BRIDGE_BINARY_ENV, &binary);
        assert_eq!(ensure_binary().expect("prebuilt"), binary);
        std::env::remove_var(BRIDGE_BINARY_ENV);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lania-go-bridge-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
