//! 运行时清单的持久化模型。
//!
//! 两处状态：
//! - `~/.runex/state.json`：全局状态（当前版本指针、每个语言的入口列表）
//! - `~/.runex/runtimes/<lang>/<version>/.runex-meta.json`：单版本元数据（来源、校验和、布局）
//!
//! 用「全局指针 + 每版本元数据」而不是把一切塞进一个文件，好处是删除某版本目录
//! 就等于删掉它的全部记录，不会留下悬空条目。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::home::META_FILE;
use crate::Platform;

pub const STATE_VERSION: u32 = 1;

/// `~/.runex/state.json`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeState {
    #[serde(default)]
    pub version: u32,
    /// 每个语言的当前版本指针：`{"node": "20.11.1"}`。
    #[serde(default)]
    pub current: BTreeMap<String, String>,
    /// 每个语言生成的 shim 入口（`use` 时同步更新，供 `init` 重放）。
    #[serde(default)]
    pub entrypoints: BTreeMap<String, Vec<String>>,
    /// 每个语言需要注入的环境变量模板（`{root}` 由解析时展开）。
    #[serde(default)]
    pub env: BTreeMap<String, Vec<(String, String)>>,
}

impl RuntimeState {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self {
                version: STATE_VERSION,
                ..Self::default()
            });
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut state: Self = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if state.version == 0 {
            state.version = STATE_VERSION;
        }
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        crate::store::write_atomic(path, text.as_bytes())
    }

    pub fn current_version(&self, language_id: &str) -> Option<&str> {
        self.current.get(language_id).map(String::as_str)
    }

    pub fn set_current(&mut self, language_id: &str, version: &str) {
        self.current
            .insert(language_id.to_string(), version.to_string());
    }

    pub fn clear_current(&mut self, language_id: &str) {
        self.current.remove(language_id);
    }
}

/// 单个已安装版本的元数据（存在版本目录内）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMeta {
    pub language: String,
    pub version: String,
    /// 上游 tag（如 `v20.11.1`、`2025-04-03`）。
    #[serde(default)]
    pub tag: String,
    pub upstream: String,
    pub artifact: String,
    #[serde(default)]
    pub sha256: Option<String>,
    /// 相对版本目录的 bin 目录。
    pub bin_dir: String,
    /// 环境变量模板（`{root}` 表示版本目录）。
    #[serde(default)]
    pub env: Vec<(String, String)>,
    #[serde(default)]
    pub entrypoints: Vec<String>,
    pub platform: String,
    #[serde(default)]
    pub installed_at: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl InstalledMeta {
    pub fn path_in(version_dir: &Path) -> std::path::PathBuf {
        version_dir.join(META_FILE)
    }

    pub fn load(version_dir: &Path) -> Result<Option<Self>> {
        let path = Self::path_in(version_dir);
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let meta = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Some(meta))
    }

    pub fn save(&self, version_dir: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        crate::store::write_atomic(&Self::path_in(version_dir), text.as_bytes())
    }

    pub fn platform_string(platform: Platform) -> String {
        platform.to_string()
    }
}

/// 「已安装版本」的展示记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledRuntime {
    pub language: String,
    pub version: String,
    pub path: String,
    pub current: bool,
    /// 该版本是否由 lania 自己安装（外部安装的版本只用于展示，不参与 `use`）。
    pub managed: bool,
    #[serde(default)]
    pub bin_dir: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lania-state-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_state_file_yields_default() {
        let dir = temp_dir("missing");
        let state = RuntimeState::load(&dir.join("state.json")).unwrap();
        assert_eq!(state.version, STATE_VERSION);
        assert!(state.current.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_round_trips() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("state.json");
        let mut state = RuntimeState::default();
        state.set_current("node", "20.11.1");
        state
            .entrypoints
            .insert("node".into(), vec!["node".into(), "npm".into()]);
        state.save(&path).unwrap();

        let loaded = RuntimeState::load(&path).unwrap();
        assert_eq!(loaded.current_version("node"), Some("20.11.1"));
        assert_eq!(loaded.entrypoints["node"], vec!["node", "npm"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn meta_round_trips() {
        let dir = temp_dir("meta");
        let meta = InstalledMeta {
            language: "go".into(),
            version: "1.22.5".into(),
            tag: "go1.22.5".into(),
            upstream: "go.dev/dl".into(),
            artifact: "go1.22.5.linux-amd64.tar.gz".into(),
            sha256: None,
            bin_dir: "bin".into(),
            env: vec![("GOROOT".into(), "{root}".into())],
            entrypoints: vec!["go".into()],
            platform: "linux-x64-gnu".into(),
            installed_at: Some("2026-01-01T00:00:00Z".into()),
            notes: vec![],
        };
        meta.save(&dir).unwrap();
        let loaded = InstalledMeta::load(&dir).unwrap().expect("meta present");
        assert_eq!(loaded, meta);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
