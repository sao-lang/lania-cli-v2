//! `lan env` 的查询类子命令：list / info / versions / doctor。
//!
//! 全部为只读操作，不经过 dry-run 门控；运行时数据统一来自 `runex` API。

use anyhow::Result;
use lania_command::{ArgSpec, CommandSpec, OptionSpec, ValueKind};
use lania_host::execution::CommandExecutionContext;
use runex::api::{DiagnosticLevel, Language, Runex};
use serde_json::{json, Value};

use crate::{
    tool_filter, DOCTOR_HANDLER_ID, INFO_HANDLER_ID, LIST_HANDLER_ID, VERSIONS_HANDLER_ID,
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

pub(super) fn execute_list(api: &Runex, ctx: &CommandExecutionContext<'_>) -> Value {
    let filter = tool_filter(ctx.command());
    if crate::option_bool(ctx, "installed").unwrap_or(false) {
        return list_installed(api, filter);
    }

    let tools: Vec<Value> = api
        .scan()
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
        "cwd": api.cwd().display().to_string(),
        "platform": std::env::consts::OS,
        "tools": tools,
    })
}

/// `list --installed`：只读 lania 自己的清单，不落盘、不联网。
fn list_installed(api: &Runex, filter: Option<String>) -> Value {
    let mut entries: Vec<Value> = Vec::new();

    if let Ok(rows) = api.installed_all() {
        for row in rows {
            if filter
                .as_ref()
                .is_some_and(|needle| !row.language.contains(needle.as_str()))
            {
                continue;
            }
            entries.push(json!({
                "language": row.language,
                "version": row.version,
                "current": row.current,
                "path": row.path,
                "upstream": runex::api::upstream_for(&row.language),
            }));
        }
    }

    json!({
        "kind": "env_installed",
        "home": api.home().root().display().to_string(),
        "platform": api.platform().to_string(),
        "installed": entries,
    })
}

pub(super) fn execute_info(api: &Runex, ctx: &CommandExecutionContext<'_>) -> Result<Value> {
    let language = require_language(ctx, "info")?;
    let status = api
        .status(language.id())
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    // 已安装版本只读 lania 自己管理的目录。
    let installed: Vec<Value> = api
        .managed_versions(language.id())
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .into_iter()
        .map(|item| {
            json!({
                "source": item.source,
                "version": item.version,
                "current": item.current,
            })
        })
        .collect();
    let provider = api.provider_info(language.id());

    Ok(json!({
        "kind": "env_toolchain_info",
        "tool": serde_json::to_value(&status)?,
        "provider": provider.map(|provider| serde_json::to_value(provider).unwrap_or(Value::Null)),
        "installedVersions": installed,
    }))
}

pub(super) fn execute_versions(api: &Runex, ctx: &CommandExecutionContext<'_>) -> Result<Value> {
    let language = require_language(ctx, "versions")?;
    let versions: Vec<Value> = api
        .managed_versions(language.id())
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
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
        match api.available_versions(language.id()) {
            Ok(list) => Value::from(
                list.into_iter()
                    .map(|version| version.raw)
                    .collect::<Vec<_>>(),
            ),
            Err(error) => json!({ "error": error.to_string() }),
        }
    } else {
        Value::Null
    };

    Ok(json!({
        "kind": "env_versions",
        "tool": language.id(),
        "managed": api.managed_ids().contains(&language.id()),
        "versions": versions,
        "available": available,
    }))
}

pub(super) fn execute_doctor(api: &Runex) -> Result<Value> {
    let tools = api.scan();
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

    let diagnostics: Vec<Value> = match api.diagnose() {
        Ok(items) => items
            .into_iter()
            .map(|item| {
                json!({
                    "level": match item.level {
                        DiagnosticLevel::Error => "error",
                        DiagnosticLevel::Warn => "warn",
                        DiagnosticLevel::Info => "info",
                    },
                    "message": item.message,
                })
            })
            .collect(),
        Err(error) => vec![json!({ "level": "error", "message": error.to_string() })],
    };
    let has_error = diagnostics.iter().any(|item| item["level"] == "error");

    Ok(json!({
        "kind": "env_doctor",
        "cwd": api.cwd().display().to_string(),
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
            runex::api::supported_language_ids().join(", ")
        )
    })
}
