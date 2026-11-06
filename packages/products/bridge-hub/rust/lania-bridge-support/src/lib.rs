//! 各「语言 bridge」共用的基础设施。
//!
//! 三个 bridge（`lan-py-bridge` / `lan-go-bridge` / `lan-dart-bridge`）解决的都是
//! 同一类问题，因此这里只保留一份实现：
//!
//! 1. **资产落盘**：单文件脚本 / 源码随二进制内嵌（`include_str!`），启动时写到
//!    `LANIA_BRIDGE_HOME`（默认用户缓存目录）。内容相同就跳过写入，升级 CLI 后自动刷新。
//! 2. **可执行文件定位**：PATH 扫描必须认 Windows 的 `PATHEXT`（`dart` 在 Flutter 下
//!    只有 `dart.bat`），且 `.bat` / `.cmd` 不能直接被 `CreateProcess` 拉起。
//!
//! 为什么单独成一个 crate：`lania-py-bridge` 只依赖 `anyhow`，
//! 不该为了几十行路径工具去拉 `lania-node-bridge`（连带 tokio）。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// 覆盖 bridge 资产目录的环境变量（测试与安装产物共用）。
pub const BRIDGE_HOME_ENV: &str = "LANIA_BRIDGE_HOME";

/// bridge 资产目录。
///
/// 放在 `cwd` 下会污染用户仓库，放 `$TMPDIR` 又可能被清理；
/// 因此用用户缓存目录：`LANIA_BRIDGE_HOME` > `LOCALAPPDATA`(Windows) >
/// `XDG_CACHE_HOME` / `~/.cache`(POSIX) > 临时目录。
pub fn bridge_home() -> PathBuf {
    if let Some(explicit) = std::env::var_os(BRIDGE_HOME_ENV).filter(|value| !value.is_empty()) {
        return PathBuf::from(explicit);
    }
    default_bridge_home()
}

fn default_bridge_home() -> PathBuf {
    // 不引入 `dirs` 依赖：两个平台变量足够覆盖 CLI 的实际使用场景。
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|value| PathBuf::from(value).join("lan").join("bridge"))
            .unwrap_or_else(|| std::env::temp_dir().join("lan").join("bridge"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(|value| PathBuf::from(value).join("lan").join("bridge"))
            .or_else(|| {
                std::env::var_os("HOME").map(|value| PathBuf::from(value).join(".cache/lan/bridge"))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("lan").join("bridge"))
    }
}

/// 把内嵌资产写到 bridge 资产目录，返回最终路径。
pub fn ensure_asset(name: &str, contents: &str) -> Result<PathBuf> {
    ensure_asset_in(&bridge_home(), name, contents)
}

/// 把内嵌资产写到 `dir` 下（测试用固定目录）。
pub fn ensure_asset_in(dir: &Path, name: &str, contents: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    // 内容相同就不重写：每次启动都落盘会让只读磁盘 / 频繁 lint 场景变慢。
    if let Ok(current) = std::fs::read_to_string(&path) {
        if current == contents {
            return Ok(path);
        }
    }
    std::fs::create_dir_all(dir)
        .map_err(|error| anyhow!("failed to create {}: {error}", dir.display()))?;
    std::fs::write(&path, contents)
        .map_err(|error| anyhow!("failed to write {}: {error}", path.display()))?;
    Ok(path)
}

/// 一个「可执行文件 + 前置参数」。
///
/// `args` 非空只发生在 Windows 的 `.bat` / `.cmd` 上：这类脚本必须经 `cmd.exe`
/// 解析，直接交给 `CreateProcess` 会报「不是有效的 Win32 应用程序」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchProgram {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
}

impl LaunchProgram {
    /// 直接执行的普通可执行文件。
    pub fn direct(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            prefix_args: Vec::new(),
        }
    }

    /// 把 `args` 拼成完整的 argv（前置参数在前）。
    pub fn argv(&self, args: &[String]) -> Vec<String> {
        let mut argv = self.prefix_args.clone();
        argv.extend(args.iter().cloned());
        argv
    }
}

/// 如果 `path` 是 Windows 批处理脚本，则包一层 `cmd /d /s /c`。
pub fn launch_program(path: impl AsRef<Path>) -> LaunchProgram {
    let path = path.as_ref();
    #[cfg(windows)]
    {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension == "bat" || extension == "cmd" {
            // `/d` 跳过 AutoRun 脚本，`/s` 让 cmd 按「原样」解析剩余命令行。
            //
            // 这里**不能**自己给路径加引号：Rust 拼命令行时会把内层引号转义成 `\"`，
            // 而 cmd.exe 不认反斜杠转义（会去找名字里带反斜杠的文件）。
            // 交给 Rust 按需加引号即可 —— 带空格的路径会得到一个 `"C:\a b\x.bat"`
            // 参数，cmd 能正确解析。
            return LaunchProgram {
                program: PathBuf::from("cmd.exe"),
                prefix_args: vec![
                    "/d".into(),
                    "/s".into(),
                    "/c".into(),
                    path.display().to_string(),
                ],
            };
        }
    }
    LaunchProgram::direct(path)
}

/// 在 PATH 里找一个可执行文件（Windows 会按 `PATHEXT` 试扩展名）。
///
/// 返回的是**命中的文件路径**，因此调用方可以用 [`launch_program`] 处理
/// `.bat` / `.cmd`；这比把裸名字交给 `Command` 更可靠（后者在 Windows 上找不到 `dart.bat`）。
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    find_in_paths(name, std::env::split_paths(&path))
}

/// [`find_in_path`] 的可测试版本：显式传入目录列表。
pub fn find_in_paths<I>(name: &str, dirs: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    let extensions = executable_extensions();
    for dir in dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        // 名字本身带扩展名（或 POSIX 下的可执行文件）先直接命中。
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        for extension in &extensions {
            let candidate = dir.join(format!("{name}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn executable_extensions() -> Vec<String> {
    if cfg!(windows) {
        let raw = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        let mut extensions = Vec::new();
        for item in raw.split(';') {
            let item = item.trim();
            if !item.is_empty() {
                extensions.push(item.to_ascii_lowercase());
            }
        }
        if extensions.is_empty() {
            extensions.push(".exe".into());
        }
        extensions
    } else {
        Vec::new()
    }
}

/// 简易 64 位 FNV-1a：只用来给内嵌源码做缓存键，不需要密码学强度。
///
/// 刻意不引入 `sha2`：bridge 资产目录里的 `*.hash` 只与本次 CLI 版本的内容比对，
/// 冲突概率（2^64 空间）远低于「同一台机器上有人手工替换源码」的概率。
pub fn content_hash(contents: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in contents.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lania-bridge-support-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", dir.display()));
        dir
    }

    #[test]
    fn ensure_asset_is_idempotent_and_refreshes_on_change() {
        let dir = temp_dir("asset");
        let first = ensure_asset_in(&dir, "bridge.txt", "v1").expect("first write");
        let second = ensure_asset_in(&dir, "bridge.txt", "v1").expect("second write");
        assert_eq!(first, second);
        assert_eq!(std::fs::read_to_string(&first).expect("read"), "v1");

        ensure_asset_in(&dir, "bridge.txt", "v2").expect("refresh");
        assert_eq!(std::fs::read_to_string(&first).expect("read"), "v2");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn find_in_paths_hits_the_first_existing_candidate() {
        let dir = temp_dir("path");
        let empty = dir.join("empty");
        let full = dir.join("full");
        std::fs::create_dir_all(&empty).expect("empty");
        std::fs::create_dir_all(&full).expect("full");
        let executable = full.join(if cfg!(windows) { "tool.EXE" } else { "tool" });
        std::fs::write(&executable, "").expect("fake executable");

        let found = find_in_paths("tool", vec![empty.clone(), full.clone()]).expect("tool found");
        // 比较规范化之后的路径：Windows 大小写不敏感，PATHEXT 的写法可能与小写扩展名不同，
        // 但指向的必须是**同一个文件**。
        assert_eq!(
            found.canonicalize().expect("found canonical"),
            executable.canonicalize().expect("executable canonical")
        );
        assert!(find_in_paths("missing-tool", vec![full]).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn content_hash_is_stable_and_content_sensitive() {
        let a = content_hash("package main\n");
        assert_eq!(a, content_hash("package main\n"));
        assert_ne!(a, content_hash("package main\n\n"));
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn direct_program_has_no_prefix_args() {
        let launch = launch_program("/usr/bin/dart");
        assert_eq!(launch.program, PathBuf::from("/usr/bin/dart"));
        assert!(launch.prefix_args.is_empty());
        assert_eq!(
            launch.argv(&["run".into(), "a.dart".into()]),
            vec!["run".to_string(), "a.dart".to_string()]
        );
    }

    #[cfg(windows)]
    #[test]
    fn batch_files_are_wrapped_with_cmd() {
        let launch = launch_program(r"C:\tools\dart.bat");
        assert_eq!(launch.program, PathBuf::from("cmd.exe"));
        assert_eq!(
            launch.prefix_args,
            vec![
                "/d".to_string(),
                "/s".to_string(),
                "/c".to_string(),
                r"C:\tools\dart.bat".to_string()
            ]
        );
        // 不能预先把路径包进引号：Rust 会把内层引号转义成 `\"`，cmd 不认这种转义。
        assert!(
            launch.prefix_args.iter().all(|arg| !arg.contains('"')),
            "prefix args must not contain quotes: {:?}",
            launch.prefix_args
        );
    }

    /// 真正跑一遍「`.bat` 经 cmd 包装后仍能收到参数」——这是 Dart bridge 在
    /// 「只有 `dart.bat` 没有 `dart.exe`」时必须走通的路径。
    #[cfg(windows)]
    #[test]
    fn batch_wrapper_forwards_arguments() {
        let dir = temp_dir("batch");
        let script = dir.join("echo-args.bat");
        std::fs::write(&script, "@echo off\r\necho %1:%2\r\n").expect("batch script");
        let launch = launch_program(&script);
        let output = std::process::Command::new(&launch.program)
            .args(launch.argv(&["alpha".to_string(), "beta".to_string()]))
            .output()
            .expect("cmd runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{stdout}");
        assert!(stdout.contains("alpha:beta"), "{stdout}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
