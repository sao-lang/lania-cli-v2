//! `lan workspace config`：workspace 命令包的包级偏好（决策 #11 / §10.2）。
//!
//! 完整 `config` 树（bare/get/set/init/doctor/sync、`--global` 与全局偏好）
//! 由共享 crate `lania-command-config` 统一实现；本文件只声明本包特有 key
//! 并转发 `config_spec`，不重复任何执行逻辑。
//!
//! 注意：原 dev/build/lint/create/add 5 个包各自的 config handler 不迁移
//! （它们随旧包消亡），workspace 只保留自己的 config 子树。
use lania_command::CommandSpec;
use lania_command_config::ConfigKey;

/// 本包特有偏好 key。通用键（`locale` / `output.mode` / `log.timestamps`）
/// 由共享 crate 统一注入，与 `configSchema` 及全局 `lan config` 对齐。
pub(crate) fn config_keys() -> Vec<ConfigKey> {
    vec![]
}

/// 供 workspace root spec 挂载的 `config` 子树。
pub(crate) fn config_spec() -> CommandSpec {
    lania_command_config::config_spec(crate::COMMAND_NAME, &config_keys())
}
