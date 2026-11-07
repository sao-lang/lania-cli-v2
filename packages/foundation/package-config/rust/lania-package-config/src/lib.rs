//! 包级偏好配置引擎（`docs/design/package-split-command-atomization.zh-CN.md` 决策 #11 / §10.2）。
//!
//! 每个命令子包自带 `config` 二级命令，管理**本包**的用户偏好；存储按包隔离：
//!
//! - 路径：`~/.lania/config/<pkg>.json`（`<pkg>` 只允许 `^[a-z][a-z0-9-]*$`，
//!   从根上杜绝路径穿越与跨包文件访问）；
//! - 引擎 API 只能触碰显式传入的 `<pkg>` 命名空间——**包间隔离由引擎保证**，
//!   不依赖调用方自觉；
//! - 全局用户偏好（`~/.lania/preferences.json`）与项目配置（`lan.config.*`）
//!   归总包 `lan config` 独占，本引擎不提供任何触碰它们的路径。
//!
//! key 支持 `a.b.c` 点分形式（映射到嵌套 JSON object）；value 的合法性由
//! 各包 manifest 的 `configSchema` 描述，本 crate 提供 [`ConfigKey`] 做校验
//! （类型 / 枚举 / 默认值）。
use std::{env, fs, path::PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};

/// 包名合法形态（与 manifest `name` 一致）：小写字母开头，仅小写字母数字与 `-`。
pub fn validate_package_name(pkg: &str) -> Result<()> {
    let valid = !pkg.is_empty()
        && pkg.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && pkg
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(())
    } else {
        bail!(
            "invalid package namespace `{pkg}`: expected ^[a-z][a-z0-9-]*$ (path-traversal guard)"
        );
    }
}

/// 某个包的偏好文件路径：`~/.lania/config/<pkg>.json`。
/// 无 `HOME`（Windows 上回退 `USERPROFILE`，与 Node launcher 一致）时返回 `None`。
pub fn package_config_path(pkg: &str) -> Result<Option<PathBuf>> {
    validate_package_name(pkg)?;
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .ok()
        .filter(|home| !home.trim().is_empty());
    let Some(home) = home else {
        return Ok(None);
    };
    Ok(Some(
        PathBuf::from(home.trim())
            .join(".lania")
            .join("config")
            .join(format!("{pkg}.json")),
    ))
}

/// 读取某包的偏好整表；文件不存在 / 解析失败时返回空表（与 preferences 同策略：尽力而为）。
pub fn load_package_config(pkg: &str) -> Result<Map<String, Value>> {
    let Some(path) = package_config_path(pkg)? else {
        return Ok(Map::new());
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        // 文件不存在（未配置过）→ 空表；权限等 IO 错误同样尽力回退，不阻断 CLI。
        return Ok(Map::new());
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => Ok(map),
        // 空文件 / 非 object：视为未配置，回退默认值而非报错。
        _ => Ok(Map::new()),
    }
}

/// 写回某包的偏好整表（自动创建父目录；无 `HOME` 时 no-op 成功，CLI 可继续）。
pub fn save_package_config(pkg: &str, config: &Map<String, Value>) -> Result<()> {
    let Some(path) = package_config_path(pkg)? else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating config dir for `{pkg}`"))?;
    }
    let json = serde_json::to_string_pretty(config)?;
    fs::write(&path, format!("{json}\n"))
        .with_context(|| format!("writing package config for `{pkg}` at {}", path.display()))?;
    Ok(())
}

/// 点分 key 读取：`shim.host` → `config["shim"]["host"]`。
pub fn get_value<'a>(config: &'a Map<String, Value>, dotted_key: &str) -> Option<&'a Value> {
    let mut current: Option<&Value> = None;
    for (index, segment) in dotted_key.split('.').enumerate() {
        let object = if index == 0 {
            config
        } else {
            current?.as_object()?
        };
        current = object.get(segment);
    }
    current
}

/// 点分 key 写入：不存在的中段自动建 object；拒绝用标量值当中段。
pub fn set_value(
    config: &mut Map<String, Value>,
    dotted_key: &str,
    value: Value,
) -> Result<()> {
    let segments: Vec<&str> = dotted_key.split('.').collect();
    if segments.iter().any(|s| s.trim().is_empty()) {
        bail!("invalid config key `{dotted_key}`");
    }
    let mut current = config;
    for (index, segment) in segments.iter().enumerate() {
        if index + 1 == segments.len() {
            current.insert((*segment).to_string(), value);
            return Ok(());
        }
        let entry = current
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            bail!("config key `{dotted_key}` conflicts with an existing scalar value");
        }
        current = entry.as_object_mut().expect("just checked is_object");
    }
    Ok(())
}

/// `configSchema` 中单个 key 的声明（与 manifest `configSchema.keys[]` 同构）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigKey {
    pub key: String,
    pub kind: ConfigValueKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<String>>,
    pub default: Value,
}

/// 支持的偏好值类型（首期：字符串 / 布尔 / 枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigValueKind {
    String,
    Bool,
    Enum,
}

impl ConfigKey {
    pub fn string(key: &str, default: &str) -> Self {
        Self {
            key: key.to_string(),
            kind: ConfigValueKind::String,
            choices: None,
            default: Value::String(default.to_string()),
        }
    }

    pub fn boolean(key: &str, default: bool) -> Self {
        Self {
            key: key.to_string(),
            kind: ConfigValueKind::Bool,
            choices: None,
            default: Value::Bool(default),
        }
    }

    pub fn enumeration(key: &str, choices: &[&str], default: &str) -> Self {
        Self {
            key: key.to_string(),
            kind: ConfigValueKind::Enum,
            choices: Some(choices.iter().map(|c| (*c).to_string()).collect()),
            default: Value::String(default.to_string()),
        }
    }

    /// 解析并校验用户输入，产出应写盘的 JSON 值。
    pub fn parse(&self, raw: &str) -> Result<Value> {
        match self.kind {
            ConfigValueKind::Bool => match raw.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Ok(Value::Bool(true)),
                "false" | "0" | "no" | "off" => Ok(Value::Bool(false)),
                _ => bail!("invalid boolean value for `{}`: expected true/false", self.key),
            },
            ConfigValueKind::Enum => {
                let normalized = raw.trim().to_ascii_lowercase();
                let allowed = self
                    .choices
                    .as_ref()
                    .map_or(true, |choices| choices.iter().any(|c| c == &normalized));
                if allowed {
                    Ok(Value::String(normalized))
                } else {
                    bail!(
                        "invalid value for `{}`: expected one of {}",
                        self.key,
                        self.choices.as_deref().unwrap_or(&[]).join(", ")
                    )
                }
            }
            ConfigValueKind::String => {
                if raw.trim().is_empty() {
                    bail!("invalid empty string for `{}`", self.key);
                }
                Ok(Value::String(raw.trim().to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests;
