//! `lan git config`：git 命令包的包级偏好（决策 #11 / §10.2）。
//!
//! 完整 `config` 树（bare/get/set/init/doctor/sync、`--global` 与全局偏好）
//! 由共享 crate `lania-command-config` 统一实现；本文件只声明本包特有 key
//! 并转发 `config_spec`，不重复任何执行逻辑。
use lania_command::CommandSpec;
use lania_command_config::ConfigKey;

/// 本包特有偏好 key。通用键（`locale` / `output.mode` / `log.timestamps`）
/// 由共享 crate 统一注入，与 `configSchema` 及全局 `lan config` 对齐。
pub fn config_keys() -> Vec<ConfigKey> {
    vec![]
}

/// 供 `spec()` 挂载的 `config` 子树。
pub fn config_spec() -> CommandSpec {
    lania_command_config::config_spec("git", &config_keys())
}
