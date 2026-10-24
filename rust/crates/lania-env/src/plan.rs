//! 把用户意图（安装 / 切换 / 删除 / 设为默认）翻译成可执行的步骤计划。
//!
//! 这是 CLI 与 GUI 的**唯一共享决策层**：
//! - CLI 拿到 `EnvPlan` 后决定是打印还是执行；
//! - GUI 拿到同一份 `EnvPlan` 后展示给用户确认再调用同一个执行入口。
//!
//! 每个步骤都显式声明 `program` / `args`，不以字符串拼接命令，
//! 从而在 shell 注入方面保持安全（`use_shell` 仅用于 nvm/sdkman 这类 shell 函数）。

use anyhow::anyhow;
use serde::Serialize;

use crate::{Language, ManagerKind, ManagerProbe};

/// 用户可执行的环境操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvAction {
    Install,
    Use,
    Remove,
    UninstallManagerVersion,
}

impl EnvAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Use => "use",
            Self::Remove => "remove",
            Self::UninstallManagerVersion => "uninstall",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "install" | "i" | "add" => Some(Self::Install),
            "use" | "switch" | "set" | "default" => Some(Self::Use),
            "remove" | "rm" | "delete" | "uninstall" => Some(Self::Remove),
            _ => None,
        }
    }
}

/// 计划中的单个执行步骤。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub program: String,
    pub args: Vec<String>,
    /// 为 true 时通过 shell 调用 shell 函数（nvm/sdkman）。
    /// 此时 `args` 是**已校验**的位置参数，由 `shell_function_script` 安全拼接。
    pub use_shell: bool,
    /// 供展示的完整命令文本。
    pub command: String,
    /// 该步骤的用途说明（人类可读）。
    pub label: String,
}

impl PlanStep {
    fn direct(program: &str, args: Vec<String>, label: impl Into<String>) -> Self {
        let command = std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .map(|part| quote_if_needed(&part))
            .collect::<Vec<_>>()
            .join(" ");
        Self {
            program: program.to_string(),
            args,
            use_shell: false,
            command,
            label: label.into(),
        }
    }

    /// 调用 shell 函数（如 `nvm install 20`）。
    ///
    /// 安全约束：`func` 必须是内置白名单里的字面量，`args` 只允许版本号 / 语言名等
    /// 经过 `sanitize_shell_token` 校验的片段，从而杜绝命令注入。
    fn shell_function(func: &str, args: &[&str], label: impl Into<String>) -> Self {
        let script = shell_function_script(func, args);
        Self {
            program: if cfg!(windows) { "cmd" } else { "bash" }.to_string(),
            args: if cfg!(windows) {
                vec!["/C".into(), script.clone()]
            } else {
                vec!["-lc".into(), script.clone()]
            },
            use_shell: true,
            command: script,
            label: label.into(),
        }
    }

    /// 写入项目版本文件的步骤（不需要外部程序）。
    fn write_file(path: &str, content: &str, label: impl Into<String>) -> Self {
        Self {
            program: "write-file".into(),
            args: vec![path.into(), content.into()],
            use_shell: false,
            command: format!("write {path} <- {content}"),
            label: label.into(),
        }
    }

    /// 供执行层复用的实际脚本（`use_shell` 步骤专用）。
    pub fn shell_script(&self) -> Option<String> {
        if !self.use_shell {
            return None;
        }
        self.args.last().cloned()
    }
}

/// 允许出现在 shell 命令里的字符集合：版本号、语言名、`@`、`.`、`-`、`_`、`+`、`/`。
///
/// 任何其它字符都会被拒绝，而不是“转义后放行”——CLI 的版本参数不需要它们。
pub fn validate_shell_token(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    let valid = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '@' | '/'));
    valid.then_some(trimmed)
}

fn shell_function_script(func: &str, args: &[&str]) -> String {
    debug_assert!(matches!(func, "nvm" | "sdk" | "sdkman"));
    let mut parts = vec![func.to_string()];
    for arg in args {
        // 非法的片段会被丢弃，宁可少传参数也不拼接任意内容。
        if let Some(safe) = validate_shell_token(arg) {
            parts.push(safe.to_string());
        }
    }
    let call = parts.join(" ");
    // `sdk` 是 SDKMAN 的 shell 函数，非交互 shell 里必须先初始化；
    // 初始化脚本缺失时退回直接调用（用户手动 source 过的情况）。
    if func == "sdk" || func == "sdkman" {
        return format!(
            "if [ -s \"$HOME/.sdkman/bin/sdkman-init.sh\" ]; then \
             . \"$HOME/.sdkman/bin/sdkman-init.sh\"; fi; {call}"
        );
    }
    if func == "nvm" {
        return format!(
            "if [ -s \"$HOME/.nvm/nvm.sh\" ]; then . \"$HOME/.nvm/nvm.sh\"; fi; {call}"
        );
    }
    call
}

/// 一次环境操作的完整计划。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvPlan {
    pub action: EnvAction,
    pub language: Language,
    pub tool: String,
    pub version: Option<String>,
    pub manager: Option<ManagerKind>,
    pub manager_available: bool,
    pub steps: Vec<PlanStep>,
    /// 无法生成计划时的原因。
    pub error: Option<String>,
    /// 提醒用户注意的附加说明（例如需要重开终端）。
    pub notes: Vec<String>,
}

impl EnvPlan {
    pub fn is_runnable(&self) -> bool {
        self.error.is_none() && !self.steps.is_empty()
    }
}

/// 为某个语言选择“最合适的”可用管理器。
pub fn pick_manager(language: Language, preferred: Option<ManagerKind>) -> Option<ManagerProbe> {
    // 有些管理器虽然本机可用，但对特定语言没有可用的包（例如 winget 里
    // 没有 MoonBit / Cangjie）。这类组合必须排除，否则会生成空 ID 的命令。
    let usable = |manager: ManagerKind| -> bool {
        if manager == ManagerKind::Winget {
            return !winget_id(language).is_empty();
        }
        true
    };
    if let Some(preferred) = preferred {
        let probe = crate::managers::probe(preferred);
        if probe.available && usable(preferred) {
            return Some(probe);
        }
    }
    language
        .managers()
        .into_iter()
        .filter(|manager| usable(*manager))
        .map(crate::managers::probe)
        .find(|probe| probe.available)
}

/// 构建环境操作计划。
pub fn build_plan(
    action: EnvAction,
    tool: &str,
    version: Option<&str>,
    preferred_manager: Option<ManagerKind>,
    cwd: &std::path::Path,
    project_scope: bool,
) -> EnvPlan {
    let Some(language) = Language::parse_id(tool) else {
        return EnvPlan {
            action,
            language: Language::Node,
            tool: tool.to_string(),
            version: version.map(ToOwned::to_owned),
            manager: None,
            manager_available: false,
            steps: Vec::new(),
            error: Some(format!(
                "unsupported tool `{tool}`; supported: node, python, rust, go, dart, kotlin, java, c"
            )),
            notes: Vec::new(),
        };
    };

    let manager = pick_manager(language, preferred_manager);
    let mut notes = Vec::new();
    let mut steps = Vec::new();
    let mut error = None;

    // 版本号会被拼入 shell 函数调用（nvm / sdkman），因此必须在入口处校验。
    if let Some(version) = version {
        if validate_shell_token(version).is_none() {
            error = Some(format!(
                "invalid version `{version}`: only letters, digits and . - _ + @ / are allowed"
            ));
        }
    }

    if error.is_some() {
        return EnvPlan {
            action,
            language,
            tool: language.id().to_string(),
            version: version.map(ToOwned::to_owned),
            manager: manager.as_ref().map(|probe| probe.manager),
            manager_available: manager.is_some(),
            steps,
            error,
            notes: vec!["use a plain version such as 20, 3.12, stable or 1.86.0".to_string()],
        };
    }

    match action {
        EnvAction::Install => match version {
            None => {
                error = Some("install requires a version, e.g. `lan env install node 20`".into())
            }
            Some(version) => match manager.as_ref() {
                Some(probe) => steps.extend(install_steps(language, probe.manager, version)),
                None => error = Some(no_manager_message(language)),
            },
        },
        EnvAction::Use => {
            if let Some(probe) = manager.as_ref() {
                steps.extend(use_steps(language, probe.manager, version));
            }
            if project_scope {
                let file = language
                    .version_files()
                    .first()
                    .copied()
                    .ok_or_else(|| anyhow!("{} has no project version file", language.id()));
                match (file, version) {
                    (Ok(file), Some(version)) => steps.push(PlanStep::write_file(
                        &cwd.join(file).display().to_string(),
                        version,
                        format!("pin {file} to {version}"),
                    )),
                    (Ok(file), None) => steps.push(PlanStep::write_file(
                        &cwd.join(file).display().to_string(),
                        "",
                        format!("clear {file} project pin"),
                    )),
                    (Err(err), _) => error = Some(err.to_string()),
                }
                notes.push("项目级 pin 只影响当前目录及其子目录".into());
            } else if version.is_none() {
                error =
                    Some("use requires a version (or --project to clear the project pin)".into());
            }
            notes.push("切换版本后需要重开终端或重新加载 shell 才能生效".into());
        }
        EnvAction::Remove | EnvAction::UninstallManagerVersion => match (manager.as_ref(), version)
        {
            (Some(probe), Some(version)) => {
                steps.extend(remove_steps(probe.manager, language, version))
            }
            (Some(_), None) => error = Some("remove requires a version".into()),
            (None, _) => error = Some(no_manager_message(language)),
        },
    }

    if error.is_none() && steps.is_empty() {
        error = Some(format!(
            "no {} steps could be generated for {}",
            action.as_str(),
            language.id()
        ));
    }

    EnvPlan {
        action,
        language,
        tool: language.id().to_string(),
        version: version.map(ToOwned::to_owned),
        manager: manager.as_ref().map(|probe| probe.manager),
        manager_available: manager.is_some(),
        steps,
        error,
        notes,
    }
}

fn no_manager_message(language: Language) -> String {
    let names: Vec<&str> = language.managers().iter().map(|m| m.id()).collect();
    format!(
        "no supported version manager found for {}. install one of: {}",
        language.id(),
        names.join(", ")
    )
}

fn install_steps(language: Language, manager: ManagerKind, version: &str) -> Vec<PlanStep> {
    let v = version.to_string();
    match manager {
        ManagerKind::Nvm => vec![PlanStep::shell_function(
            "nvm",
            &["install", &v],
            format!("install node {v} via nvm"),
        )],
        ManagerKind::Fnm => vec![PlanStep::direct(
            "fnm",
            vec!["install".into(), v.clone()],
            format!("install node {v} via fnm"),
        )],
        ManagerKind::Volta => vec![PlanStep::direct(
            "volta",
            vec!["install".into(), format!("node@{v}")],
            format!("install node {v} via volta"),
        )],
        ManagerKind::N => vec![PlanStep::direct(
            "n",
            vec![v.clone()],
            format!("install node {v} via n"),
        )],
        ManagerKind::Asdf => vec![
            PlanStep::direct(
                "asdf",
                vec!["plugin".into(), "add".into(), language.id().into()],
                "ensure asdf plugin",
            ),
            PlanStep::direct(
                "asdf",
                vec!["install".into(), language.id().into(), v.clone()],
                format!("install {} {v} via asdf", language.id()),
            ),
        ],
        ManagerKind::Mise => vec![PlanStep::direct(
            "mise",
            vec![
                "use".into(),
                "--global".into(),
                format!("{}@{v}", language.id()),
            ],
            format!("install {} {v} via mise", language.id()),
        )],
        ManagerKind::Pyenv => vec![PlanStep::direct(
            "pyenv",
            vec!["install".into(), v.clone()],
            format!("install python {v} via pyenv"),
        )],
        ManagerKind::Uv => vec![PlanStep::direct(
            "uv",
            vec!["python".into(), "install".into(), v.clone()],
            format!("install python {v} via uv"),
        )],
        ManagerKind::Rustup => vec![PlanStep::direct(
            "rustup",
            vec!["toolchain".into(), "install".into(), v.clone()],
            format!("install rust toolchain {v}"),
        )],
        ManagerKind::GoInstall => vec![PlanStep::direct(
            "go",
            vec!["install".into(), format!("golang.org/dl/go{v}@latest")],
            format!("prepare go {v} via golang.org/dl"),
        )],
        ManagerKind::Fvm => vec![PlanStep::direct(
            "fvm",
            vec!["install".into(), v.clone()],
            format!("install flutter {v} via fvm"),
        )],
        ManagerKind::Sdkman => vec![PlanStep::shell_function(
            "sdk",
            &["install", sdkman_candidate(language), &v],
            format!("install {} {v} via sdkman", language.id()),
        )],
        ManagerKind::Scoop => vec![PlanStep::direct(
            "scoop",
            vec!["install".into(), format!("{}@{}", language.id(), v)],
            format!("install {} {v} via scoop", language.id()),
        )],
        ManagerKind::Winget => vec![PlanStep::direct(
            "winget",
            vec![
                "install".into(),
                "--id".into(),
                winget_id(language).to_string(),
                "--version".into(),
                v.clone(),
            ],
            format!("install {} {v} via winget", language.id()),
        )],
        ManagerKind::Brew => vec![PlanStep::direct(
            "brew",
            vec!["install".into(), format!("{}@{v}", language.id())],
            format!("install {} {v} via brew", language.id()),
        )],
        ManagerKind::Apt => vec![PlanStep::direct(
            "apt-get",
            vec!["install".into(), "-y".into(), language.id().into()],
            format!("install {} via apt", language.id()),
        )],
        // ---- 其余语言专用管理器 ----
        ManagerKind::Bunup => vec![PlanStep::direct(
            "bun",
            vec!["upgrade".into(), format!("bun@{v}")],
            format!("install bun {v}"),
        )],
        ManagerKind::Denoup => vec![PlanStep::direct(
            "deno",
            vec!["upgrade".into(), format!("--version={v}")],
            format!("install deno {v}"),
        )],
        ManagerKind::ZigVm => vec![PlanStep::direct(
            "zig",
            vec!["fetch".into(), v.clone()],
            format!("fetch zig {v}"),
        )],
        ManagerKind::Coursier => vec![PlanStep::direct(
            "cs",
            vec!["install".into(), v.clone()],
            format!("install {v} via coursier"),
        )],
        ManagerKind::Dotnet => vec![PlanStep::direct(
            "dotnet",
            vec!["--version".into(), v.clone()],
            format!("select .NET SDK {v}"),
        )],
        ManagerKind::Swiftenv => vec![PlanStep::direct(
            "swiftenv",
            vec!["install".into(), v.clone()],
            format!("install swift {v}"),
        )],
        ManagerKind::Rbenv => vec![PlanStep::direct(
            "rbenv",
            vec!["install".into(), v.clone()],
            format!("install ruby {v}"),
        )],
        ManagerKind::Rvm => vec![PlanStep::direct(
            "rvm",
            vec!["install".into(), v.clone()],
            format!("install ruby {v} via rvm"),
        )],
        ManagerKind::Phpenv => vec![PlanStep::direct(
            "phpenv",
            vec!["install".into(), v.clone()],
            format!("install php {v}"),
        )],
        ManagerKind::Plenv => vec![PlanStep::direct(
            "plenv",
            vec!["install".into(), v.clone()],
            format!("install perl {v}"),
        )],
        ManagerKind::Luaver => vec![PlanStep::direct(
            "luaver",
            vec!["install".into(), v.clone()],
            format!("install lua {v}"),
        )],
        ManagerKind::KerL => vec![PlanStep::direct(
            "kerl",
            vec!["build".into(), "install".into(), v.clone()],
            format!("build erlang {v}"),
        )],
        ManagerKind::GhcUp => vec![PlanStep::direct(
            "ghcup",
            vec!["install".into(), v.clone()],
            format!("install ghc {v}"),
        )],
        ManagerKind::Juliaup => vec![PlanStep::direct(
            "juliaup",
            vec!["add".into(), v.clone()],
            format!("install julia {v}"),
        )],
        ManagerKind::Rig => vec![PlanStep::direct(
            "rig",
            vec!["add".into(), v.clone()],
            format!("install R {v}"),
        )],
        ManagerKind::Choosenim => vec![PlanStep::direct(
            "choosenim",
            vec![v.clone()],
            format!("install nim {v}"),
        )],
        ManagerKind::Opam => vec![
            PlanStep::direct(
                "opam",
                vec!["init".into(), "--bare".into()],
                "ensure opam initialized",
            ),
            PlanStep::direct(
                "opam",
                vec!["switch".into(), "create".into(), v.clone()],
                format!("create opam switch {v}"),
            ),
        ],
        ManagerKind::Cljup => vec![PlanStep::direct(
            "cljup",
            vec!["install".into(), v.clone()],
            format!("install clojure {v}"),
        )],
        ManagerKind::Moonup => vec![PlanStep::direct(
            "moon",
            vec!["version".into(), "install".into(), v.clone()],
            format!("install moonbit {v}"),
        )],
        ManagerKind::Cangjieup => vec![PlanStep::direct(
            "cjc",
            vec!["--version".into()],
            format!("安装仓颉 {v} 需要通过 Cangjie SDK 安装包"),
        )],
        ManagerKind::Deveco => vec![PlanStep::direct(
            "ohpm",
            vec!["install".into()],
            format!("安装 ArkTS/HarmonyOS SDK {v} 需要通过 DevEco Studio"),
        )],
        ManagerKind::Foundryup => vec![PlanStep::direct(
            "foundryup",
            vec!["--version".into(), v.clone()],
            format!("install foundry {v}"),
        )],
        ManagerKind::Vup => vec![PlanStep::direct(
            "v",
            vec!["up".into()],
            "update V toolchain to latest".to_string(),
        )],
        ManagerKind::Haxelihx => vec![PlanStep::direct(
            "haxelib",
            vec!["install".into(), "hxcpp".into()],
            format!("安装 Haxe {v} 需要通过系统包管理器"),
        )],
    }
}

fn use_steps(language: Language, manager: ManagerKind, version: Option<&str>) -> Vec<PlanStep> {
    let v = version.unwrap_or_default().to_string();
    let has_version = !v.is_empty();
    match manager {
        ManagerKind::Nvm => vec![PlanStep::shell_function(
            "nvm",
            &["use", &v],
            "switch node version via nvm",
        )],
        ManagerKind::Fnm => vec![PlanStep::direct(
            "fnm",
            vec!["use".into(), v.clone()],
            "switch node version via fnm",
        )],
        ManagerKind::Volta => vec![PlanStep::direct(
            "volta",
            vec!["pin".into(), format!("node@{v}")],
            "pin node version via volta",
        )],
        ManagerKind::N => vec![PlanStep::direct(
            "n",
            vec![v.clone()],
            "switch node version via n",
        )],
        ManagerKind::Asdf => {
            if has_version {
                vec![PlanStep::direct(
                    "asdf",
                    vec!["global".into(), language.id().into(), v.clone()],
                    "set asdf global version",
                )]
            } else {
                vec![]
            }
        }
        ManagerKind::Mise => vec![PlanStep::direct(
            "mise",
            vec![
                "use".into(),
                "--global".into(),
                format!("{}@{v}", language.id()),
            ],
            "set mise global version",
        )],
        ManagerKind::Pyenv => {
            if has_version {
                vec![PlanStep::direct(
                    "pyenv",
                    vec!["global".into(), v.clone()],
                    "set pyenv global version",
                )]
            } else {
                vec![]
            }
        }
        ManagerKind::Uv => vec![PlanStep::direct(
            "uv",
            vec!["python".into(), "pin".into(), v.clone()],
            "pin python version via uv",
        )],
        ManagerKind::Rustup => vec![PlanStep::direct(
            "rustup",
            vec!["default".into(), v.clone()],
            "set default rust toolchain",
        )],
        ManagerKind::GoInstall => vec![PlanStep::direct(
            "go",
            vec!["env".into(), "-w".into(), format!("GOTOOLCHAIN=go{v}")],
            "set go toolchain",
        )],
        ManagerKind::Fvm => vec![PlanStep::direct(
            "fvm",
            vec!["use".into(), v.clone()],
            "switch flutter version via fvm",
        )],
        ManagerKind::Sdkman => vec![PlanStep::shell_function(
            "sdk",
            &["default", sdkman_candidate(language), &v],
            "set sdkman default",
        )],
        ManagerKind::Scoop => vec![PlanStep::direct(
            "scoop",
            vec![
                "reset".into(),
                language.id().into(),
                "--version".into(),
                v.clone(),
            ],
            "switch scoop app version",
        )],
        ManagerKind::Winget => vec![],
        ManagerKind::Brew => vec![PlanStep::direct(
            "brew",
            vec![
                "link".into(),
                "--overwrite".into(),
                format!("{}@{v}", language.id()),
            ],
            "link brew version",
        )],
        ManagerKind::Apt => vec![],
        // ---- 其余语言专用管理器 ----
        ManagerKind::Bunup => vec![], // bun 无固定版本切换命令，通过升级落盘
        ManagerKind::Denoup => vec![],
        ManagerKind::ZigVm => vec![PlanStep::direct(
            "zig",
            vec!["use".into(), v.clone()],
            "switch zig version",
        )],
        ManagerKind::Coursier => vec![PlanStep::direct(
            "cs",
            vec!["install".into(), v.clone()],
            "activate component via coursier",
        )],
        ManagerKind::Dotnet => vec![PlanStep::direct(
            "dotnet",
            vec!["--version".into(), v.clone()],
            "select .NET SDK",
        )],
        ManagerKind::Swiftenv => vec![PlanStep::direct(
            "swiftenv",
            vec!["global".into(), v.clone()],
            "set swift global version",
        )],
        ManagerKind::Rbenv => vec![PlanStep::direct(
            "rbenv",
            vec!["global".into(), v.clone()],
            "set rbenv global version",
        )],
        ManagerKind::Rvm => vec![PlanStep::direct(
            "rvm",
            vec!["use".into(), format!("--default={v}")],
            "set rvm default version",
        )],
        ManagerKind::Phpenv => vec![PlanStep::direct(
            "phpenv",
            vec!["global".into(), v.clone()],
            "set phpenv global version",
        )],
        ManagerKind::Plenv => vec![PlanStep::direct(
            "plenv",
            vec!["global".into(), v.clone()],
            "set plenv global version",
        )],
        ManagerKind::Luaver => vec![PlanStep::direct(
            "luaver",
            vec!["use".into(), v.clone()],
            "set luaver default version",
        )],
        ManagerKind::KerL => vec![PlanStep::direct(
            "kerl",
            vec!["activate".into(), v.clone()],
            "activate erlang release",
        )],
        ManagerKind::GhcUp => vec![PlanStep::direct(
            "ghcup",
            vec!["set".into(), v.clone()],
            "set ghcup default",
        )],
        ManagerKind::Juliaup => vec![PlanStep::direct(
            "juliaup",
            vec!["default".into(), v.clone()],
            "set juliaup default",
        )],
        ManagerKind::Rig => vec![PlanStep::direct(
            "rig",
            vec!["default".into(), v.clone()],
            "set R default version",
        )],
        ManagerKind::Choosenim => vec![PlanStep::direct(
            "choosenim",
            vec![v.clone()],
            "switch nim version",
        )],
        ManagerKind::Opam => vec![PlanStep::direct(
            "opam",
            vec!["switch".into(), v.clone()],
            "switch opam switch",
        )],
        ManagerKind::Cljup => vec![PlanStep::direct(
            "cljup",
            vec!["default".into(), v.clone()],
            "set cljup default",
        )],
        ManagerKind::Moonup => vec![PlanStep::direct(
            "moon",
            vec!["version".into(), "use".into(), v.clone()],
            "switch moonbit version",
        )],
        ManagerKind::Cangjieup => vec![PlanStep::direct(
            "cjc",
            vec!["--version".into()],
            "切换仓颉版本需要调整 CANGJIE_HOME 指向的 SDK",
        )],
        ManagerKind::Deveco => vec![PlanStep::direct(
            "ohpm",
            vec!["--version".into()],
            "切换 HarmonyOS SDK 需要在 DevEco Studio 中选择",
        )],
        ManagerKind::Foundryup => vec![PlanStep::direct(
            "foundryup",
            vec!["--version".into(), v.clone()],
            "switch foundry version",
        )],
        ManagerKind::Vup => vec![PlanStep::direct(
            "v",
            vec!["up".into()],
            "update V toolchain".to_string(),
        )],
        ManagerKind::Haxelihx => vec![PlanStep::direct(
            "haxelib",
            vec!["set".into(), v.clone()],
            "set haxelib version",
        )],
    }
}

fn remove_steps(manager: ManagerKind, language: Language, version: &str) -> Vec<PlanStep> {
    let v = version.to_string();
    match manager {
        ManagerKind::Nvm => vec![PlanStep::shell_function(
            "nvm",
            &["uninstall", &v],
            format!("uninstall node {v}"),
        )],
        ManagerKind::Fnm => vec![PlanStep::direct(
            "fnm",
            vec!["uninstall".into(), v.clone()],
            format!("uninstall node {v}"),
        )],
        ManagerKind::Volta => vec![PlanStep::direct(
            "volta",
            vec!["uninstall".into(), format!("node@{v}")],
            format!("uninstall node {v}"),
        )],
        ManagerKind::Asdf => vec![PlanStep::direct(
            "asdf",
            vec!["uninstall".into(), language.id().into(), v.clone()],
            format!("uninstall {} {v}", language.id()),
        )],
        ManagerKind::Mise => vec![PlanStep::direct(
            "mise",
            vec!["uninstall".into(), format!("{}@{v}", language.id())],
            format!("uninstall {} {v}", language.id()),
        )],
        ManagerKind::Pyenv => vec![PlanStep::direct(
            "pyenv",
            vec!["uninstall".into(), v.clone()],
            format!("uninstall python {v}"),
        )],
        ManagerKind::Rustup => vec![PlanStep::direct(
            "rustup",
            vec!["toolchain".into(), "uninstall".into(), v.clone()],
            format!("uninstall rust toolchain {v}"),
        )],
        ManagerKind::Fvm => vec![PlanStep::direct(
            "fvm",
            vec!["releases".into(), "remove".into(), v.clone()],
            format!("remove flutter {v}"),
        )],
        ManagerKind::Sdkman => vec![PlanStep::shell_function(
            "sdk",
            &["uninstall", sdkman_candidate(language), &v],
            format!("uninstall {} {v}", language.id()),
        )],
        ManagerKind::Scoop => vec![PlanStep::direct(
            "scoop",
            vec!["uninstall".into(), language.id().into()],
            format!("uninstall {} via scoop", language.id()),
        )],
        ManagerKind::Winget => vec![PlanStep::direct(
            "winget",
            vec![
                "uninstall".into(),
                "--id".into(),
                winget_id(language).to_string(),
            ],
            format!("uninstall {} via winget", language.id()),
        )],
        ManagerKind::Brew => vec![PlanStep::direct(
            "brew",
            vec!["uninstall".into(), format!("{}@{v}", language.id())],
            format!("uninstall {} {v}", language.id()),
        )],
        ManagerKind::Apt => vec![PlanStep::direct(
            "apt-get",
            vec!["remove".into(), "-y".into(), language.id().into()],
            format!("remove {} via apt", language.id()),
        )],
        // ---- 其余语言专用管理器 ----
        ManagerKind::Swiftenv => vec![PlanStep::direct(
            "swiftenv",
            vec!["uninstall".into(), v.clone()],
            format!("uninstall swift {v}"),
        )],
        ManagerKind::Rbenv => vec![PlanStep::direct(
            "rbenv",
            vec!["uninstall".into(), v.clone()],
            format!("uninstall ruby {v}"),
        )],
        ManagerKind::Rvm => vec![PlanStep::direct(
            "rvm",
            vec!["remove".into(), v.clone()],
            format!("uninstall ruby {v} via rvm"),
        )],
        ManagerKind::Phpenv => vec![PlanStep::direct(
            "phpenv",
            vec!["uninstall".into(), v.clone()],
            format!("uninstall php {v}"),
        )],
        ManagerKind::Plenv => vec![PlanStep::direct(
            "plenv",
            vec!["uninstall".into(), v.clone()],
            format!("uninstall perl {v}"),
        )],
        ManagerKind::Luaver => vec![PlanStep::direct(
            "luaver",
            vec!["uninstall".into(), v.clone()],
            format!("uninstall lua {v}"),
        )],
        ManagerKind::KerL => vec![PlanStep::direct(
            "kerl",
            vec!["delete".into(), "installation".into(), v.clone()],
            format!("remove erlang {v}"),
        )],
        ManagerKind::GhcUp => vec![PlanStep::direct(
            "ghcup",
            vec!["uninstall".into(), v.clone()],
            format!("uninstall ghc {v}"),
        )],
        ManagerKind::Juliaup => vec![PlanStep::direct(
            "juliaup",
            vec!["remove".into(), v.clone()],
            format!("remove julia {v}"),
        )],
        ManagerKind::Rig => vec![PlanStep::direct(
            "rig",
            vec!["rm".into(), v.clone()],
            format!("remove R {v}"),
        )],
        ManagerKind::Opam => vec![PlanStep::direct(
            "opam",
            vec!["switch".into(), "remove".into(), v.clone()],
            format!("remove opam switch {v}"),
        )],
        ManagerKind::Dotnet => vec![],
        ManagerKind::ZigVm => vec![PlanStep::direct(
            "zig",
            vec!["delete".into(), v.clone()],
            format!("remove zig {v}"),
        )],
        _ => vec![],
    }
}

fn sdkman_candidate(language: Language) -> &'static str {
    match language {
        Language::Kotlin => "kotlin",
        Language::Scala => "scala",
        Language::Groovy => "groovy",
        Language::Clojure => "clojure",
        _ => "java",
    }
}

fn winget_id(language: Language) -> &'static str {
    match language {
        Language::Node => "OpenJS.NodeJS",
        Language::Deno => "DenoLand.Deno",
        Language::Bun => "Oven-sh.Bun",
        Language::Python => "Python.Python.3",
        Language::Rust => "Rustlang.Rustup",
        Language::Go => "GoLang.Go",
        Language::Zig => "zig.zig",
        Language::Dart => "Google.DartSDK",
        Language::Kotlin => "JetBrains.Kotlin",
        Language::Java => "EclipseAdoptium.Temurin",
        Language::Scala => "Scala.Scala",
        Language::Groovy => "Groovy.Groovy",
        Language::C => "BrechtSanders.WinLibs.POSIX.UCRT",
        Language::Cpp => "BrechtSanders.WinLibs.POSIX.UCRT",
        Language::CSharp => "Microsoft.DotNet.SDK.8",
        Language::FSharp => "Microsoft.DotNet.SDK.8",
        Language::Swift => "Swift.Toolchain",
        Language::Ruby => "RubyInstallerTeam.Ruby",
        Language::PHP => "PHP.PHP",
        Language::Perl => "StrawberryPerl.StrawberryPerl",
        Language::Lua => "DEVCOM.Lua",
        Language::Elixir => "Elixir.Elixir",
        Language::Erlang => "Erlang.ErlangOTP",
        Language::Haskell => "haskell.ghcup",
        Language::Julia => "Julialang.Julia",
        Language::R => "RProject.R",
        Language::Nim => "nim-lang.Nim",
        Language::Crystal => "CrystalLang.Crystal",
        Language::OCaml => "OCaml.OCaml",
        Language::Clojure => "Clojure.Clojure",
        Language::Fortran => "GNU.Fortran",
        Language::D => "Dlang.DMD",
        // 以下语言在 winget 没有官方包（或名称不稳定），返回空串表示不可用，
        // 由调用方回退到其它管理器。
        Language::V => "Vlang.V",
        Language::Gleam => "Gleam.Gleam",
        // MoonBit / Cangjie / ArkTS / Solidity / Haxe / Elm 无官方 winget 包。
        Language::MoonBit
        | Language::Cangjie
        | Language::ArkTs
        | Language::Solidity
        | Language::Haxe
        | Language::Elm => "",
    }
}

fn quote_if_needed(part: &str) -> String {
    if part.is_empty() || part.chars().any(|c| c.is_whitespace()) {
        format!("\"{part}\"")
    } else {
        part.to_string()
    }
}
