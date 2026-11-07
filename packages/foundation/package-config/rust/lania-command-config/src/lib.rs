//! 命令子包 `config` 子树的**唯一**实现（拆分文档决策 #11 / §10.2）。
//!
//! 每个命令子包都挂同一棵 `config` 树（`config` / `get` / `set` / `init` /
//! `doctor` / `sync`），逻辑只在这里写一遍；各包只在自己的 `config.rs` 里
//! 声明「特有 key」并转发到本 crate。三类作用域：
//!
//! - **包级偏好**：`~/.lania/config/<pkg>.json`，由 `lania-package-config`
//!   按包隔离，key 按 `ConfigKey` 白名单校验；
//! - **全局偏好**：`--global` 时读写 `~/.lania/preferences.json`（3 键
//!   `locale` / `output.mode` / `log.timestamps`），复用 `lania-preferences`；
//! - **init/doctor/sync**：对全局偏好 + 项目配置（`lan.config.*`）做只读 /
//!   初始化报告。总包 `lan config` 的 `project_config` 实现是私有的，这里
//!   不复制它，只做轻量的「存在性 + 建议」报告。
//!
//! ## 为什么是独立 crate、且返回纯数据
//!
//! `CommandExecutionContext` 来自 `lania-host`，而 `lania-host` 依赖
//! `lania-package-config`。若把命令实现塞进 `lania-package-config` 就会形成
//! 依赖环。本 crate 作为更高层，只依赖 `lania-command` /
//! `lania-package-config` / `lania-preferences` 三个纯数据 crate，
//! **不依赖 `lania-host`**：`execute_config` 返回 `serde_json::Value`，
//! 由各包在自己的 handler 里用 `ctx.complete_template_info(value, 0)` 包装，
//! 因此天然无环，也满足「各包一行调用」的最小胶水。

use anyhow::{anyhow, bail, Context, Result};
use lania_command::{ArgSpec, CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_package_config::{
    get_value, load_package_config, package_config_path, save_package_config, set_value,
};
use lania_preferences::{
    load_preferences, normalize_locale, normalize_output_mode, preferences_path, save_preferences,
    UserPreferences,
};
use serde_json::{json, Map, Value};

/// 各包 `config_keys()` 的返回元素类型（免得每个包都直接依赖 `lania-package-config`）。
pub use lania_package_config::ConfigKey;

/// 项目配置文件的候选名（只读探测，不解析内容，避免复制总包的实现）。
const PROJECT_CONFIG_CANDIDATES: &[&str] = &[
    "lan.config.ts",
    "lan.config.mts",
    "lan.config.js",
    "lan.config.mjs",
    "lan.config.json",
    "lan.config.yaml",
    "lan.config.yml",
];

// ─────────────────────────────────────────────────────────────
// 公共键 / handler id（各包 configSchema 与注册都以此为准）
// ─────────────────────────────────────────────────────────────

/// 三个与全局偏好同名的通用键；每个子包的 `configSchema` 都必须声明它们，
/// 这样总包 `lan config apply` / `get --package` 才能读写同一套键名。
pub fn common_keys() -> Vec<ConfigKey> {
    vec![
        ConfigKey::enumeration("locale", &["zh", "en"], "en"),
        ConfigKey::enumeration("output.mode", &["json", "human"], "json"),
        ConfigKey::boolean("log.timestamps", false),
    ]
}

/// 完整 key 集 = 通用键 + 各包特有键（特有键只写一遍，通用键不重复声明）。
fn all_keys(package_keys: &[ConfigKey]) -> Vec<ConfigKey> {
    let mut keys = common_keys();
    keys.extend(package_keys.iter().cloned());
    keys
}

/// 本包 config 子树的 6 个 handler id，供各包 `setup()` 注册。
pub fn handler_ids(pkg: &str) -> Vec<String> {
    ["", ".get", ".set", ".init", ".doctor", ".sync"]
        .iter()
        .map(|suffix| format!("command.{pkg}.config{suffix}"))
        .collect()
}

/// 该 `handler_id` 是否属于某包的 config 子树（供各包 handler 分派）。
pub fn is_config_handler_id(pkg: &str, handler_id: &str) -> bool {
    let base = format!("command.{pkg}.config");
    handler_id == base || handler_id.starts_with(&format!("{base}."))
}

/// `command.<pkg>.config.<sub>` 中的 `<sub>`；根节点返回 `""`。
fn subcommand<'a>(pkg: &str, handler_id: &'a str) -> &'a str {
    let prefix = format!("command.{pkg}.config.");
    handler_id.strip_prefix(&prefix).unwrap_or("")
}

// ─────────────────────────────────────────────────────────────
// spec：一棵统一的 config 树（英文文案，中文由 localization 统一覆写）
// ─────────────────────────────────────────────────────────────

fn flag(long: &str, help: &str) -> OptionSpec {
    OptionSpec {
        long: long.into(),
        short: None,
        help: help.into(),
        value_kind: ValueKind::Bool,
        default_value: None,
        choices: vec![],
        negatable: false,
    }
}

/// 生成完整 `config` 树：root(bare) + get + set + init + doctor + sync，
/// 含 `--global` / `--force` / `--write` 选项与英文 help/example。
pub fn config_spec(pkg: &str, package_keys: &[ConfigKey]) -> CommandSpec {
    let keys = all_keys(package_keys);
    let key_list = keys
        .iter()
        .map(|key| key.key.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    let key_help = format!("Preference key: {key_list}");
    let value_help = format!("Preference value (validated against the {pkg} configSchema)");
    let ex = |suffix: &str| format!("lan {pkg} config{suffix}");
    let id = |suffix: &str| format!("command.{pkg}.config{suffix}");

    CommandSpec::new(
        "config",
        format!("Manage {pkg} package preferences and global CLI configuration"),
        id(""),
    )
    .with_options(vec![flag(
        "global",
        "Operate on global CLI preferences instead of the package store",
    )])
    .with_examples(vec![
        Example {
            command: ex(""),
            description: format!("Show {pkg} package preferences and their storage path"),
        },
        Example {
            command: ex(" get locale"),
            description: "Read a preference (omit --global for package scope)".into(),
        },
        Example {
            command: ex(" set output.mode json"),
            description: "Write a package preference".into(),
        },
        Example {
            command: ex(" --global"),
            description: "Show global CLI preferences".into(),
        },
        Example {
            command: ex(" init"),
            description: "Initialize global preferences and report the project config".into(),
        },
        Example {
            command: ex(" doctor"),
            description: "Read-only diagnostics for preferences and project config".into(),
        },
        Example {
            command: ex(" sync"),
            description: "List config sync suggestions between global and package scopes".into(),
        },
    ])
    .with_subcommands(vec![
        CommandSpec::new(
            "get",
            "Read a preference (omit <key> to print all)",
            id(".get"),
        )
        .with_args(vec![ArgSpec {
            name: "key".into(),
            required: false,
            multiple: false,
            help: key_help.clone(),
        }])
        .with_options(vec![flag(
            "global",
            "Read from global CLI preferences instead of the package store",
        )]),
        CommandSpec::new("set", "Update a preference", id(".set"))
            .with_args(vec![
                ArgSpec {
                    name: "key".into(),
                    required: true,
                    multiple: false,
                    help: key_help,
                },
                ArgSpec {
                    name: "value".into(),
                    required: true,
                    multiple: false,
                    help: value_help,
                },
            ])
            .with_options(vec![flag(
                "global",
                "Write global CLI preferences instead of the package store",
            )]),
        CommandSpec::new(
            "init",
            "Initialize global CLI preferences and report the project config",
            id(".init"),
        )
        .with_options(vec![flag("force", "Overwrite an existing preferences file")]),
        CommandSpec::new(
            "doctor",
            "Read-only diagnostics for global preferences and project config",
            id(".doctor"),
        ),
        CommandSpec::new(
            "sync",
            "Suggest config entries to sync between global and package scopes",
            id(".sync"),
        )
        .with_options(vec![flag("write", "Write the suggested config entries")]),
    ])
}

// ─────────────────────────────────────────────────────────────
// 执行：统一实现，返回纯数据 Value
// ─────────────────────────────────────────────────────────────

/// 统一执行入口。各包 handler 里：
/// `let value = execute_config(pkg, &config_keys, ctx.command())?;`
/// 然后 `Ok(ctx.complete_template_info(value, 0))`。
pub fn execute_config(
    pkg: &str,
    package_keys: &[ConfigKey],
    cmd: &CommandContext,
) -> Result<Value> {
    let keys = all_keys(package_keys);
    let sub = subcommand(pkg, &cmd.handler_id);
    let global = option_bool(cmd, "global");
    match sub {
        "init" => execute_init(pkg, cmd),
        "doctor" => execute_doctor(pkg),
        "sync" => execute_sync(pkg, &keys, cmd),
        "get" => execute_get(pkg, &keys, cmd, global),
        "set" => execute_set(pkg, &keys, cmd, global),
        _ => execute_view(pkg, &keys, global),
    }
}

fn option_bool(cmd: &CommandContext, name: &str) -> bool {
    cmd.argv
        .options
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn arg_string(cmd: &CommandContext, name: &str) -> Option<String> {
    cmd.argv
        .args
        .get(name)
        .and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
}

fn find_key<'a>(pkg: &str, keys: &'a [ConfigKey], name: &str) -> Result<&'a ConfigKey> {
    keys.iter().find(|key| key.key == name).ok_or_else(|| {
        let known = keys
            .iter()
            .map(|key| key.key.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        anyhow!("unknown {pkg} config key `{name}`; known keys: {known}")
    })
}

fn effective_value(config: &Map<String, Value>, key: &ConfigKey) -> Value {
    get_value(config, &key.key)
        .cloned()
        .unwrap_or_else(|| key.default.clone())
}

fn package_path_text(pkg: &str) -> Option<String> {
    package_config_path(pkg)
        .ok()
        .flatten()
        .map(|path| path.display().to_string())
}

fn prefs_path_text() -> Option<String> {
    preferences_path().map(|path| path.display().to_string())
}

fn prefs_view(prefs: &UserPreferences) -> Value {
    json!({
        "locale": prefs.locale,
        "log": { "timestamps": prefs.log_timestamps },
        "output": { "mode": prefs.output_mode },
    })
}

fn global_value(prefs: &UserPreferences, key: &str) -> Option<Value> {
    match key {
        "locale" => Some(json!(prefs.locale)),
        "output.mode" => Some(json!(prefs.output_mode)),
        "log.timestamps" => Some(json!(prefs.log_timestamps)),
        _ => None,
    }
}

fn key_entry(key: &ConfigKey, value: Value) -> Value {
    json!({
        "key": key.key,
        "kind": key.kind,
        "choices": key.choices,
        "default": key.default,
        "value": value,
    })
}

/// `lan <pkg> config` / `config --global`：列出全部 key 的当前值 + 默认值 + 存储路径。
fn execute_view(pkg: &str, keys: &[ConfigKey], global: bool) -> Result<Value> {
    if global {
        let prefs = load_preferences();
        let entries: Vec<Value> = keys
            .iter()
            .filter_map(|key| global_value(&prefs, &key.key).map(|value| key_entry(key, value)))
            .collect();
        return Ok(json!({
            "kind": format!("{pkg}_config"),
            "scope": "global",
            "path": prefs_path_text(),
            "config": prefs_view(&prefs),
            "keys": entries,
        }));
    }

    let config = load_package_config(pkg)?;
    let entries: Vec<Value> = keys
        .iter()
        .map(|key| key_entry(key, effective_value(&config, key)))
        .collect();
    Ok(json!({
        "kind": format!("{pkg}_config"),
        "scope": "package",
        "package": pkg,
        "path": package_path_text(pkg),
        "keys": entries,
    }))
}

/// `lan <pkg> config get [key]`：单值（省略 key 时与 bare 等价）。
fn execute_get(
    pkg: &str,
    keys: &[ConfigKey],
    cmd: &CommandContext,
    global: bool,
) -> Result<Value> {
    let Some(name) = arg_string(cmd, "key") else {
        return execute_view(pkg, keys, global);
    };
    let key = find_key(pkg, keys, &name)?;

    if global {
        let prefs = load_preferences();
        let value = global_value(&prefs, &key.key)
            .ok_or_else(|| anyhow!("`{name}` is not a global CLI preference"))?;
        return Ok(json!({
            "kind": format!("{pkg}_config_value"),
            "scope": "global",
            "path": prefs_path_text(),
            "key": key.key,
            "value": value,
            "default": key.default,
        }));
    }

    let config = load_package_config(pkg)?;
    Ok(json!({
        "kind": format!("{pkg}_config_value"),
        "scope": "package",
        "package": pkg,
        "path": package_path_text(pkg),
        "key": key.key,
        "value": effective_value(&config, key),
        "default": key.default,
    }))
}

/// `lan <pkg> config set <key> <value>`：按 `ConfigKey` 校验后写盘。
fn execute_set(
    pkg: &str,
    keys: &[ConfigKey],
    cmd: &CommandContext,
    global: bool,
) -> Result<Value> {
    let name = arg_string(cmd, "key").ok_or_else(|| {
        anyhow!("{pkg} config set requires a key, e.g. `lan {pkg} config set locale zh`")
    })?;
    let raw = arg_string(cmd, "value").ok_or_else(|| {
        anyhow!("{pkg} config set requires a value, e.g. `lan {pkg} config set locale zh`")
    })?;
    let key = find_key(pkg, keys, &name)?;
    let parsed = key
        .parse(&raw)
        .with_context(|| format!("invalid value for `{name}`"))?;

    if global {
        let mut prefs = load_preferences();
        apply_global(&mut prefs, &key.key, &parsed)?;
        save_preferences(&prefs)?;
        return Ok(json!({
            "kind": format!("{pkg}_config_set"),
            "scope": "global",
            "path": prefs_path_text(),
            "key": key.key,
            "value": parsed,
        }));
    }

    let mut config = load_package_config(pkg)?;
    let previous = get_value(&config, &key.key).cloned();
    set_value(&mut config, &key.key, parsed.clone())?;
    save_package_config(pkg, &config)?;
    Ok(json!({
        "kind": format!("{pkg}_config_set"),
        "scope": "package",
        "package": pkg,
        "path": package_path_text(pkg),
        "key": key.key,
        "value": parsed,
        "previous": previous,
    }))
}

fn apply_global(prefs: &mut UserPreferences, key: &str, parsed: &Value) -> Result<()> {
    match key {
        "locale" => prefs.locale = normalize_locale(parsed.as_str().unwrap_or("en")),
        "output.mode" => {
            prefs.output_mode = normalize_output_mode(parsed.as_str().unwrap_or("json"))
        }
        "log.timestamps" => prefs.log_timestamps = parsed.as_bool().unwrap_or(false),
        _ => bail!("`{key}` is not a global CLI preference"),
    }
    Ok(())
}

/// `lan <pkg> config init`：初始化全局偏好文件（`--force` 覆盖）并报告项目配置。
fn execute_init(pkg: &str, cmd: &CommandContext) -> Result<Value> {
    let force = option_bool(cmd, "force");
    let path = preferences_path();
    let existed = path.as_ref().map(|path| path.exists()).unwrap_or(false);
    let prefs = load_preferences();
    let written = if force || !existed {
        save_preferences(&prefs)?;
        true
    } else {
        false
    };
    Ok(json!({
        "kind": format!("{pkg}_config_init"),
        "scope": "global",
        "path": prefs_path_text(),
        "written": written,
        "existed": existed,
        "config": prefs_view(&prefs),
        "packageConfigPath": package_path_text(pkg),
        "projectConfig": project_config_report(cmd),
    }))
}

/// `lan <pkg> config doctor`：只读诊断全局 / 包级偏好文件与项目配置。
fn execute_doctor(pkg: &str) -> Result<Value> {
    let mut issues: Vec<Value> = Vec::new();
    let path = preferences_path();
    match &path {
        None => issues.push(json!({
            "code": "no_home", "severity": "warning",
            "message": "HOME is unset; global preferences cannot be persisted",
        })),
        Some(path) if !path.exists() => issues.push(json!({
            "code": "preferences_missing", "severity": "info",
            "message": "~/.lania/preferences.json does not exist yet; defaults are in use",
        })),
        Some(path) => {
            if let Ok(raw) = std::fs::read_to_string(path) {
                if serde_json::from_str::<UserPreferences>(&raw).is_err() {
                    issues.push(json!({
                        "code": "preferences_invalid", "severity": "error",
                        "message": "~/.lania/preferences.json is not valid; defaults are in use",
                    }));
                }
            }
        }
    }

    let package_path = package_config_path(pkg).ok().flatten();
    if let Some(path) = &package_path {
        if path.exists() {
            if let Ok(raw) = std::fs::read_to_string(path) {
                if serde_json::from_str::<Value>(&raw).is_err() {
                    issues.push(json!({
                        "code": "package_config_invalid", "severity": "error",
                        "message": format!("~/.lania/config/{pkg}.json is not valid JSON"),
                    }));
                }
            }
        }
    }

    let healthy = issues
        .iter()
        .all(|issue| issue["severity"].as_str() != Some("error"));
    Ok(json!({
        "kind": format!("{pkg}_config_doctor"),
        "scope": "global",
        "path": prefs_path_text(),
        "packagePath": package_path.map(|path| path.display().to_string()),
        "healthy": healthy,
        "issues": issues,
    }))
}

/// `lan <pkg> config sync`：列出「全局偏好 → 本包 config」的同步建议；
/// `--write` 时把全局 3 键镜像写入包级 store（不触碰项目 lan.config）。
fn execute_sync(pkg: &str, keys: &[ConfigKey], cmd: &CommandContext) -> Result<Value> {
    let write = option_bool(cmd, "write");
    let prefs = load_preferences();
    let suggestions: Vec<Value> = keys
        .iter()
        .filter_map(|key| {
            global_value(&prefs, &key.key).map(|value| {
                json!({
                    "key": key.key,
                    "value": value,
                    "reason": "mirror the global CLI preference into this package store",
                })
            })
        })
        .collect();

    let mut written = false;
    if write && !suggestions.is_empty() {
        let mut config = load_package_config(pkg)?;
        for suggestion in &suggestions {
            let key = suggestion["key"]
                .as_str()
                .context("sync suggestion missing key")?;
            set_value(&mut config, key, suggestion["value"].clone())?;
        }
        save_package_config(pkg, &config)?;
        written = true;
    }

    Ok(json!({
        "kind": format!("{pkg}_config_sync"),
        "scope": "package",
        "package": pkg,
        "path": package_path_text(pkg),
        "written": written,
        "globalPath": prefs_path_text(),
        "global": prefs_view(&prefs),
        "suggestions": suggestions,
    }))
}

/// 项目配置只读探测：只报告 `lan.config.*` 是否存在，不解析 / 不写盘。
fn project_config_report(cmd: &CommandContext) -> Value {
    let cwd = if cmd.cwd.is_empty() { "." } else { &cmd.cwd };
    let dir = std::path::Path::new(cwd);
    let existing: Vec<&str> = PROJECT_CONFIG_CANDIDATES
        .iter()
        .copied()
        .filter(|name| dir.join(name).is_file())
        .collect();
    json!({
        "cwd": cwd,
        "candidates": PROJECT_CONFIG_CANDIDATES,
        "existing": existing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_keys_match_global_preferences() {
        let keys = common_keys();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].key, "locale");
        assert_eq!(keys[0].default, json!("en"));
        assert_eq!(keys[1].key, "output.mode");
        assert_eq!(keys[1].default, json!("json"));
        assert_eq!(keys[2].key, "log.timestamps");
        assert_eq!(keys[2].default, json!(false));
    }

    #[test]
    fn handler_ids_cover_the_whole_config_tree() {
        let ids = handler_ids("add");
        assert_eq!(
            ids,
            vec![
                "command.add.config",
                "command.add.config.get",
                "command.add.config.set",
                "command.add.config.init",
                "command.add.config.doctor",
                "command.add.config.sync",
            ]
        );
        for id in &ids {
            assert!(is_config_handler_id("add", id));
        }
        // 全局 `lan config` 不属于任何包级 config 子树。
        assert!(!is_config_handler_id("add", "command.config"));
        assert!(!is_config_handler_id("add", "command.config.get"));
        assert!(!is_config_handler_id("add", "command.add.list"));
    }

    #[test]
    fn config_spec_has_all_subcommands_and_flags() {
        let spec = config_spec("add", &[]);
        assert_eq!(spec.name, "config");
        assert_eq!(spec.handler_id, "command.add.config");
        assert!(spec.options.iter().any(|option| option.long == "global"));
        let subs: Vec<&str> = spec.subcommands.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(subs, vec!["get", "set", "init", "doctor", "sync"]);
        let init = &spec.subcommands[2];
        assert!(init.options.iter().any(|option| option.long == "force"));
        let sync = &spec.subcommands[4];
        assert!(sync.options.iter().any(|option| option.long == "write"));
    }
}
