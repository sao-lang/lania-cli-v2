//! 工具链状态探测：当前版本、可执行路径、项目版本约束。
//!
//! 这一层只读，不做任何修改，是 GUI 与 CLI 共用的数据源。
//!
//! 探测分两类来源：
//! - **lania 自管**（`~/.runex/runtimes/<lang>/<version>`）：安装、切换、卸载都走
//!   [`crate::RuntimeStore`]；这里的 `source = "lania"` 与 `current` 指针用于展示；
//! - **系统/外部安装**：只探测「有没有、什么版本、在哪」，不接管、不修改。

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::state::{InstalledMeta, RuntimeState};
use crate::{Language, RuntimeHome};

/// 单个受管工具（语言或附属包管理器）的完整状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    pub name: String,
    pub language: String,
    /// 是否可用（PATH 中能找到，或 lania 已托管该语言的 current 版本）。
    pub installed: bool,
    pub version: Option<String>,
    pub binary_path: Option<String>,
    /// 版本来源：`lania`（自管）或 `system`（外部安装）。
    pub source: Option<String>,
    /// lania 是否自己管理这个语言（第一批语言为 true）。
    pub managed: bool,
    /// 上游分发渠道（自管语言才有）。
    pub upstream: Option<String>,
    /// lania 自管目录下的已安装版本数。
    pub installed_count: Option<usize>,
    /// 项目目录中的版本约束文件与内容。
    pub project_version: Option<ProjectVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVersion {
    pub file: String,
    pub version: String,
}

/// 一条「已安装版本」记录：版本号 + 来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    pub source: String,
    pub version: String,
    pub current: bool,
}

/// 环境探测服务。
#[derive(Debug, Clone, Default)]
pub struct EnvService;

impl EnvService {
    pub fn new() -> Self {
        Self
    }

    /// 探测全部受支持工具（语言 + 附属包管理器）。
    pub fn scan(&self, cwd: &Path) -> Vec<ToolStatus> {
        self.scan_with(cwd, true)
    }

    /// 同 [`Self::scan`]，但可跳过版本号探测（`with_version = false`）。
    ///
    /// TUI 首屏用 `false` 先把列表画出来，之后按需对选中项调用
    /// [`Self::resolve_version`] 补全版本，避免打开界面时卡住几秒。
    pub fn scan_with(&self, cwd: &Path, with_version: bool) -> Vec<ToolStatus> {
        let mut names: Vec<(String, bool, String)> = Vec::new();
        for language in Language::all() {
            names.push((language.id().to_string(), true, String::new()));
        }
        for language in Language::all() {
            for companion in language.companions() {
                if names.iter().any(|(id, _, _)| id == companion) {
                    continue;
                }
                names.push((companion.to_string(), false, language.id().to_string()));
            }
        }

        let mut results = Vec::with_capacity(names.len());
        for (id, is_language, owner) in names {
            if is_language {
                if let Some(language) = Language::parse_id(&id) {
                    results.push(self.status_with(language, cwd, with_version));
                }
            } else if let Some(language) = Language::parse_id(&owner) {
                results.push(self.companion_status_with(language, &id, with_version));
            }
        }
        results
    }

    /// 探测单个语言运行时。
    ///
    /// `with_version = false` 时跳过子进程探测（`--version`），只判断安装状态。
    pub fn status(&self, language: Language, cwd: &Path) -> ToolStatus {
        self.status_with(language, cwd, true)
    }

    pub fn status_with(&self, language: Language, cwd: &Path, with_version: bool) -> ToolStatus {
        let language_id = language.id();
        let managed = crate::supported_language_ids().contains(&language_id);
        let home = RuntimeHome::resolve().ok();

        // lania 自管版本优先：current 指针比 PATH 更“权威”，
        // 而且版本号就是目录名，不需要再跑一次 `--version`。
        let managed_current = home.as_ref().and_then(|home| {
            let state = RuntimeState::load(&home.state_path()).ok()?;
            let version = state.current_version(language_id)?.to_string();
            let version_dir = home.version_dir(language_id, &version);
            let meta = InstalledMeta::load(&version_dir).ok().flatten()?;
            let bin_dir = version_dir.join(&meta.bin_dir);
            // 用 shim 同款的入口解析，避免 `.\node` 这种带点号的路径出现在展示里。
            let probe = language
                .probe_binaries()
                .first()
                .copied()
                .unwrap_or_default();
            let binary = crate::shims::resolve_entry(&bin_dir, probe);
            std::path::Path::new(&binary)
                .is_file()
                .then(|| (version, PathBuf::from(binary)))
        });

        let installed_count = home
            .as_ref()
            .filter(|_| managed)
            .map(|home| count_versions(&home.language_dir(language_id)));

        let (binary_path, version, source, installed) = match managed_current {
            Some((version, binary)) => {
                (Some(binary), Some(version), Some("lania".to_string()), true)
            }
            None => {
                let binary = language
                    .probe_binaries()
                    .iter()
                    .find_map(|name| which(name));
                let installed = match language {
                    Language::CSharp | Language::FSharp if with_version => dotnet_has_sdk(),
                    _ => binary.is_some(),
                };
                let version = if installed && with_version {
                    binary
                        .as_deref()
                        .and_then(|path| read_version(language, path))
                } else {
                    None
                };
                let source = installed.then(|| "system".to_string());
                (binary, version, source, installed)
            }
        };

        ToolStatus {
            id: language_id.to_string(),
            name: language.display_name().to_string(),
            language: language_id.to_string(),
            installed,
            version,
            binary_path: binary_path.map(|path| path.display().to_string()),
            source,
            managed,
            upstream: managed.then(|| crate::provider::upstream_for(language_id).to_string()),
            installed_count,
            project_version: language
                .read_version_file(cwd)
                .map(|(file, version)| ProjectVersion { file, version }),
        }
    }

    /// 读取某个已安装语言的实际版本号（用于懒加载）。
    pub fn resolve_version(&self, language: Language) -> Option<String> {
        let binary = language
            .probe_binaries()
            .iter()
            .find_map(|name| which(name))?;
        read_version(language, &binary)
    }

    /// 读取附属工具（npm / pip / cargo …）的版本号（用于懒加载）。
    pub fn resolve_companion_version(&self, companion: &str) -> Option<String> {
        let binary = which(companion)?;
        read_npm_like_version(&binary)
    }

    fn companion_status_with(
        &self,
        language: Language,
        companion: &str,
        with_version: bool,
    ) -> ToolStatus {
        let binary = which(companion);
        // rustup 之类的 shim 即使组件未安装也会存在于 PATH 中，
        // 执行时会报 "is not installed for the toolchain"，必须判定为未安装。
        let broken_shim = binary.as_deref().is_some_and(is_missing_component);
        let version = if with_version && !broken_shim {
            binary.as_deref().and_then(read_npm_like_version)
        } else {
            None
        };
        ToolStatus {
            id: companion.to_string(),
            name: companion.to_string(),
            language: language.id().to_string(),
            installed: binary.is_some() && !broken_shim,
            version,
            binary_path: binary.map(|path| path.display().to_string()),
            source: None,
            managed: false,
            upstream: None,
            installed_count: None,
            project_version: None,
        }
    }

    /// 列出某个语言由 lania 管理的已安装版本（新 → 旧）。
    pub fn managed_versions(&self, language: Language) -> Vec<InstalledVersion> {
        let Ok(home) = RuntimeHome::resolve() else {
            return Vec::new();
        };
        let state = RuntimeState::load(&home.state_path()).ok();
        let current = state.and_then(|state| {
            state
                .current_version(language.id())
                .map(ToString::to_string)
        });
        let mut versions: Vec<InstalledVersion> =
            list_version_dirs(&home.language_dir(language.id()))
                .into_iter()
                .filter_map(|path| {
                    let name = path.file_name()?.to_string_lossy().to_string();
                    Some(InstalledVersion {
                        source: "lania".to_string(),
                        current: current.as_deref() == Some(name.as_str()),
                        version: name,
                    })
                })
                .collect();
        versions.sort_by(|a, b| {
            let left = crate::VersionTag::parse(&a.version).map(|tag| tag.parts);
            let right = crate::VersionTag::parse(&b.version).map(|tag| tag.parts);
            right.cmp(&left)
        });
        versions
    }
}

fn list_version_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join(crate::META_FILE).is_file())
        .collect()
}

fn count_versions(dir: &Path) -> usize {
    list_version_dirs(dir).len()
}

/// 在 PATH 中查找可执行文件。
///
/// 结果缓存 + 按需直查：只在 PATH 目录里尝试少量组合，
/// 而不是预先枚举整个 PATH（后者在 Windows 上要 stat 上万次）。
pub fn which(binary: &str) -> Option<PathBuf> {
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, Option<PathBuf>>>> =
        std::sync::Mutex::new(None);

    if let Ok(cache) = CACHE.lock() {
        if let Some(hit) = cache.as_ref().and_then(|map| map.get(binary)) {
            return hit.clone();
        }
    }

    let resolved = lookup_in_path_index(binary);
    if let Ok(mut cache) = CACHE.lock() {
        cache
            .get_or_insert_with(std::collections::HashMap::new)
            .insert(binary.to_string(), resolved.clone());
    }
    resolved
}

/// PATH 的完整索引：`文件名(小写) -> 命中路径`，按 PATH 顺序保留首个命中。
fn path_index() -> &'static std::collections::HashMap<String, PathBuf> {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    static INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map: HashMap<String, PathBuf> = HashMap::new();
        let Some(path_var) = std::env::var_os("PATH") else {
            return map;
        };
        for dir in std::env::split_paths(&path_var) {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.is_empty() {
                    continue;
                }
                // PATH 靠前的目录优先：保留首个命中。
                map.entry(name).or_insert_with(|| entry.path());
            }
        }
        map
    })
}

fn lookup_in_path_index(binary: &str) -> Option<PathBuf> {
    let index = path_index();
    let lower = binary.to_ascii_lowercase();
    let mut keys = vec![lower.clone()];
    if cfg!(windows) {
        // PATHEXT：优先 `.exe`，其次是 `.cmd` / `.bat` 这类脚本入口。
        for extension in [".exe", ".cmd", ".bat", ".ps1"] {
            keys.push(format!("{lower}{extension}"));
        }
    }
    keys.iter().find_map(|key| index.get(key).cloned())
}

fn read_version(language: Language, binary: &Path) -> Option<String> {
    // Dart 的 shim 启动极慢（首次要解包 SDK），优先从 SDK 目录读版本。
    if language == Language::Dart {
        if let Some(version) = dart_or_flutter_sdk_version(binary) {
            return Some(version);
        }
    }
    if is_expensive_probe(binary) {
        return None;
    }

    // 不同语言的版本查询方式差异很大，不能统一用 `--version`。
    for invocation in version_invocations(language) {
        let args: Vec<&str> = invocation.to_vec();
        let Some(text) = run_for_version(binary, &args) else {
            continue;
        };
        if is_error_output(&text) {
            continue;
        }
        if let Some(version) = crate::extract_semver(&text) {
            return Some(version);
        }
    }
    None
}

/// 每种语言的版本查询参数（按顺序尝试，直到拿到可解析的版本）。
fn version_invocations(language: Language) -> Vec<Vec<&'static str>> {
    match language {
        // `go version` 不带 `--`。
        Language::Go => vec![vec!["version"]],
        // `dotnet --version` 在缺 SDK 时报错，用 `--list-sdks` 才能拿到真实版本列表。
        Language::CSharp | Language::FSharp => vec![vec!["--list-sdks"]],
        // R 的可执行文件是大写 `R`，且用 `--version`。
        Language::R => vec![vec!["--version"]],
        // Erlang 没有 `--version`，需要 eval。
        Language::Erlang => vec![
            vec![
                "-eval",
                "erlang:display(erlang:system_info(otp_release)), halt().",
                "-noshell",
            ],
            vec!["-version"],
        ],
        // Java 8 用 `-version`，9+ 用 `--version`。
        Language::Java => vec![vec!["--version"], vec!["-version"]],
        // OCaml 的 `ocaml` 是可执行解释器，版本要用 `-version`。
        Language::OCaml => vec![vec!["-version"], vec!["--version"]],
        _ => vec![vec!["--version"]],
    }
}

fn read_npm_like_version(binary: &Path) -> Option<String> {
    // 少数工具（如 gofmt）没有 `--version`，退回 `-version` / `version` 探针。
    // 注意：像 `flutter --version` 这类启动很慢的命令会拖垮整体扫描，
    // 因此这里对已知的“慢探测”直接跳过（Dart/Flutter 版本另有轻量来源）。
    if is_expensive_probe(binary) {
        return dart_or_flutter_sdk_version(binary);
    }
    // 明确没有 `--version` 的工具：不去猜，避免把 help 文本里的数字当版本。
    let name = binary
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(
        name.as_str(),
        "gofmt" | "gofumpt" | "make" | "cmake" | "ninja" | "pkg-config" | "opam" | "kerl"
    ) {
        return None;
    }
    for flag in ["--version", "-version", "version"] {
        if let Some(text) = run_for_version(binary, &[flag]) {
            if is_error_output(&text) {
                continue;
            }
            if let Some(version) = crate::extract_semver(&text) {
                return Some(version);
            }
        }
    }
    None
}

/// 已知启动成本高的工具（首次运行需要解包 / 校验 SDK），跳过进程探测。
fn is_expensive_probe(binary: &Path) -> bool {
    let name = binary
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(name.as_str(), "flutter" | "dart" | "gradle" | "mvn")
}

/// 从 Flutter SDK 目录直接读版本（不启动进程）。
fn dart_or_flutter_sdk_version(binary: &Path) -> Option<String> {
    // SDK 布局：`<sdk>/bin/dart` 或 `<sdk>/bin/flutter`，版本在 `<sdk>/version`。
    let mut dir = binary.parent()?.to_path_buf();
    for _ in 0..3 {
        if let Ok(text) = std::fs::read_to_string(dir.join("version")) {
            if let Some(version) = crate::extract_semver(&text) {
                return Some(version);
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn run_for_version(binary: &Path, args: &[&str]) -> Option<String> {
    // 先尝试解析 shim 背后的真实可执行文件（POSIX 符号链接）。
    let mut targets: Vec<PathBuf> = Vec::new();
    if let Some(real) = resolve_real_binary(binary) {
        targets.push(real);
    }
    targets.push(binary.to_path_buf());

    for target in targets {
        for (program, prefix) in version_invocation(&target) {
            let mut command = Command::new(&program);
            command.args(&prefix).args(args);
            // 版本探测是「锦上添花」：任何卡住的工具都不该拖慢整个列表。
            let Some(output) = run_with_timeout(command, std::time::Duration::from_secs(5)) else {
                continue;
            };
            let text = String::from_utf8_lossy(&output.stdout).to_string()
                + &String::from_utf8_lossy(&output.stderr);
            // WSL 的 `bash` 会输出 UTF-16 乱码并返回非零；这类输出不能当作版本。
            if looks_like_garbage(&text) {
                continue;
            }
            if output.status.success() || !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    None
}

fn resolve_real_binary(binary: &Path) -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }
    std::fs::canonicalize(binary)
        .ok()
        .filter(|path| path != binary)
}

/// `.NET` 的「已安装」不能只看 `dotnet` 可执行文件存在与否：
/// 只装了 runtime 而没有 SDK 时，`dotnet --version` 会失败，
/// 此时不应把 C#/F# 标记为可用。
fn dotnet_has_sdk() -> bool {
    // `dotnet --list-sdks` 冷启动要 2~3 秒，C# 和 F# 会各查一次，这里整进程只执行一次。
    static CACHE: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

    if let Ok(cache) = CACHE.lock() {
        if let Some(hit) = *cache {
            return hit;
        }
    }

    let resolved = (|| {
        let binary = which("dotnet")?;
        let text = run_for_version(&binary, &["--list-sdks"])?;
        Some(text.lines().map(str::trim).any(|line| {
            !line.is_empty() && line.chars().next().is_some_and(|c| c.is_ascii_digit())
        }))
    })()
    .unwrap_or(false);

    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(resolved);
    }
    resolved
}

/// 带超时地执行子进程：超时后杀掉并返回 `None`。
fn run_with_timeout(
    mut command: Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    use std::process::Stdio;

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().ok()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

/// 识别明显不是版本输出的内容（WSL 报错、UTF-16 乱码）。
fn looks_like_garbage(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let suspicious = text
        .chars()
        .filter(|c| *c == '\u{FFFD}' || *c == '\0')
        .count();
    if suspicious * 20 > text.chars().count() {
        return true;
    }
    text.contains("wsl:")
}

/// 返回“如何调用这个可执行文件”的候选序列。
fn version_invocation(binary: &Path) -> Vec<(String, Vec<String>)> {
    let mut candidates = Vec::new();
    let path_text = binary.display().to_string();
    let lower = path_text.to_ascii_lowercase();

    if lower.ends_with(".cmd") || lower.ends_with(".bat") {
        candidates.push(("cmd".to_string(), vec!["/C".to_string(), path_text.clone()]));
    } else if cfg!(windows) && binary.extension().is_none() && is_posix_script(binary) {
        // npm/pnpm/yarn 的 shim 是 sh 脚本。优先 Git Bash：系统里的 `bash`
        // 可能被 WSL 接管，那会以完全不同的文件系统语义执行。
        let shells = git_bash_candidates();
        for shell in &shells {
            candidates.push((shell.clone(), vec![path_text.clone()]));
        }
        if shells.is_empty() {
            for shell in ["sh", "bash"] {
                candidates.push((shell.to_string(), vec![path_text.clone()]));
            }
        }
    }

    candidates.push((path_text, Vec::new()));
    candidates
}

/// Git for Windows 的 bash 常见安装位置（优先于 PATH 上的 `bash`）。
fn git_bash_candidates() -> Vec<String> {
    let mut out = Vec::new();
    for candidate in [
        "C:/Program Files/Git/bin/bash.exe",
        "C:/Program Files/Git/usr/bin/bash.exe",
        "C:/Program Files (x86)/Git/bin/bash.exe",
    ] {
        if Path::new(candidate).is_file() {
            out.push(candidate.to_string());
        }
    }
    out
}

fn is_posix_script(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.starts_with(b"#!")
}

/// 判断某个 shim 是否指向「未安装的组件」（例如 rustup 的 rustfmt / clippy）。
///
/// 只在 POSIX 且路径像 rustup shim 时才真正调用一次，避免给每次扫描加开销。
fn is_missing_component(binary: &Path) -> bool {
    if cfg!(windows) {
        return false;
    }
    let path = binary.display().to_string();
    if !(path.contains("/.cargo/bin/") || path.contains("/.rustup/")) {
        return false;
    }
    let Some(output) = run_with_timeout(
        {
            let mut command = Command::new(binary);
            command.arg("--version");
            command
        },
        std::time::Duration::from_secs(5),
    ) else {
        return false;
    };
    String::from_utf8_lossy(&output.stderr)
        .to_ascii_lowercase()
        .contains("is not installed for the toolchain")
}

/// 判断输出是否是错误信息（而不是版本信息）。
pub(crate) fn is_error_output(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("not possible to find")
        || lower.contains("not recognized")
        || lower.contains("command not found")
        || lower.contains("no such file")
        || lower.contains("is not recognized as")
        // rustup 组件缺失时的提示：`error: 'rustfmt' is not installed for the toolchain ...`
        || lower.contains("is not installed for the toolchain")
        || lower.contains("help: run `rustup component add")
        // `/bin/sh: 1: nvm: not found` —— 缺少 shell 函数时的典型输出。
        || lower.contains(": not found")
        || lower.starts_with("error:")
        || lower.contains("usage:")
        || lower.contains("unrecognized option")
        || lower.contains("unknown option")
}

/// 从任意文本里抽出第一个语义化版本号。
pub fn extract_semver(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        // 版本必须以「数字.数字」开头，避免把日期、端口号当成版本。
        let start = index;
        while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.') {
            index += 1;
        }
        let candidate: String = chars[start..index].iter().collect();
        let candidate = candidate.trim_end_matches('.');
        if candidate.contains('.') && !candidate.starts_with('.') {
            return Some(candidate.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_outputs_are_recognized() {
        assert!(is_error_output("bash: nvm: command not found"));
        assert!(is_error_output(
            "error: 'rustfmt' is not installed for the toolchain"
        ));
        assert!(!is_error_output("v20.11.1"));
    }

    #[test]
    fn garbage_detection_catches_utf16() {
        assert!(looks_like_garbage("wsl: 检测到 localhost"));
        assert!(looks_like_garbage("\u{0}\u{0}\u{0}\u{0}\u{FFFD}\u{FFFD}"));
        assert!(!looks_like_garbage("node v20.11.1"));
    }

    #[test]
    fn extracts_semver_from_common_outputs() {
        assert_eq!(extract_semver("node v20.11.1").as_deref(), Some("20.11.1"));
        assert_eq!(
            extract_semver("go version go1.22.5 linux/amd64").as_deref(),
            Some("1.22.5")
        );
        assert_eq!(extract_semver("no version here"), None);
    }
}
