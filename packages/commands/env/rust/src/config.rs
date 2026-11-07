//! `lan env config`：env 命令包的包级偏好（决策 #11 / 拆分文档 §10.2）。
//!
//! 完整 `config` 树（bare/get/set/init/doctor/sync、`--global` 与全局偏好）
//! 由共享 crate `lania-command-config` 统一实现；本文件只保留 env 特有的
//! `shim.host` 键声明与 `shim_host_preference()` 读取 API（供 `runtime_api()` 用），
//! 不再重复任何命令执行逻辑。
use anyhow::{anyhow, Result};
use lania_command::CommandSpec;
use lania_package_config::{get_value, load_package_config, ConfigKey};
use serde_json::{Map, Value};

/// 本包特有偏好 key：
/// - 通用键（`locale` / `output.mode` / `log.timestamps`）由共享 crate 统一注入，
///   与 `configSchema` 及全局 `lan config` 的 3 键对齐；
/// - `shim.host`：`lan env use/install` 生成 shim 时写死的管理命令——
///   `lan`（默认，`lan env which <name> --resolve`）或 `runex`（`runex which <name>`）。
///   默认值与总包现状逐字节一致；详见拆分文档 §6.6。
pub fn config_keys() -> Vec<ConfigKey> {
    vec![ConfigKey::enumeration("shim.host", &["lan", "runex"], "lan")]
}

/// 供 `spec()` 挂载的 `config` 子树（完整实现来自共享 crate）。
pub fn config_spec() -> CommandSpec {
    lania_command_config::config_spec("env", &config_keys())
}

fn effective_value(config: &Map<String, Value>, key: &ConfigKey) -> Value {
    get_value(config, &key.key)
        .cloned()
        .unwrap_or_else(|| key.default.clone())
}

/// 本包特有键查找：全局 / 通用键不进入这个作用域（由共享 crate 单独处理）。
fn find_key(key_name: &str) -> Result<ConfigKey> {
    config_keys()
        .into_iter()
        .find(|key| key.key == key_name)
        .ok_or_else(|| anyhow!("unknown env config key `{key_name}`; known keys: shim.host"))
}

/// `runtime_api()` 用：读取 `shim.host` 偏好（未配置 / 读盘失败一律回退 `lan`，
/// 保证总包现状零行为变化）。
pub(crate) fn shim_host_preference() -> Option<String> {
    let config = load_package_config("env").ok()?;
    let key = find_key("shim.host").ok()?;
    let value = effective_value(&config, &key);
    value.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lania_package_config::{save_package_config, set_value};
    use serde_json::json;

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
