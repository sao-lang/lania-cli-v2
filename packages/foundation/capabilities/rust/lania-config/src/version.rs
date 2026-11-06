//! 配置 schema 版本策略。
//!
//! `lan.config` 带 `version` 字段时，宿主需要决定：
//! - 这份配置是否仍然可用（Compatible）
//! - 是否建议用户迁移/重写（RewriteRecommended）
//! - 是否已经超过宿主支持范围（Unsupported）
//!
//! `normalize.rs` 会把这里的结果提前算出来，放进 `LanConfigSnapshot.version_strategy`，
//! 让下游逻辑不必反复比较版本号。

use crate::{ConfigMigrationPolicy, ConfigVersionStrategy, CURRENT_LAN_CONFIG_VERSION};

pub(crate) fn version_strategy(detected_version: u32) -> ConfigVersionStrategy {
    ConfigVersionStrategy {
        current_version: CURRENT_LAN_CONFIG_VERSION,
        minimum_compatible_version: 1,
        detected_version,
        policy: if detected_version < CURRENT_LAN_CONFIG_VERSION {
            ConfigMigrationPolicy::RewriteRecommended
        } else if detected_version == CURRENT_LAN_CONFIG_VERSION {
            ConfigMigrationPolicy::Compatible
        } else {
            ConfigMigrationPolicy::Unsupported
        },
    }
}
