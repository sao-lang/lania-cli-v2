//! `lan env config` 子命令：env 命令包的**包级偏好**（决策 #11 / 拆分文档 §10.2）。
//!
//! 作用域硬锁定 `~/.lania/config/env.json`，key 按 manifest `configSchema` 校验；
//! 引擎（`lania-package-config`）保证本命令**无法**触碰全局偏好
//! （`~/.lania/preferences.json`）、项目配置（`lan.config.*`）或其它包的偏好文件。
use anyhow::Result;
use lania_package_config::{
    get_value, load_package_config, package_config_path, save_package_config, set_value,
    ConfigKey,
};
use serde_json::{json, Value};

use super::{CONFIG_GET_HANDLER_ID, CONFIG_HANDLER_ID, CONFIG_SET_HANDLER_ID};
use lania_command::{ArgSpec, CommandSpec, Example};
use lania_host::execution::{
    CommandExecution, CommandExecutionContext, ExecutionError, EXIT_RUNTIME_ERROR,
};

/// 本包偏好 key 白名单（与 `lania.package.json` 的 `configSchema.keys` 保持一致）。
///
/// - `shim.host`：`lan env use/install` 生成 shim 时写死的管理命令——
///   `lan`（默认，`lan env which <name> --resolve`）或 `runex`（`runex which <name>`）。
///   默认值与总包现状逐字节一致；详见拆分文档 §6.6。
pub fn config_keys() -> Vec<ConfigKey> {
    vec![ConfigKey::enumeration("shim.host", &["lan", "runex"], "lan")]
}

pub fn config_spec() -> CommandSpec {
    CommandSpec::new(
        "config",
        "Manage env package preferences (isolated per-package store)",
        CONFIG_HANDLER_ID,
    )
    .with_subcommands(vec![get_spec(), set_spec()])
    .with_examples(vec![
        Example {
            command: "lan env config".into(),
            description: "Show env package preferences and their storage path".into(),
        },
        Example {
            command: "lan env config get shim.host".into(),
            description: "Read the shim host preference".into(),
        },
        Example {
            command: "lan env config set shim.host runex".into(),
            description: "Make generated shims call `runex which` instead of `lan env which`"
                .into(),
        },
    ])
}

fn get_spec() -> CommandSpec {
    CommandSpec::new(
        "get",
        "Read an env package preference (omit <key> to print all)",
        CONFIG_GET_HANDLER_ID,
    )
    .with_args(vec![ArgSpec {
        name: "key".into(),
        required: false,
        multiple: false,
        help: "Preference key: shim.host".into(),
    }])
}

fn set_spec() -> CommandSpec {
    CommandSpec::new("set", "Update an env package preference", CONFIG_SET_HANDLER_ID)
        .with_args(vec![
            ArgSpec {
                name: "key".into(),
                required: true,
                multiple: false,
                help: "Preference key: shim.host".into(),
            },
            ArgSpec {
                name: "value".into(),
                required: true,
                multiple: false,
                help: "Preference value (enum: lan | runex for shim.host)".into(),
            },
        ])
}

/// 读取位置参数（与 `tool` / `name` 同路径：`argv.args`）。
fn arg_string(ctx: &CommandExecutionContext<'_>, name: &str) -> Option<String> {
    ctx.command()
        .argv
        .args
        .get(name)
        .and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
}

/// 读取偏好值：显式值优先，未配置时回退 schema 默认值。
fn effective_value(config: &serde_json::Map<String, Value>, key: &ConfigKey) -> Value {
    get_value(config, &key.key)
        .cloned()
        .unwrap_or_else(|| key.default.clone())
}

fn storage_path_text() -> Option<String> {
    package_config_path("env")
        .ok()
        .flatten()
        .map(|path| path.display().to_string())
}

/// `lan env config`（bare）：列出全部 key 的当前值 + 默认值 + 存储路径。
fn execute_config_view(ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
    let config = load_package_config("env")?;
    let keys: Vec<Value> = config_keys()
        .iter()
        .map(|key| {
            json!({
                "key": key.key,
                "kind": key.kind,
                "choices": key.choices,
                "default": key.default,
                "value": effective_value(&config, key),
            })
        })
        .collect();
    Ok(ctx.complete_template_info(
        json!({
            "kind": "env_config",
            "scope": "package",
            "package": "env",
            "path": storage_path_text(),
            "keys": keys,
        }),
        0,
    ))
}

/// `lan env config get [key]`：单值（省略 key 时与 bare 等价）。
fn execute_config_get(ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
    let Some(key_name) = arg_string(ctx, "key") else {
        return execute_config_view(ctx);
    };
    let key = find_key(&key_name)?;
    let config = load_package_config("env")?;
    Ok(ctx.complete_template_info(
        json!({
            "kind": "env_config_value",
            "scope": "package",
            "package": "env",
            "path": storage_path_text(),
            "key": key.key,
            "value": effective_value(&config, &key),
            "default": key.default,
        }),
        0,
    ))
}

/// `lan env config set <key> <value>`：校验后写盘。
fn execute_config_set(ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
    let key_name = arg_string(ctx, "key").ok_or_else(|| {
        anyhow::anyhow!("env config set requires a key, e.g. `lan env config set shim.host runex`")
    })?;
    let raw_value = arg_string(ctx, "value").ok_or_else(|| {
        anyhow::anyhow!(
            "env config set requires a value, e.g. `lan env config set shim.host runex`"
        )
    })?;

    let key = find_key(&key_name)?;
    let parsed = key.parse(&raw_value).map_err(|error| ExecutionError {
        exit_code: EXIT_RUNTIME_ERROR,
        message: error.to_string(),
    })?;

    let mut config = load_package_config("env")?;
    let previous = get_value(&config, &key.key).cloned();
    set_value(&mut config, &key.key, parsed.clone()).map_err(|error| ExecutionError {
        exit_code: EXIT_RUNTIME_ERROR,
        message: error.to_string(),
    })?;
    save_package_config("env", &config).map_err(|error| ExecutionError {
        exit_code: EXIT_RUNTIME_ERROR,
        message: error.to_string(),
    })?;

    Ok(ctx.complete_template_info(
        json!({
            "kind": "env_config_set",
            "scope": "package",
            "package": "env",
            "path": storage_path_text(),
            "key": key.key,
            "value": parsed,
            "previous": previous,
        }),
        0,
    ))
}

fn find_key(key_name: &str) -> Result<ConfigKey> {
    config_keys()
        .into_iter()
        .find(|key| key.key == key_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unknown env config key `{key_name}`; known keys: {}",
                ["shim.host"].join(", ")
            )
        })
}

/// 宿主 handler 分派（`lib.rs` 的 match 调用）。
pub(crate) fn execute_config(ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
    match ctx.command().handler_id.as_str() {
        CONFIG_GET_HANDLER_ID => execute_config_get(ctx),
        CONFIG_SET_HANDLER_ID => execute_config_set(ctx),
        _ => execute_config_view(ctx),
    }
}

/// `runtime_api()` 用：读取 `shim.host` 偏好（未配置 / 读盘失败一律回退 `lan`，
/// 保证总包现状零行为变化）。
pub(crate) fn shim_host_preference() -> Option<String> {
    let config = load_package_config("env").ok()?;
    let key = config_keys().into_iter().find(|key| key.key == "shim.host")?;
    let value = effective_value(&config, &key);
    value.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    #[test]
    fn schema_has_shim_host_with_lan_default() {
        let keys = config_keys();
        assert_eq!(keys.len(), 1);
        let key = &keys[0];
        assert_eq!(key.key, "shim.host");
        assert_eq!(key.default, json!("lan"));
        assert_eq!(
            key.choices,
            Some(vec!["lan".to_string(), "runex".to_string()])
        );
    }

    #[test]
    fn preference_defaults_to_lan_when_store_missing() {
        let guard = home_guard();
        assert_eq!(shim_host_preference().as_deref(), Some("lan"));
        drop(guard);
    }

    #[test]
    fn preference_roundtrips_runex() {
        let guard = home_guard();
        let mut config = Map::new();
        set_value(&mut config, "shim.host", json!("runex")).expect("set");
        save_package_config("env", &config).expect("save");
        assert_eq!(shim_host_preference().as_deref(), Some("runex"));
        drop(guard);
    }

    #[test]
    fn unknown_key_is_rejected() {
        assert!(find_key("locale").is_err(), "global keys must stay out of the package scope");
        assert!(find_key("shim.host").is_ok());
    }

    /// 独立临时 HOME + 互斥锁：HOME 是进程级环境变量，两个改写 HOME 的测试必须串行。
    fn home_guard() -> impl Drop {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let lock = LOCK.lock().expect("lock");
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let home = std::env::temp_dir().join(format!(
            "lan-env-config-test-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&home).expect("create home");
        let previous = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);
        struct Guard {
            previous: Option<std::ffi::OsString>,
            _lock: std::sync::MutexGuard<'static, ()>,
        }
        impl Drop for Guard {
            fn drop(&mut self) {
                match &self.previous {
                    Some(value) => std::env::set_var("HOME", value),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
        Guard {
            previous,
            _lock: lock,
        }
    }
}
