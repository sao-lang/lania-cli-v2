//! `lan env` 的查询类子命令：list / info / versions / doctor。
//!
//! 全部为只读操作，不经过 dry-run 门控。

use anyhow::Result;
use lania_command::{ArgSpec, CommandSpec, OptionSpec, ValueKind};
use lania_env::{EnvService, InstalledVersion, Language, Platform, RuntimeHome};
use lania_host::execution::CommandExecutionContext;
use serde_json::{json, Value};
use std::path::Path;

use crate::{
    runtime_store, tool_filter, DOCTOR_HANDLER_ID, INFO_HANDLER_ID, LIST_HANDLER_ID,
    VERSIONS_HANDLER_ID,
};

pub(super) fn list_spec() -> CommandSpec {
    CommandSpec::new(
        "list",
        "Show all supported toolchains and detected versions",
        LIST_HANDLER_ID,
    )
    .with_args(vec![ArgSpec {
        name: "tool".into(),
        required: false,
        multiple: false,
        help: "Filter by tool or language name".into(),
    }])
    .with_options(vec![OptionSpec {
        long: "installed".into(),
        short: Some('i'),
        help: "Only list versions installed by lania (reads ~/.runex, no network)".into(),
        value_kind: ValueKind::Bool,
        default_value: None,
        choices: vec![],
        negatable: false,
    }])
}

pub(super) fn info_spec() -> CommandSpec {
    CommandSpec::new(
        "info",
        "Show details for a single toolchain",
        INFO_HANDLER_ID,
    )
    .with_args(vec![tool_arg()])
}

pub(super) fn versions_spec() -> CommandSpec {
    CommandSpec::new(
        "versions",
        "List installed versions of a toolchain",
        VERSIONS_HANDLER_ID,
    )
    .with_args(vec![tool_arg()])
    .with_options(vec![OptionSpec {
        long: "available".into(),
        short: Some('a'),
        help: "Also query upstream for the versions that can be installed".into(),
        value_kind: ValueKind::Bool,
        default_value: None,
        choices: vec![],
        negatable: false,
    }])
}

pub(super) fn doctor_spec() -> CommandSpec {
    CommandSpec::new(
        "doctor",
        "Check missing runtimes, shim gaps and PATH setup",
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
                help: "Version (prefix allowed), e.g. 20 / 3.12 / 1.86.0".into(),
            },
        ])
        .with_options(vec![
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
        help: "Tool to manage: node/python/go/rust/deno/bun/zig".into(),
    }
}

pub(super) fn execute_list(
    service: &EnvService,
    cwd: &Path,
    ctx: &CommandExecutionContext<'_>,
) -> Value {
    let filter = tool_filter(ctx.command());
    if crate::option_bool(ctx, "installed").unwrap_or(false) {
        return list_installed(filter);
    }

    let tools: Vec<Value> = service
        .scan(cwd)
        .into_iter()
        .filter(|tool| {
            filter
                .as_ref()
                .is_none_or(|needle| tool.id.contains(needle) || tool.language.contains(needle))
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

/// `list --installed`：只读 lania 自己的清单，不落盘、不联网。
fn list_installed(filter: Option<String>) -> Value {
    let home = RuntimeHome::resolve().ok();
    let platform = Platform::detect().ok();
    let service = EnvService::new();
    let mut entries: Vec<Value> = Vec::new();

    if let Some(home) = home.as_ref() {
        for id in lania_env::supported_language_ids() {
            if filter
                .as_ref()
                .is_some_and(|needle| !id.contains(needle.as_str()))
            {
                continue;
            }
            let Some(language) = Language::parse_id(id) else {
                continue;
            };
            for version in service.managed_versions(language) {
                entries.push(installed_entry(home, id, &version));
            }
        }
    }

    json!({
        "kind": "env_installed",
        "home": home.as_ref().map(|home| home.root().display().to_string()),
        "platform": platform.map(|platform| platform.to_string()),
        "installed": entries,
    })
}

fn installed_entry(home: &RuntimeHome, id: &str, version: &InstalledVersion) -> Value {
    json!({
        "language": id,
        "version": version.version,
        "current": version.current,
        "path": home.version_dir(id, &version.version).display().to_string(),
        "upstream": lania_env::upstream_for(id),
    })
}

pub(super) fn execute_info(
    service: &EnvService,
    cwd: &Path,
    ctx: &CommandExecutionContext<'_>,
) -> Result<Value> {
    let language = require_language(ctx, "info")?;
    let status = service.status(language, cwd);
    // 已安装版本只读 lania 自己管理的目录。
    let installed: Vec<Value> = service
        .managed_versions(language)
        .into_iter()
        .map(|item| {
            json!({
                "source": item.source,
                "version": item.version,
                "current": item.current,
            })
        })
        .collect();
    let provider = Platform::detect()
        .ok()
        .and_then(|platform| lania_env::describe_provider(language.id(), platform));

    Ok(json!({
        "kind": "env_toolchain_info",
        "tool": serde_json::to_value(&status)?,
        "provider": provider.map(|provider| serde_json::to_value(provider).unwrap_or(Value::Null)),
        "installedVersions": installed,
    }))
}

pub(super) fn execute_versions(
    service: &EnvService,
    ctx: &CommandExecutionContext<'_>,
) -> Result<Value> {
    let language = require_language(ctx, "versions")?;
    let versions: Vec<Value> = service
        .managed_versions(language)
        .into_iter()
        .map(|item| {
            json!({
                "version": item.version,
                "source": item.source,
                "current": item.current,
            })
        })
        .collect();

    // `--available` 需要联网：只在用户显式要求时才查上游。
    let available = if crate::option_bool(ctx, "available").unwrap_or(false) {
        match runtime_store() {
            Ok(store) => store
                .available_versions(language.id())
                .map(|list| {
                    Value::from(
                        list.into_iter()
                            .map(|version| version.raw)
                            .collect::<Vec<_>>(),
                    )
                })
                .unwrap_or_else(|error| json!({ "error": error.to_string() })),
            Err(error) => json!({ "error": error.to_string() }),
        }
    } else {
        Value::Null
    };

    Ok(json!({
        "kind": "env_versions",
        "tool": language.id(),
        "managed": lania_env::supported_language_ids().contains(&language.id()),
        "versions": versions,
        "available": available,
    }))
}

pub(super) fn execute_doctor(service: &EnvService, cwd: &Path) -> Result<Value> {
    let tools = service.scan(cwd);
    let mut missing = Vec::new();
    let mut external = Vec::new();
    let mut managed = Vec::new();
    let mut unsupported = Vec::new();

    for tool in &tools {
        // 只对“语言运行时”报告：附属工具（npm/poetry/maven…）缺了不算环境问题。
        if tool.id != tool.language {
            continue;
        }
        if !tool.installed {
            missing.push(json!({ "id": tool.id, "name": tool.name }));
        } else if tool.source.as_deref() == Some("system") {
            external.push(json!({
                "id": tool.id,
                "binary": tool.binary_path,
                "reason": "installed outside lania (system package manager or manual install)",
            }));
        }
        if tool.managed {
            managed.push(json!({
                "id": tool.id,
                "current": tool.version,
                "source": tool.source,
                "installedCount": tool.installed_count,
                "upstream": tool.upstream,
            }));
        } else {
            unsupported.push(tool.id.clone());
        }
    }

    let diagnostics: Vec<Value> = match runtime_store() {
        Ok(store) => store
            .diagnose()
            .map(|items| {
                items
                    .into_iter()
                    .map(|item| {
                        json!({
                            "level": match item.level {
                                lania_env::DiagnosticLevel::Error => "error",
                                lania_env::DiagnosticLevel::Warn => "warn",
                            },
                            "message": item.message,
                        })
                    })
                    .collect()
            })
            .unwrap_or_else(|error| {
                vec![json!({ "level": "error", "message": error.to_string() })]
            }),
        Err(error) => vec![json!({ "level": "error", "message": error.to_string() })],
    };
    let has_error = diagnostics.iter().any(|item| item["level"] == "error");

    Ok(json!({
        "kind": "env_doctor",
        "cwd": cwd.display().to_string(),
        "platform": std::env::consts::OS,
        "managed": managed,
        "missing": missing,
        "external": external,
        "unsupported": unsupported,
        "diagnostics": diagnostics,
        "ok": missing.is_empty() && !has_error,
    }))
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
            "unsupported tool `{tool}`; supported: {}",
            lania_env::supported_language_ids().join(", ")
        )
    })
}
