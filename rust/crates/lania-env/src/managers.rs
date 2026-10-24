//! 版本管理器（nvm / pyenv / rustup / sdkman …）的描述与命令拼装。
//!
//! 每个管理器只声明三件事：
//! 1. 如何在系统里“探测”它是否可用（`detect`）。
//! 2. 安装 / 卸载 / 切换某个版本时，应该执行什么命令（`plan_*`）。
//! 3. 它把版本装到哪里（用于 GUI 展示）。

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::Language;

/// 受支持的版本管理器。新增管理器只需在这里加一个变体并补齐匹配分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagerKind {
    // Node 生态
    Nvm,
    Fnm,
    Volta,
    N,
    Bunup,
    Denoup,
    // 通用 / 多语言
    Asdf,
    Mise,
    // Python
    Pyenv,
    Uv,
    // Rust
    Rustup,
    // Go
    GoInstall,
    // Dart / Flutter
    Fvm,
    // JVM
    Sdkman,
    // 其他语言专用
    ZigVm,
    Coursier,
    Dotnet,
    Swiftenv,
    Rbenv,
    Rvm,
    Phpenv,
    Plenv,
    Luaver,
    KerL,
    GhcUp,
    Juliaup,
    Rig,
    Choosenim,
    Opam,
    Cljup,
    // 新兴语言专用
    Moonup,
    Cangjieup,
    Deveco,
    Foundryup,
    Vup,
    Haxelihx,
    // 系统包管理器
    Scoop,
    Winget,
    Brew,
    Apt,
}

impl ManagerKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Nvm => "nvm",
            Self::Fnm => "fnm",
            Self::Volta => "volta",
            Self::N => "n",
            Self::Bunup => "bun",
            Self::Denoup => "deno",
            Self::Asdf => "asdf",
            Self::Mise => "mise",
            Self::Pyenv => "pyenv",
            Self::Uv => "uv",
            Self::Rustup => "rustup",
            Self::GoInstall => "go-install",
            Self::Fvm => "fvm",
            Self::Sdkman => "sdkman",
            Self::ZigVm => "zig",
            Self::Coursier => "coursier",
            Self::Dotnet => "dotnet",
            Self::Swiftenv => "swiftenv",
            Self::Rbenv => "rbenv",
            Self::Rvm => "rvm",
            Self::Phpenv => "phpenv",
            Self::Plenv => "plenv",
            Self::Luaver => "luaver",
            Self::KerL => "kerl",
            Self::GhcUp => "ghcup",
            Self::Juliaup => "juliaup",
            Self::Rig => "rig",
            Self::Choosenim => "choosenim",
            Self::Opam => "opam",
            Self::Cljup => "cljup",
            Self::Moonup => "moon",
            Self::Cangjieup => "cangjie",
            Self::Deveco => "deveco",
            Self::Foundryup => "foundry",
            Self::Vup => "v",
            Self::Haxelihx => "haxelib",
            Self::Scoop => "scoop",
            Self::Winget => "winget",
            Self::Brew => "brew",
            Self::Apt => "apt",
        }
    }

    /// 该管理器可以管理的语言。
    pub fn languages(self) -> Vec<Language> {
        Language::all()
            .into_iter()
            .filter(|language| language.managers().contains(&self))
            .collect()
    }

    /// 安装/卸载是否需要联网下载（用于 GUI 提示与 dry-run 说明）。
    pub fn downloads_binary(self) -> bool {
        !matches!(self, Self::Scoop | Self::Winget | Self::Brew | Self::Apt)
    }

    pub fn parse_id(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        Some(match value.as_str() {
            "nvm" => Self::Nvm,
            "fnm" => Self::Fnm,
            "volta" => Self::Volta,
            "n" => Self::N,
            "bun" => Self::Bunup,
            "deno" => Self::Denoup,
            "asdf" => Self::Asdf,
            "mise" => Self::Mise,
            "pyenv" => Self::Pyenv,
            "uv" => Self::Uv,
            "rustup" => Self::Rustup,
            "go" | "go-install" => Self::GoInstall,
            "fvm" => Self::Fvm,
            "sdkman" => Self::Sdkman,
            "zig" | "zvm" => Self::ZigVm,
            "coursier" | "cs" => Self::Coursier,
            "dotnet" => Self::Dotnet,
            "swiftenv" => Self::Swiftenv,
            "rbenv" => Self::Rbenv,
            "rvm" => Self::Rvm,
            "phpenv" => Self::Phpenv,
            "plenv" => Self::Plenv,
            "luaver" => Self::Luaver,
            "kerl" => Self::KerL,
            "ghcup" => Self::GhcUp,
            "juliaup" => Self::Juliaup,
            "rig" => Self::Rig,
            "choosenim" => Self::Choosenim,
            "opam" => Self::Opam,
            "cljup" => Self::Cljup,
            "moon" | "moonbit" => Self::Moonup,
            "cangjie" | "cjc" => Self::Cangjieup,
            "deveco" | "harmony" | "ohpm" => Self::Deveco,
            "foundry" | "foundryup" => Self::Foundryup,
            "v" | "vup" => Self::Vup,
            "haxelib" | "haxe" => Self::Haxelihx,
            "scoop" => Self::Scoop,
            "winget" => Self::Winget,
            "brew" => Self::Brew,
            "apt" => Self::Apt,
            _ => return None,
        })
    }
}

/// 探测结果：管理器是否可用，以及它的可执行入口与版本安装根目录。
#[derive(Debug, Clone, Serialize)]
pub struct ManagerProbe {
    pub manager: ManagerKind,
    pub available: bool,
    pub path: Option<String>,
    pub root: Option<String>,
    pub note: Option<String>,
}

/// 预热：把「被各语言实际引用到的」管理器探测结果写入缓存。
pub fn warm_up() {
    // 只探测「各语言实际引用到」的管理器（去重后约 20 个），
    // 而不是写死全部 34 个，避免为用不到的管理器付出 PATH 查找成本。
    //
    // 每个探测都是一次 PATH 查找（Windows 上 `Get-Command` 约 100ms），
    // 串行会累计到数秒，因此这里并行探测。
    let mut seen = std::collections::HashSet::new();
    let mut pending: Vec<ManagerKind> = Vec::new();
    for language in Language::all() {
        for manager in language.managers() {
            if seen.insert(manager.id()) {
                pending.push(manager);
            }
        }
    }

    let threads = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4)
        .clamp(1, 8);
    if pending.len() <= 1 || threads == 1 {
        for manager in pending {
            let _ = probe(manager);
        }
        return;
    }

    // 按轮次切分，每轮并行探测一批；`probe` 内部用 RwLock 缓存，天然线程安全。
    for chunk in pending.chunks(threads) {
        std::thread::scope(|scope| {
            for manager in chunk {
                scope.spawn(move || {
                    let _ = probe(*manager);
                });
            }
        });
    }
}

/// 探测某个管理器是否在本机可用。
///
/// 结果按进程缓存：一次扫描会对同一管理器重复探测十几次
/// （例如 nvm 出现在 node/asdf/mise 等多个语言的候选列表里）。
pub fn probe(manager: ManagerKind) -> ManagerProbe {
    static CACHE: std::sync::RwLock<Option<std::collections::HashMap<&'static str, ManagerProbe>>> =
        std::sync::RwLock::new(None);

    let key = manager.id();
    // 读路径用 `RwLock`：绝大部分调用都是缓存命中，读锁可并发。
    if let Ok(cache) = CACHE.read() {
        if let Some(hit) = cache.as_ref().and_then(|map| map.get(key)) {
            return hit.clone();
        }
    }

    let resolved = probe_uncached(manager);
    if let Ok(mut cache) = CACHE.write() {
        cache
            .get_or_insert_with(std::collections::HashMap::new)
            .insert(key, resolved.clone());
    }
    resolved
}

/// 一次性取出一组管理器的探测结果（只加一次锁）。
pub fn probe_many(managers: &[ManagerKind]) -> Vec<ManagerProbe> {
    managers.iter().map(|manager| probe(*manager)).collect()
}

fn probe_uncached(manager: ManagerKind) -> ManagerProbe {
    match manager {
        ManagerKind::Nvm => probe_nvm(),
        ManagerKind::Pyenv => probe_binary(manager, "pyenv", ".pyenv", &["PYENV_ROOT"]),
        ManagerKind::Rustup => probe_binary(manager, "rustup", ".rustup", &["RUSTUP_HOME"]),
        ManagerKind::Fnm => probe_binary(manager, "fnm", ".fnm", &["FNM_DIR"]),
        ManagerKind::Volta => probe_binary(manager, "volta", ".volta", &["VOLTA_HOME"]),
        ManagerKind::Asdf => probe_binary(manager, "asdf", ".asdf", &["ASDF_DATA_DIR", "ASDF_DIR"]),
        ManagerKind::Mise => probe_binary(manager, "mise", ".local/share/mise", &["MISE_DATA_DIR"]),
        ManagerKind::Uv => probe_binary(manager, "uv", ".local/share/uv", &["UV_DATA_DIR"]),
        ManagerKind::Fvm => probe_binary(manager, "fvm", "fvm", &["FVM_HOME"]),
        ManagerKind::Sdkman => probe_sdkman(),
        ManagerKind::Scoop => probe_binary(manager, "scoop", "scoop", &["SCOOP"]),
        ManagerKind::Winget => probe_binary(manager, "winget", "", &[]),
        ManagerKind::Brew => probe_binary(manager, "brew", "", &["HOMEBREW_PREFIX"]),
        ManagerKind::Apt => probe_binary(manager, "apt-get", "", &[]),
        ManagerKind::N => probe_binary(manager, "n", "", &["N_PREFIX"]),
        ManagerKind::GoInstall => probe_go(),
        // 其余管理器统一按「可执行文件 + 数据目录」探测。
        ManagerKind::Bunup => probe_binary(manager, "bun", ".bun", &["BUN_INSTALL"]),
        ManagerKind::Denoup => probe_binary(manager, "deno", ".deno", &["DENO_INSTALL"]),
        ManagerKind::ZigVm => probe_binary(manager, "zig", "", &["ZIG_GLOBAL_CACHE_DIR"]),
        ManagerKind::Coursier => {
            probe_binary(manager, "cs", ".local/share/coursier", &["COURSIER_HOME"])
        }
        ManagerKind::Dotnet => probe_binary(manager, "dotnet", ".dotnet", &["DOTNET_ROOT"]),
        ManagerKind::Swiftenv => probe_binary(manager, "swiftenv", ".swiftenv", &["SWIFTENV_ROOT"]),
        ManagerKind::Rbenv => probe_binary(manager, "rbenv", ".rbenv", &["RBENV_ROOT"]),
        ManagerKind::Rvm => probe_binary(manager, "rvm", ".rvm", &["RVM_HOME"]),
        ManagerKind::Phpenv => probe_binary(manager, "phpenv", ".phpenv", &["PHPENV_ROOT"]),
        ManagerKind::Plenv => probe_binary(manager, "plenv", ".plenv", &["PLENV_ROOT"]),
        ManagerKind::Luaver => probe_binary(manager, "luaver", ".luaver", &["LUAVER_DIR"]),
        ManagerKind::KerL => probe_binary(manager, "kerl", ".kerl", &["KERL_BASE_DIR"]),
        ManagerKind::GhcUp => {
            probe_binary(manager, "ghcup", ".ghcup", &["GHCUP_INSTALL_BASE_PREFIX"])
        }
        ManagerKind::Juliaup => {
            probe_binary(manager, "juliaup", ".juliaup", &["JULIAUP_DEPOT_PATH"])
        }
        ManagerKind::Rig => probe_binary(manager, "rig", ".local/share/R", &[]),
        ManagerKind::Choosenim => probe_binary(
            manager,
            "choosenim",
            ".choosenim",
            &["CHOOSENIM_CHOOSE_VERSION"],
        ),
        ManagerKind::Opam => probe_binary(manager, "opam", ".opam", &["OPAMROOT"]),
        ManagerKind::Cljup => probe_binary(manager, "clj", ".cljup", &[]),
        // 新兴语言：官方工具链自带版本管理，探测对应的 CLI 入口。
        ManagerKind::Moonup => probe_binary(manager, "moon", ".moon", &["MOON_HOME"]),
        ManagerKind::Cangjieup => probe_binary(manager, "cjc", ".cangjie", &["CANGJIE_HOME"]),
        ManagerKind::Deveco => probe_binary(manager, "ohpm", ".ohpm", &["DEVECO_SDK_HOME"]),
        ManagerKind::Foundryup => probe_binary(manager, "forge", ".foundry", &["FOUNDRY_DIR"]),
        ManagerKind::Vup => probe_binary(manager, "v", ".vmodules", &["VMODULES"]),
        ManagerKind::Haxelihx => probe_binary(manager, "haxelib", "haxelib", &["HAXELIB_PATH"]),
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn expanded_hint(root: &str, check_exists: bool) -> Option<PathBuf> {
    if root.is_empty() {
        return None;
    }
    let home = home_dir()?;
    let path = home.join(root);
    if check_exists && !path.exists() {
        return None;
    }
    Some(path)
}

fn probe_binary(
    manager: ManagerKind,
    binary: &str,
    default_root: &str,
    root_envs: &[&str],
) -> ManagerProbe {
    let found = which(binary);
    // 注意：`platform_root` / `expanded_hint` 都可能返回不存在的路径，
    // 这里统一做一次 `exists()` 判断即可，避免重复的磁盘查询。
    let root = root_envs
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .or_else(|| platform_root(manager, false))
        .or_else(|| expanded_hint(default_root, false))
        .filter(|path| path.exists())
        .map(|path| path.display().to_string());
    ManagerProbe {
        manager,
        available: found.is_some(),
        path: found.map(|path| path.display().to_string()),
        root,
        note: None,
    }
}

/// 某些管理器在特定平台上不使用 `~/.<name>` 布局，需要单独的默认根目录。
///
/// 典型例子：Volta 在 Windows 上安装到 `%LOCALAPPDATA%\Volta`，而在
/// Linux/macOS 上才是 `~/.volta`。
fn platform_root(manager: ManagerKind, check_exists: bool) -> Option<PathBuf> {
    let candidate = if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
        let userprofile = std::env::var_os("USERPROFILE").map(PathBuf::from);

        match manager {
            ManagerKind::Volta => local?.join("Volta"),
            ManagerKind::Fnm => local?.join("fnm"),
            // uv 在 Windows 上把数据目录放在 `%APPDATA%\uv`（而非 `~/.local/share/uv`）。
            ManagerKind::Uv => roaming?.join("uv"),
            ManagerKind::Denoup => local?.join("deno"),
            ManagerKind::Bunup => userprofile?.join(".bun"),
            ManagerKind::Pyenv | ManagerKind::Rbenv | ManagerKind::Phpenv | ManagerKind::Plenv => {
                userprofile?.join(format!(".{}", manager.id()))
            }
            ManagerKind::Dotnet => PathBuf::from(
                std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into()),
            )
            .join("dotnet"),
            _ => return None,
        }
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        match manager {
            // macOS 上 uv 遵循 XDG，落在 `~/.local/share/uv`；
            // 而 Volta 在 macOS 是 `~/.volta`（与 Linux 一致）。
            ManagerKind::Volta => home?.join(".volta"),
            ManagerKind::Uv => home?.join(".local").join("share").join("uv"),
            ManagerKind::Fnm => home?.join(".local").join("share").join("fnm"),
            ManagerKind::Denoup => home?.join(".deno"),
            ManagerKind::Bunup => home?.join(".bun"),
            ManagerKind::Pyenv
            | ManagerKind::Rbenv
            | ManagerKind::Phpenv
            | ManagerKind::Plenv
            | ManagerKind::Sdkman
            | ManagerKind::Nvm => home?.join(format!(".{}", manager.id())),
            ManagerKind::Dotnet => home?.join(".dotnet"),
            _ => return None,
        }
    };
    if check_exists && !candidate.exists() {
        return None;
    }
    Some(candidate)
}

fn probe_nvm() -> ManagerProbe {
    let binary = which("nvm").or_else(|| which("nvm.exe"));
    let root_env = std::env::var("NVM_DIR")
        .ok()
        .filter(|value| !value.is_empty());
    let root = root_env
        .map(PathBuf::from)
        .or_else(|| expanded_hint(".nvm", false))
        .filter(|path| path.exists());
    // nvm 在 Windows 上通常是 nvm-windows，可执行文件本身就在 PATH 中；
    // 在 POSIX 上它是 shell 函数，需通过 `bash -lc 'nvm'` 才能探测到。
    let shell_available = !cfg!(windows) && detect_posix_shell_function("nvm");
    ManagerProbe {
        manager: ManagerKind::Nvm,
        available: binary.is_some() || shell_available || root.is_some(),
        path: binary
            .map(|path| path.display().to_string())
            .or_else(|| root.as_ref().map(|path| path.display().to_string())),
        root: root.map(|path| path.display().to_string()),
        note: if shell_available {
            Some("nvm 是 shell 函数，将通过 bash -lc 调用".into())
        } else {
            None
        },
    }
}

fn probe_sdkman() -> ManagerProbe {
    let root = std::env::var("SDKMAN_DIR")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| expanded_hint(".sdkman", false))
        .filter(|path| path.exists());
    let init = root
        .as_ref()
        .map(|path| path.join("bin").join("sdkman-init.sh"));
    let available = init.as_ref().is_some_and(|path| path.is_file()) || which("sdk").is_some();
    ManagerProbe {
        manager: ManagerKind::Sdkman,
        available,
        path: init
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string()),
        root: root.map(|path| path.display().to_string()),
        note: Some("sdkman 通过 sdkman-init.sh 提供 shell 函数".into()),
    }
}

fn probe_go() -> ManagerProbe {
    let go = which("go");
    let root = std::env::var("GOROOT")
        .ok()
        .filter(|value| !value.is_empty());
    ManagerProbe {
        manager: ManagerKind::GoInstall,
        available: go.is_some(),
        path: go.map(|path| path.display().to_string()),
        root,
        note: Some("使用 go 官方 toolchain（go install / GOTOOLCHAIN）".into()),
    }
}

fn detect_posix_shell_function(name: &str) -> bool {
    Command::new("bash")
        .args(["-lc", &format!("command -v {name}")])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// 在 PATH 中查找可执行文件（Windows 会补齐 PATHEXT 后缀）。
///
/// Windows 上还要处理**无扩展名 shim**（例如 npm 安装的 `npm`、`pnpm`），
/// 这类文件是 posix sh 脚本而非 PE 可执行文件，但确实是合法的命令入口。
///
/// 性能策略：
/// - 结果缓存（同名只查一次）
/// - **按需直查**：只在 PATH 目录里尝试 `name` / `name.EXE` 等少量组合，
///   而不是预先枚举整个 PATH（后者在 Windows 上要 stat 上万次，约 1 秒）。
pub fn which(binary: &str) -> Option<PathBuf> {
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, Option<PathBuf>>>> =
        std::sync::Mutex::new(None);

    if let Ok(cache) = CACHE.lock() {
        if let Some(hit) = cache.as_ref().and_then(|map| map.get(binary)) {
            return hit.clone();
        }
    }

    // 索引已覆盖整个 PATH；未命中就说明 PATH 里没有这个可执行文件。
    // 之前的 `which_direct` 回退会再遍历一遍所有目录（Windows 上还要叠加
    // PATHEXT 组合），未安装的工具每个都要白跑上百次 `is_file`，
    // 60 个工具累计近 1.5 秒。
    let resolved = lookup_in_path_index(binary);
    if let Ok(mut cache) = CACHE.lock() {
        cache
            .get_or_insert_with(std::collections::HashMap::new)
            .insert(binary.to_string(), resolved.clone());
    }
    resolved
}

/// PATH 的完整索引：`文件名(小写) -> 命中路径`，按 PATH 顺序保留首个命中。
///
/// 逐次 `which_direct` 要为每个候选名遍历所有 PATH 目录，Windows 上还要
/// 叠加 PATHEXT 组合（20 目录 × 6 扩展 ≈ 120 次 `is_file`）。扫描 60+ 个工具
/// 就是上万次系统调用，实测占用近 2 秒。改成索引一次、查表命中。
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
                // 只关心文件；符号链接（Linux 上的 shim）`is_file()` 也会为 true。
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.is_empty() {
                    continue;
                }
                let path = entry.path();
                // PATH 靠前的目录优先：保留首个命中。
                map.entry(name).or_insert(path);
            }
        }
        map
    })
}

fn lookup_in_path_index(binary: &str) -> Option<PathBuf> {
    let index = path_index();
    let lower = binary.to_ascii_lowercase();

    // 无扩展名的 shim 优先（npm/pnpm 在 Windows 上就是这种形式）。
    if let Some(path) = index.get(&lower) {
        if path.is_file() {
            return Some(path.clone());
        }
    }
    if cfg!(windows) && Path::new(binary).extension().is_none() {
        let pathext =
            std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        for ext in pathext.split(';').filter(|value| !value.is_empty()) {
            let key = format!("{}{}", lower, ext.to_ascii_lowercase());
            if let Some(path) = index.get(&key) {
                if path.is_file() {
                    return Some(path.clone());
                }
            }
        }
    }
    None
}

/// 逐目录探测可执行文件。
///
/// 现在只在构建 PATH 索引失败（极少见）时作为兜底使用；正常路径全部走
/// [`lookup_in_path_index`]。保留它是为了避免索引不可用时功能直接失效。
#[allow(dead_code)]
fn which_direct(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let has_ext = Path::new(binary).extension().is_some();
    let extensions: Vec<String> = if cfg!(windows) && !has_ext {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
            .collect()
    } else {
        Vec::new()
    };

    for dir in std::env::split_paths(&path_var) {
        // 无扩展名（shim）优先：npm/pnpm 在 Windows 上就是这种形式。
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
        for ext in &extensions {
            let with_ext = dir.join(format!("{binary}{ext}"));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }
    None
}

/// 预热：把全部管理器探测结果填进缓存（串行执行，避免并行争用）。
/// 安装根目录的推荐展示路径（即使管理器未安装也给出预期位置）。
pub fn install_location_hint(manager: ManagerKind) -> Option<String> {
    probe(manager).root
}
