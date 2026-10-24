//! `lan env` 的查询类子命令：list / info / versions / doctor。
//!
//! 全部为只读操作，不经过 dry-run 门控。

use anyhow::Result;
use lania_command::{ArgSpec, CommandSpec, OptionSpec, ValueKind};
use lania_env::{EnvService, Language, ManagerKind};
use lania_host::execution::CommandExecutionContext;
use serde_json::{json, Value};
use std::path::Path;

use crate::{
    manager_filter, tool_filter, DOCTOR_HANDLER_ID, INFO_HANDLER_ID, LIST_HANDLER_ID,
    VERSIONS_HANDLER_ID,
};

pub(super) fn list_spec() -> CommandSpec {
    CommandSpec::new(
        "list",
        "Show all supported toolchains and versions",
        LIST_HANDLER_ID,
    )
    .with_options(vec![manager_option()])
}

pub(super) fn info_spec() -> CommandSpec {
    CommandSpec::new(
        "info",
        "Show details for a single toolchain",
        INFO_HANDLER_ID,
    )
    .with_args(vec![tool_arg()])
    .with_options(vec![manager_option()])
}

pub(super) fn versions_spec() -> CommandSpec {
    CommandSpec::new(
        "versions",
        "List installed versions of a toolchain",
        VERSIONS_HANDLER_ID,
    )
    .with_args(vec![tool_arg()])
    .with_options(vec![manager_option()])
}

pub(super) fn doctor_spec() -> CommandSpec {
    CommandSpec::new(
        "doctor",
        "Check for missing toolchains and PATH conflicts",
        DOCTOR_HANDLER_ID,
    )
}

/// 构造 install / use / remove 共用的位置参数与选项。
pub(super) fn mutate_spec(name: &str, about: &str, handler_id: &str) -> CommandSpec {
    CommandSpec::new(name, about, handler_id)
        .with_args(vec![
            tool_arg(),
            ArgSpec {
                name: "version".into(),
                required: false,
                multiple: false,
                help: "Target version, e.g. 20 / 3.12 / stable".into(),
            },
        ])
        .with_options(vec![
            manager_option(),
            OptionSpec {
                long: "yes".into(),
                short: Some('y'),
                help: "Actually execute the plan (default is dry-run)".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "project".into(),
                short: Some('p'),
                help: "Write the version into the project version file".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
        ])
}

fn tool_arg() -> ArgSpec {
    ArgSpec {
        name: "tool".into(),
        required: true,
        multiple: false,
        help: "Tool to manage: node/python/rust/go/dart/kotlin/java/c".into(),
    }
}

fn manager_option() -> OptionSpec {
    OptionSpec {
        long: "manager".into(),
        short: Some('m'),
        help: "Force a specific version manager (nvm/fnm/volta/pyenv/rustup/...)".into(),
        value_kind: ValueKind::OptionalString,
        default_value: None,
        choices: vec![],
        negatable: false,
    }
}

pub(super) fn execute_list(
    service: &EnvService,
    cwd: &Path,
    ctx: &CommandExecutionContext<'_>,
) -> Value {
    let filter = tool_filter(ctx.command());
    let manager = manager_filter(ctx);
    let tools: Vec<Value> = service
        .scan(cwd)
        .into_iter()
        .filter(|tool| {
            filter
                .as_ref()
                .is_none_or(|needle| tool.id.contains(needle) || tool.language.contains(needle))
        })
        .filter(|tool| {
            manager.is_none_or(|manager| {
                tool.candidates
                    .iter()
                    .any(|probe| probe.manager == manager && probe.available)
            })
        })
        .map(|tool| serde_json::to_value(tool).unwrap_or(Value::Null))
        .collect();
    json!({
        "kind": "env_toolchains",
        "cwd": cwd.display().to_string(),
        "platform": std::env::consts::OS,
        "tools": tools,
    })
}

pub(super) fn execute_info(
    service: &EnvService,
    cwd: &Path,
    ctx: &CommandExecutionContext<'_>,
) -> Result<Value> {
    let language = require_language(ctx, "info")?;
    let status = service.status(language, cwd);
    // 列出「所有候选管理器」装过的版本，而不只是当前生效的那个：
    // 例如 node 来自系统安装时，用户仍需要看到 nvm/volta 里装过哪些版本。
    let installed = service
        .versions_for_language(language)
        .into_iter()
        .map(|item| {
            json!({
                "manager": item.manager,
                "version": item.version,
                "current": status.version.as_deref() == Some(item.version.as_str()),
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "kind": "env_toolchain_info",
        "tool": serde_json::to_value(&status)?,
        "installedVersions": installed,
    }))
}

pub(super) fn execute_versions(
    service: &EnvService,
    ctx: &CommandExecutionContext<'_>,
) -> Result<Value> {
    let language = require_language(ctx, "versions")?;
    let forced = manager_filter(ctx);

    // 未指定管理器时，列出所有候选管理器装过的版本（含来源），
    // 这样即使当前运行时来自系统安装，也能看到 nvm/volta 中的历史版本。
    let versions: Vec<Value> = match forced {
        Some(manager) => service
            .installed_versions_for(manager, Some(language))
            .into_iter()
            .map(|version| json!({ "version": version, "manager": manager.id() }))
            .collect(),
        None => service
            .versions_for_language(language)
            .into_iter()
            .map(|item| json!({ "version": item.version, "manager": item.manager }))
            .collect(),
    };

    Ok(json!({
        "kind": "env_versions",
        "tool": language.id(),
        "versions": versions,
    }))
}

pub(super) fn execute_doctor(service: &EnvService, cwd: &Path) -> Value {
    let tools = service.scan(cwd);
    let mut missing = Vec::new();
    let mut conflicts = Vec::new();
    let mut managers: Vec<Value> = Vec::new();
    let mut seen_managers: Vec<ManagerKind> = Vec::new();

    for tool in &tools {
        // 只对“语言运行时”报告缺失；附属工具（npm/poetry/maven…）缺了不算环境问题。
        if !tool.installed && tool.id == tool.language {
            missing.push(json!({ "id": tool.id, "name": tool.name }));
        }
        // 只有语言运行时、且路径确实不在任何已知管理器下，才算 PATH 冲突。
        if tool.installed
            && tool.id == tool.language
            && tool.manager.is_none()
            && !is_system_path(tool.binary_path.as_deref().unwrap_or_default())
        {
            conflicts.push(json!({
                "id": tool.id,
                "binary": tool.binary_path,
                "reason": "binary comes from outside every known version manager",
            }));
        }
        for probe in &tool.candidates {
            if probe.available && !seen_managers.contains(&probe.manager) {
                seen_managers.push(probe.manager);
                managers.push(json!({
                    "manager": probe.manager.id(),
                    "path": probe.path,
                    "root": probe.root,
                }));
            }
        }
    }

    json!({
        "kind": "env_doctor",
        "cwd": cwd.display().to_string(),
        "platform": std::env::consts::OS,
        "managers": managers,
        "missing": missing,
        "conflicts": conflicts,
        "ok": missing.is_empty() && conflicts.is_empty(),
    })
}

/// 判断可执行文件是否来自系统级安装目录（这类来源不算“用户级 PATH 冲突”）。
fn is_system_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let mut prefixes: Vec<String> = vec![
        "c:/windows".into(),
        "c:/program files".into(),
        "c:/program files (x86)".into(),
        "d:/program files".into(),
        "d:/apps/".into(),
        "/usr/bin".into(),
        "/usr/local/bin".into(),
        "/opt/".into(),
        "c:/python".into(),
    ];
    // winget 安装的 App Execution Alias 位于用户的 WindowsApps 目录。
    if let Some(home) = std::env::var_os("LOCALAPPDATA") {
        prefixes.push(
            Path::new(&home)
                .join("Microsoft")
                .join("WindowsApps")
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase(),
        );
    }
    prefixes.iter().any(|prefix| normalized.starts_with(prefix))
}

fn require_language(ctx: &CommandExecutionContext<'_>, subcommand: &str) -> Result<Language> {
    let tool = ctx
        .command()
        .argv
        .args
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!("env {subcommand} requires a tool, e.g. `lan env {subcommand} node`")
        })?;
    Language::parse_id(tool).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported tool `{tool}`; supported: node, python, rust, go, dart, kotlin, java, c"
        )
    })
}

fn parse_manager_id(value: &str) -> Option<ManagerKind> {
    match value {
        "nvm" => Some(ManagerKind::Nvm),
        "fnm" => Some(ManagerKind::Fnm),
        "volta" => Some(ManagerKind::Volta),
        "n" => Some(ManagerKind::N),
        "asdf" => Some(ManagerKind::Asdf),
        "mise" => Some(ManagerKind::Mise),
        "pyenv" => Some(ManagerKind::Pyenv),
        "uv" => Some(ManagerKind::Uv),
        "rustup" => Some(ManagerKind::Rustup),
        "go-install" => Some(ManagerKind::GoInstall),
        "fvm" => Some(ManagerKind::Fvm),
        "sdkman" => Some(ManagerKind::Sdkman),
        "scoop" => Some(ManagerKind::Scoop),
        "winget" => Some(ManagerKind::Winget),
        "brew" => Some(ManagerKind::Brew),
        "apt" => Some(ManagerKind::Apt),
        _ => None,
    }
}
