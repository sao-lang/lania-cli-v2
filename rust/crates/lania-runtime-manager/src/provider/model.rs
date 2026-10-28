//! Provider 的共享数据模型：版本清单、产物计划、安装布局。
//!
//! 这些类型是 Provider 与 Store/Shim 之间的**唯一契约**：
//! Provider 只负责回答「有哪些版本」「下哪个文件」「怎么摆目录」「需要什么环境变量」，
//! 下载、校验、解压、登记、生成 shim 全部由服务层统一完成。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Platform;

/// 上游发布的一个版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// 规范化版本号（会作为安装目录名），例如 `20.11.1`。
    pub version: String,
    /// 是否为预发布（beta / rc / nightly）。
    pub prerelease: bool,
    /// 上游的原始 tag，供 Provider 拼下载地址时使用（如 `v20.11.1`、`1.86.0`）。
    pub tag: String,
}

/// 一个版本在某平台上要下载的产物。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactPlan {
    pub url: String,
    /// 官方校验和；`None` 表示上游没有提供（会给出提示但不阻断安装）。
    pub sha256: Option<String>,
    /// 官方签名文件地址（记录在元数据里便于审阅，不做验签）。
    pub signature: Option<String>,
    /// 下载文件名（用于缓存与报错信息）。
    pub file_name: String,
}

/// 解压后的目录整理结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// 需要在 shim 中暴露的 bin 目录（相对版本目录）。
    pub bin_dir: PathBuf,
    /// 该运行时需要的环境变量（值里的 `{root}` 会在使用时展开为版本目录）。
    pub env: Vec<(String, String)>,
    /// 需要在 shim 里包住的入口，如 `node`、`npm`、`npx`。
    pub entrypoints: Vec<String>,
    /// 安装过程中需要额外执行的整理动作说明（写入元数据便于排查）。
    pub notes: Vec<String>,
    /// 安装到版本目录后需要置为可执行的文件（zip 产物在 POSIX 上会丢权限位）。
    pub executables: Vec<PathBuf>,
}

impl Layout {
    pub fn new(bin_dir: impl Into<PathBuf>, entrypoints: &[&str]) -> Self {
        Self {
            bin_dir: bin_dir.into(),
            env: Vec::new(),
            entrypoints: entrypoints.iter().map(|item| item.to_string()).collect(),
            notes: Vec::new(),
            executables: Vec::new(),
        }
    }

    pub fn with_env(mut self, key: &str, value: impl Into<String>) -> Self {
        self.env.push((key.to_string(), value.into()));
        self
    }

    pub fn with_executables(mut self, files: &[&str]) -> Self {
        self.executables.extend(files.iter().map(PathBuf::from));
        self
    }

    /// 把 `{root}` 占位符替换成实际版本目录。
    pub fn resolved_env(&self, root: &Path) -> Vec<(String, String)> {
        let root = root.display().to_string();
        self.env
            .iter()
            .map(|(key, value)| (key.clone(), value.replace("{root}", &root)))
            .collect()
    }
}

/// Provider 能提供的全部静态描述（供 CLI/TUI 展示与清单使用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    /// 上游分发渠道（人类可读，如 `nodejs.org/dist`）。
    pub upstream: String,
    /// 该运行时的入口命令。
    pub entrypoints: Vec<String>,
    /// 是否支持当前平台。
    pub supported: bool,
    /// 不支持时的原因。
    pub unsupported_reason: Option<String>,
}

/// Provider 在解析版本时需要的上下文。
#[derive(Debug, Clone, Copy)]
pub struct ResolveContext<'a> {
    pub platform: Platform,
    pub http: &'a crate::HttpClient,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_env_expands_root_placeholder() {
        let layout = Layout::new("bin", &["java"])
            .with_env("JAVA_HOME", "{root}")
            .with_env("EXTRA", "static");
        let resolved = layout.resolved_env(Path::new("/opt/runex/java/21"));
        assert_eq!(
            resolved,
            vec![
                ("JAVA_HOME".to_string(), "/opt/runex/java/21".to_string()),
                ("EXTRA".to_string(), "static".to_string()),
            ]
        );
    }
}
