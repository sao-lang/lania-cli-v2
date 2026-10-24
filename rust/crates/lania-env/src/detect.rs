//! 工具链状态探测：当前版本、可执行路径、命中哪个管理器、项目版本约束。
//!
//! 这一层只读，不做任何修改，是 GUI 与 CLI 共用的数据源。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use lania_exec::{ExecCommand, ExecService};
use serde::Serialize;

use crate::{Language, ManagerKind, ManagerProbe};

/// 单个受管工具（语言或附属包管理器）的完整状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    pub name: String,
    pub language: String,
    /// 是否在 PATH 中找到了可执行文件。
    pub installed: bool,
    pub version: Option<String>,
    pub binary_path: Option<String>,
    /// 命中的版本管理器（None 表示系统自带 / 未知来源）。
    pub manager: Option<String>,
    /// 该工具可用的管理器清单（含是否可用）。
    pub candidates: Vec<ManagerProbe>,
    /// 项目目录中的版本约束文件与内容。
    pub project_version: Option<ProjectVersion>,
    /// 安装根目录提示（用于 GUI 展示）。
    pub install_root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVersion {
    pub file: String,
    pub version: String,
}

/// 一条「已安装版本」记录：版本号 + 它由哪个管理器安装。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    pub manager: String,
    pub version: String,
}

/// 环境探测服务。
#[derive(Debug, Clone, Default)]
pub struct EnvService;

impl EnvService {
    pub fn new() -> Self {
        Self
    }

    /// 探测全部受支持工具（语言 + 附属包管理器）。
    ///
    /// 每个工具的探测都要执行子进程（`--version`），串行会累计到数秒；
    /// 这里用线程池并行，把总耗时压到「最慢的单个工具」量级。
    pub fn scan(&self, cwd: &Path) -> Vec<ToolStatus> {
        self.scan_with(cwd, true)
    }

    /// 同 [`scan`]，但可跳过版本号探测（`with_version = false`）。
    ///
    /// TUI 首屏用 `false` 先把列表画出来，之后按需对选中项调用
    /// [`resolve_version`] 补全版本，避免打开界面时卡住几秒。
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

        // 先用串行预热把管理器探测结果全部写入缓存（各只做一次），
        // 之后每个工具的 `status` 只是读缓存，不再有磁盘/锁争用。
        crate::managers::warm_up();

        // 预热完成后串行处理即可：此时瓶颈是 `--version` 子进程，
        // 并行收益有限，反而会因线程创建与缓存争用而变慢。
        let mut results = Vec::with_capacity(names.len());
        for (id, is_language, owner) in names {
            if is_language {
                if let Some(language) = Language::parse_id(&id) {
                    results.push(self.status_with(language, cwd, with_version));
                }
            } else if let Some(language) = Language::parse_id(&owner) {
                results.push(self.companion_status_with(language, &id, cwd, with_version));
            }
        }
        results
    }

    /// 探测单个语言运行时。
    ///
    /// `with_version = false` 时跳过子进程探测（`--version`），只判断安装状态。
    /// 这样打开 TUI 或快速浏览时不会被几十个 `--version` 拖慢。
    pub fn status(&self, language: Language, cwd: &Path) -> ToolStatus {
        self.status_with(language, cwd, true)
    }

    pub fn status_with(&self, language: Language, cwd: &Path, with_version: bool) -> ToolStatus {
        let binary_path = language
            .probe_binaries()
            .iter()
            .find_map(|name| crate::managers::which(name));

        // `.NET` 系列只有装了 SDK 才真正可用来编译：只装 runtime 时
        // `dotnet` 存在但 `--list-sdks` 为空，此时应视为「未安装」。
        //
        // `dotnet --list-sdks` 是慢探测（冷启动近 1 秒），首屏扫描
        // （`with_version = false`）先用「可执行文件存在」占位，
        // 真实结果由后台补齐阶段（[`resolve_version_at`]）修正。
        let installed = match language {
            Language::CSharp | Language::FSharp if with_version => dotnet_has_sdk(),
            _ => binary_path.is_some(),
        };

        let version = if installed && with_version {
            binary_path
                .as_deref()
                .and_then(|path| read_version(language, path))
        } else {
            None
        };

        let candidates: Vec<ManagerProbe> = language
            .managers()
            .into_iter()
            .map(crate::managers::probe)
            .collect();
        let manager = binary_path
            .as_deref()
            .and_then(|path| infer_manager(language, path, &candidates));
        ToolStatus {
            id: language.id().to_string(),
            name: language.display_name().to_string(),
            language: language.id().to_string(),
            installed,
            version,
            binary_path: binary_path.map(|path| path.display().to_string()),
            manager,
            install_root: best_install_root(&candidates),
            candidates,
            project_version: language
                .read_version_file(cwd)
                .map(|(file, version)| ProjectVersion { file, version }),
        }
    }

    /// 读取某个已安装语言的实际版本号（用于懒加载）。
    pub fn resolve_version(&self, language: Language) -> Option<String> {
        let binary_path = language
            .probe_binaries()
            .iter()
            .find_map(|name| crate::managers::which(name))?;
        read_version(language, &binary_path)
    }

    /// 读取附属工具（npm / pip / cargo …）的版本号（用于懒加载）。
    pub fn resolve_companion_version(&self, companion: &str) -> Option<String> {
        let binary_path = crate::managers::which(companion)?;
        read_npm_like_version(&binary_path)
    }

    fn companion_status_with(
        &self,
        language: Language,
        companion: &str,
        _cwd: &Path,
        with_version: bool,
    ) -> ToolStatus {
        let binary_path = crate::managers::which(companion);
        // rustup 之类的 shim 即使组件未安装也会存在于 PATH 中，
        // 执行时会报 “is not installed for the toolchain”。这种情况下
        // 必须判定为未安装，否则界面会显示「已安装」但版本永远拿不到。
        let broken_shim = binary_path
            .as_deref()
            .is_some_and(|path| is_missing_component(path));
        let version = if with_version && !broken_shim {
            binary_path
                .as_deref()
                .and_then(|path| read_npm_like_version(path))
        } else {
            None
        };
        let candidates: Vec<ManagerProbe> = language
            .managers()
            .into_iter()
            .map(crate::managers::probe)
            .collect();
        ToolStatus {
            id: companion.to_string(),
            name: companion.to_string(),
            language: language.id().to_string(),
            installed: binary_path.is_some() && !broken_shim,
            version,
            binary_path: binary_path.map(|path| path.display().to_string()),
            manager: None,
            install_root: best_install_root(&candidates),
            candidates,
            project_version: None,
        }
    }

    /// 列出某个管理器已安装的版本（用于 GUI 的“已装版本”列表）。
    pub fn installed_versions(&self, manager: ManagerKind) -> Vec<String> {
        self.installed_versions_for(manager, None)
    }

    /// 同 [`installed_versions`]，但可指定语言以便按 runtime 过滤（Volta 需要）。
    pub fn installed_versions_for(
        &self,
        manager: ManagerKind,
        language: Option<crate::Language>,
    ) -> Vec<String> {
        let probe = crate::managers::probe(manager);
        match manager {
            ManagerKind::Nvm => nvm_versions(&probe),
            ManagerKind::Pyenv => dir_versions(probe.root.as_deref(), "versions"),
            ManagerKind::Rustup => rustup_toolchains(),
            ManagerKind::Fnm => fnm_list(&probe),
            // Volta 的布局是 `tools/image/<runtime>/<version>/`，必须按 runtime 过滤。
            ManagerKind::Volta => volta_versions(&probe, language),
            ManagerKind::Asdf => dir_versions(probe.root.as_deref(), "installs"),
            ManagerKind::Mise => dir_versions(probe.root.as_deref(), "installs"),
            // uv 的目录名形如 `cpython-3.14.2-windows-x86_64-none`，需要抽出语义版本。
            ManagerKind::Uv => uv_python_versions(&probe),
            ManagerKind::Fvm => dir_versions(probe.root.as_deref(), "versions"),
            ManagerKind::Sdkman => sdkman_versions(probe.root.as_deref()),
            ManagerKind::GoInstall => go_toolchains(&probe),
            ManagerKind::Rbenv => dir_versions(probe.root.as_deref(), "versions"),
            ManagerKind::Rvm => dir_versions(probe.root.as_deref(), "rubies"),
            ManagerKind::Phpenv => dir_versions(probe.root.as_deref(), "versions"),
            ManagerKind::Plenv => dir_versions(probe.root.as_deref(), "versions"),
            ManagerKind::Swiftenv => dir_versions(probe.root.as_deref(), "versions"),
            ManagerKind::KerL => dir_versions(probe.root.as_deref(), "releases"),
            ManagerKind::GhcUp => dir_versions(probe.root.as_deref(), "ghc"),
            ManagerKind::Opam => dir_versions(probe.root.as_deref(), "switches"),
            ManagerKind::Dotnet => dotnet_sdks(),
            ManagerKind::Juliaup => juliaup_channels(),
            ManagerKind::Bunup => bun_versions(),
            ManagerKind::Denoup => deno_versions(),
            _ => Vec::new(),
        }
    }

    /// 列出「该语言由任何已知管理器安装的全部版本」。
    ///
    /// 与 `installed_versions` 的区别：这里会遍历该语言的所有候选管理器，
    /// 因此即使当前 `node` 来自系统安装，也能列出 nvm/volta 里装过的版本。
    pub fn versions_for_language(&self, language: crate::Language) -> Vec<InstalledVersion> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for manager in language.managers() {
            let probe = crate::managers::probe(manager);
            if !probe.available {
                continue;
            }
            for version in self.installed_versions_for(manager, Some(language)) {
                let key = format!("{}/{}", manager.id(), version);
                if seen.insert(key) {
                    out.push(InstalledVersion {
                        manager: manager.id().to_string(),
                        version,
                    });
                }
            }
        }
        // 系统包管理器（apt/dnf/pacman/brew…）没有「列出本工具所有版本」的概念，
        // 因此上面的循环拿不到任何条目。此时把当前正在使用的版本补进去，
        // 否则详情页会显示「已安装版本 (0)」，与左栏的「已安装」自相矛盾。
        if out.is_empty() {
            if let Some(version) = self.resolve_version(language) {
                out.push(InstalledVersion {
                    manager: "system".to_string(),
                    version,
                });
            }
        }
        out
    }

    /// 通过 `lania-exec` 执行一个动作步骤（调用方负责 dry-run 判断）。
    pub fn run_step(
        &self,
        exec: &ExecService,
        cwd: &Path,
        step: &crate::PlanStep,
    ) -> Result<String> {
        let command = if step.use_shell {
            // shell 函数步骤：脚本已在 `PlanStep::shell_function` 中做过 token 白名单校验。
            let mut cmd = ExecCommand::new(step.program.clone());
            cmd = cmd.with_args(step.args.clone());
            cmd.use_shell = true;
            cmd
        } else {
            ExecCommand::new(step.program.clone()).with_args(step.args.clone())
        }
        .in_dir(cwd.display().to_string());
        let result = exec.run(command)?;
        Ok(result.stdout)
    }
}

fn best_install_root(candidates: &[ManagerProbe]) -> Option<String> {
    candidates
        .iter()
        .find(|probe| probe.available)
        .and_then(|probe| probe.root.clone())
        .or_else(|| candidates.iter().find_map(|probe| probe.root.clone()))
}

fn infer_manager(language: Language, binary: &Path, candidates: &[ManagerProbe]) -> Option<String> {
    let normalized = binary
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let matched = candidates.iter().find(|probe| {
        probe
            .root
            .as_deref()
            .map(|root| {
                let root = root.replace('\\', "/").to_ascii_lowercase();
                !root.is_empty() && normalized.starts_with(&root)
            })
            .unwrap_or(false)
    });
    matched
        .map(|probe| probe.manager.id().to_string())
        .or_else(|| marker_based_manager(language, &normalized))
}

fn marker_based_manager(language: Language, path: &str) -> Option<String> {
    let markers: &[(&str, &str)] = match language {
        Language::Node => &[
            ("/.nvm/", "nvm"),
            ("/fnm_multishells/", "fnm"),
            ("/.volta/", "volta"),
            ("/n/versions/", "n"),
        ],
        Language::Deno => &[("/.deno/", "deno"), ("/.asdf/", "asdf"), ("/mise/", "mise")],
        Language::Bun => &[("/.bun/", "bun"), ("/.asdf/", "asdf"), ("/mise/", "mise")],
        Language::Python => &[("/.pyenv/", "pyenv"), ("/uv/", "uv"), ("/.asdf/", "asdf")],
        // Windows 上 rustup 把 shim 放在 `~/.cargo/bin`，因此也要匹配 cargo home。
        Language::Rust => &[("/.rustup/", "rustup"), ("/.cargo/bin/", "rustup")],
        Language::Go => &[("/go/", "go-install"), ("/sdk/", "go-install")],
        Language::Zig => &[("/zig/", "zig"), ("/.asdf/", "asdf")],
        Language::Dart => &[("/fvm/", "fvm")],
        Language::Kotlin
        | Language::Java
        | Language::Scala
        | Language::Groovy
        | Language::Clojure => &[("/.sdkman/", "sdkman")],
        Language::CSharp | Language::FSharp => &[("/.dotnet/", "dotnet")],
        Language::Swift => &[("/.swiftenv/", "swiftenv")],
        Language::Ruby => &[("/.rbenv/", "rbenv"), ("/.rvm/", "rvm")],
        Language::PHP => &[("/.phpenv/", "phpenv")],
        Language::Perl => &[("/.plenv/", "plenv")],
        Language::Lua => &[("/.luaver/", "luaver")],
        Language::Erlang => &[("/.kerl/", "kerl")],
        Language::Haskell => &[("/.ghcup/", "ghcup")],
        Language::Julia => &[("/.juliaup/", "juliaup")],
        Language::Nim => &[("/.choosenim/", "choosenim")],
        Language::OCaml => &[("/.opam/", "opam")],
        Language::C | Language::Cpp => &[],
        _ => &[],
    };
    markers
        .iter()
        .find(|(marker, _)| path.contains(marker))
        .map(|(_, manager)| (*manager).to_string())
}

fn read_version(language: Language, binary: &Path) -> Option<String> {
    // Dart 的 shim 启动极慢（首次要解包 SDK），优先从 SDK 目录读版本。
    if language == Language::Dart {
        if let Some(version) = dart_version_from_sdk(binary) {
            return Some(version);
        }
    }
    // 运行时的 `--version` 也遵循“昂贵探测”规则。
    if is_expensive_probe(binary) {
        return None;
    }

    // 不同语言的版本查询方式差异很大，不能统一用 `--version`。
    for invocation in version_invocations(language) {
        let args: Vec<&str> = invocation.iter().copied().collect();
        let Some(text) = run_for_version(binary, &args) else {
            continue;
        };
        if is_error_output(&text) {
            continue;
        }
        if let Some(version) = extract_semver(&text) {
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
    let text = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    text.contains("is not installed for the toolchain")
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
        // 其它常见的“没有版本”输出
        || lower.starts_with("error:")
        || lower.contains("usage:")
        || lower.contains("unrecognized option")
        || lower.contains("unknown option")
}

/// `.NET` 的「已安装」不能只看 `dotnet` 可执行文件存在与否：
/// 只装了 runtime 而没有 SDK 时，`dotnet --version` 会失败，
/// 此时不应把 C#/F# 标记为可用。
fn dotnet_has_sdk() -> bool {
    // `dotnet --list-sdks` 冷启动要 2~3 秒（.NET 运行时自己也要初始化），
    // 而 C# 和 F# 会各查一次。这里缓存结果，整个进程只执行一次。
    static CACHE: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

    if let Ok(cache) = CACHE.lock() {
        if let Some(hit) = *cache {
            return hit;
        }
    }

    let resolved = (|| {
        let binary = crate::managers::which("dotnet")?;
        let text = run_for_version(&binary, &["--list-sdks"])?;
        let has_sdk = text.lines().map(str::trim).any(|line| {
            !line.is_empty() && line.chars().next().is_some_and(|c| c.is_ascii_digit())
        });
        Some(has_sdk)
    })()
    .unwrap_or(false);

    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(resolved);
    }
    resolved
}

/// 从 Flutter SDK 的版本文件读取 Dart 版本，避免启动慢命令。
fn dart_version_from_sdk(binary: &Path) -> Option<String> {
    dart_or_flutter_sdk_version(binary)
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
            if let Some(version) = extract_semver(&text) {
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
///
/// 布局：
/// - `<flutter>/bin/flutter`、`<flutter>/bin/dart` 是 sh 脚本 shim
/// - 真实 SDK：`<flutter>/bin/cache/dart-sdk/bin/dart.exe`
/// - 版本文件：`<flutter>/bin/cache/dart-sdk/version` 与 `<flutter>/version`
fn dart_or_flutter_sdk_version(binary: &Path) -> Option<String> {
    let Some(bin_dir) = binary.parent() else {
        return None;
    };
    let sdk_version = bin_dir.join("cache").join("dart-sdk").join("version");
    if let Ok(content) = std::fs::read_to_string(&sdk_version) {
        if let Some(version) = extract_semver(&content) {
            return Some(version);
        }
    }
    if let Some(flutter_root) = bin_dir.parent() {
        if let Ok(content) = std::fs::read_to_string(flutter_root.join("version")) {
            if let Some(version) = extract_semver(&content) {
                return Some(version);
            }
        }
    }
    None
}

/// 执行 `binary <flag>` 并把 stdout + stderr 合并为文本。
///
/// Windows 上的 npm / pnpm / yarn / dart 等通常是**无扩展名的 shim 或 `.cmd`**，
/// `Command::new(path)` 直接调用会失败（`os error 193`）。处理顺序：
/// 1. `.cmd` / `.bat` → `cmd /C`
/// 2. 无扩展名且以 `#!` 开头的 posix 脚本 → 通过 **Git Bash**（而非可能是 WSL 的 `bash`）执行
/// 3. 直接执行
fn run_for_version(binary: &Path, args: &[&str]) -> Option<String> {
    // 先尝试解析 shim 背后的真实可执行文件（Windows 常见）。
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
            // 统一加 5 秒硬超时，超时就当没有版本。
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
    // UTF-16LE 文本会被解码成大量 NUL / 替换字符。
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
        // 只有在确定没有 Git Bash 时才回退到 PATH 上的 sh/bash，
        // 避免每次探测都白跑一次 WSL。
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
    let mut found = Vec::new();
    let mut roots: Vec<PathBuf> = Vec::new();
    for env_key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Ok(value) = std::env::var(env_key) {
            roots.push(
                PathBuf::from(value)
                    .join("Git")
                    .join("bin")
                    .join("bash.exe"),
            );
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Git")
                .join("bin")
                .join("bash.exe"),
        );
    }
    for candidate in roots {
        if candidate.is_file() {
            found.push(candidate.display().to_string());
        }
    }
    found
}

/// 判断文件是否为 posix shell 脚本（读取前两个字节）。
fn is_posix_script(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 2];
    matches!(file.read_exact(&mut head), Ok(())) && &head == b"#!"
}

/// 某些工具在 Windows 上只提供 sh 脚本 shim（如 Flutter 的 `dart`、`flutter`），
/// 而本机未必有可用的 POSIX shell。这里尝试解析出真正的可执行文件。
pub(crate) fn resolve_real_binary(binary: &Path) -> Option<PathBuf> {
    if !cfg!(windows) || !is_posix_script(binary) {
        return None;
    }
    let text = std::fs::read_to_string(binary).ok()?;
    if !text.contains("dart-sdk") && !text.contains("flutter") {
        return None;
    }
    // Flutter 的 shim 位于 `<flutter>/bin/`，真实 SDK 在 `<flutter>/bin/cache/dart-sdk/bin/`。
    let bin_dir = binary.parent()?;
    let candidates = [
        bin_dir
            .join("cache")
            .join("dart-sdk")
            .join("bin")
            .join("dart.exe"),
        bin_dir
            .join("cache")
            .join("dart-sdk")
            .join("bin")
            .join("dart"),
        bin_dir.join("dart.exe"),
        bin_dir.join("flutter.bat"),
    ];
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// 从任意版本输出中提取版本号。
///
/// 规则：优先取段数最多（≥2 段）的 `1.2.3` 形态；只有在完全没有多段版本时，
/// 才接受单段数字，且仍然是**第一个**出现的数字（避免把 `3.1.5` 误判成
/// `2023`、把 `go version go1.25.4 windows/amd64` 误判成 `10`）。
pub fn extract_semver(text: &str) -> Option<String> {
    let mut best: Option<String> = None;
    let mut best_score = 0usize;
    let mut first_single: Option<String> = None;
    for raw in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        let candidate = raw.trim_matches('.');
        if candidate.is_empty() {
            continue;
        }
        let digits: Vec<&str> = candidate.split('.').collect();
        if digits.len() > 4 {
            continue;
        }
        if !digits
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        {
            continue;
        }
        if digits.len() == 1 {
            // 单段数字只有在不是 `0` 时才算备选；`0` 几乎总是无关计数。
            if first_single.is_none() && candidate != "0" {
                first_single = Some(candidate.to_string());
            }
            continue;
        }
        if digits.len() > best_score {
            best_score = digits.len();
            best = Some(candidate.to_string());
        }
    }
    best.or(first_single)
}

fn dir_versions(root: Option<&str>, sub: &str) -> Vec<String> {
    let Some(root) = root else {
        return Vec::new();
    };
    let dir: PathBuf = Path::new(root).join(sub);
    list_subdirs(&dir)
}

fn list_subdirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        // 隐藏目录（`.temp` / `.lock` / `.gitignore` 等）不是版本目录。
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    names
}

fn nvm_versions(probe: &ManagerProbe) -> Vec<String> {
    #[cfg(windows)]
    {
        dir_versions(probe.root.as_deref(), "")
    }
    #[cfg(not(windows))]
    {
        shell_lines("bash -lc 'nvm ls --no-colors 2>/dev/null'")
    }
}

fn fnm_list(probe: &ManagerProbe) -> Vec<String> {
    let mut versions = dir_versions(probe.root.as_deref(), "node-versions");
    if versions.is_empty() {
        versions = shell_lines("fnm list 2>/dev/null");
    }
    versions
}

fn rustup_toolchains() -> Vec<String> {
    let mut items = shell_lines("rustup toolchain list");
    if items.is_empty() {
        // 兜底：直接读 rustup home 下的 toolchains 目录。
        let home = std::env::var("RUSTUP_HOME")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".rustup")));
        if let Some(home) = home {
            items = list_subdirs(&home.join("toolchains"));
        }
    }
    items
        .into_iter()
        .filter_map(|line| {
            line.split_whitespace()
                .next()
                .map(|name| name.trim_start_matches('(').to_string())
        })
        .filter(|name| !name.is_empty() && !name.starts_with('('))
        .collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn sdkman_versions(root: Option<&str>) -> Vec<String> {
    let Some(root) = root else {
        return Vec::new();
    };
    let candidates = Path::new(root).join("candidates");
    let mut versions = Vec::new();
    for language in ["java", "kotlin"] {
        versions.extend(list_subdirs(&candidates.join(language)));
    }
    versions
}

/// Go 的多版本机制是「toolchain」：下载的 toolchain 放在
/// `$GOPATH/pkg/mod/golang.org/toolchain@v0.0.1-go<version>.<os>-<arch>`。
///
/// 注意：`GOROOT` 只指向当前 SDK，历史 toolchain 不在那里。
fn go_toolchains(probe: &ManagerProbe) -> Vec<String> {
    let mut versions = Vec::new();
    let mut roots: Vec<PathBuf> = Vec::new();

    // 显式设置时优先用 env，否则用默认 GOPATH（`~/go`）并回退到 GOROOT。
    if let Ok(gopath) = std::env::var("GOPATH") {
        if !gopath.is_empty() {
            roots.push(PathBuf::from(gopath));
        }
    }
    if let Some(home) = home_dir() {
        roots.push(home.join("go"));
    }
    if let Some(root) = probe.root.as_deref() {
        // GOROOT 下的 toolchain 目录（部分发行版会放在这里）。
        roots.push(PathBuf::from(root));
    }

    for root in roots {
        let dir = root.join("pkg").join("mod").join("golang.org");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(rest) = name.strip_prefix("toolchain@") else {
                continue;
            };
            // `v0.0.1-go1.26.10.windows-amd64` → `1.26.10`
            let Some(go_part) = rest.split("-go").nth(1) else {
                continue;
            };
            if let Some(version) = extract_semver(go_part) {
                versions.push(version);
            }
        }
    }

    versions.sort();
    versions.dedup();
    versions
}

fn dotnet_sdks() -> Vec<String> {
    shell_lines("dotnet --list-sdks")
        .into_iter()
        .filter_map(|line| line.split_whitespace().next().map(ToOwned::to_owned))
        .collect()
}

/// Volta 把运行时装在 `tools/image/<runtime>/<version>/`。
///
/// 必须按语言过滤，否则 node 的版本列表里会混进 npm/pnpm/yarn 的版本。
fn volta_versions(probe: &ManagerProbe, language: Option<crate::Language>) -> Vec<String> {
    let Some(root) = probe.root.as_deref() else {
        return Vec::new();
    };
    let image = Path::new(root).join("tools").join("image");
    let runtime = language.map(volta_runtime_name);
    let mut versions = Vec::new();

    match runtime {
        Some(runtime) => {
            versions.extend(list_subdirs(&image.join(runtime)));
        }
        None => {
            let Ok(entries) = std::fs::read_dir(&image) else {
                return Vec::new();
            };
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    versions.extend(list_subdirs(&entry.path()));
                }
            }
        }
    }

    versions.sort();
    versions.dedup();
    versions
}

/// 语言 → Volta 使用的 runtime 目录名。
fn volta_runtime_name(language: crate::Language) -> &'static str {
    use crate::Language;
    match language {
        Language::Node => "node",
        Language::Bun => "bun",
        Language::Deno => "deno",
        Language::Python => "python",
        Language::Ruby => "ruby",
        _ => "node",
    }
}

/// uv 的 Python 安装目录形如 `cpython-3.14.2-windows-x86_64-none`，
/// 需要从中抽出 `3.14.2` 这样的语义版本。
fn uv_python_versions(probe: &ManagerProbe) -> Vec<String> {
    let Some(root) = probe.root.as_deref() else {
        return Vec::new();
    };
    let mut versions: Vec<String> = list_subdirs(&Path::new(root).join("python"))
        .into_iter()
        // `.temp`、`.lock` 等隐藏项不是安装目录。
        .filter(|name| !name.starts_with('.'))
        .filter_map(|name| extract_semver(&name))
        .collect();
    versions.sort();
    versions.dedup();
    versions
}

fn juliaup_channels() -> Vec<String> {
    shell_lines("juliaup status")
        .into_iter()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next().map(ToOwned::to_owned))
        .filter(|name| !name.is_empty())
        .collect()
}

/// bun 的运行时版本目录。
///
/// 注意：`$BUN_INSTALL/install/cache` 是 **npm 包缓存**，不是运行时版本，
/// 之前误读这里会列出成百上千个包名。bun 的运行时实际由 `bun upgrade`
/// 原地替换，不保留多版本目录，因此这里只返回当前版本。
fn bun_versions() -> Vec<String> {
    let Some(binary) = crate::managers::which("bun") else {
        return Vec::new();
    };
    run_for_version(&binary, &["--version"])
        .and_then(|text| extract_semver(&text))
        .into_iter()
        .collect()
}

/// deno 同样不保留多版本目录，`deno upgrade` 只会覆盖当前版本。
fn deno_versions() -> Vec<String> {
    let Some(binary) = crate::managers::which("deno") else {
        return Vec::new();
    };
    run_for_version(&binary, &["--version"])
        .and_then(|text| extract_semver(&text))
        .into_iter()
        .collect()
}

fn shell_lines(script: &str) -> Vec<String> {
    // Windows 下脚本由 cmd 执行，POSIX 重定向语法会报错，才需要剥离；
    // 在 POSIX 上必须保留 `2>/dev/null`，否则 stderr（例如 nvm 未安装的报错）
    // 会混进 stdout，被当成版本号解析。
    let script = if cfg!(windows) {
        script.replace(" 2>/dev/null", "").replace(" 2>NUL", "")
    } else {
        script.to_string()
    };
    let invocations: Vec<(String, Vec<String>)> = if cfg!(windows) {
        vec![("cmd".to_string(), vec!["/C".to_string(), script.clone()])]
    } else {
        // 优先 bash；Alpine 等只有 ash 的环境回退到 sh。
        vec![
            ("bash".to_string(), vec!["-c".to_string(), script.clone()]),
            ("sh".to_string(), vec!["-c".to_string(), script.clone()]),
        ]
    };

    for (program, args) in invocations {
        let Ok(output) = Command::new(&program).args(&args).output() else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        // WSL 的 bash 会输出 UTF-16 乱码，必须跳过。
        if text
            .chars()
            .filter(|c| *c == '\u{FFFD}' || *c == '\0')
            .count()
            * 20
            > text.chars().count()
        {
            continue;
        }
        let lines: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if !lines.is_empty() {
            return lines;
        }
        if output.status.success() {
            return lines;
        }
    }
    Vec::new()
}
