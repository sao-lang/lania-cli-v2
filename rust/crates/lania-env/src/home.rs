//! 运行时根目录（`~/.runex`）的目录布局。
//!
//! ```text
//! ~/.runex/                     # 可用 RUNEX_HOME 覆盖
//! ├── runtimes/<lang>/<version>/
//! ├── shims/
//! ├── cache/downloads/
//! └── state.json
//! ```
//!
//! 项目级 `.lania/` 与这里职责不同（项目状态 vs 全局运行时），互不影响。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub const HOME_ENV: &str = "RUNEX_HOME";
pub const DEFAULT_HOME_DIR: &str = ".runex";

/// 单个已安装版本在其版本目录内登记的元数据文件名。
pub const META_FILE: &str = ".runex-meta.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHome {
    root: PathBuf,
}

impl RuntimeHome {
    /// 解析运行时根目录：优先 `RUNEX_HOME`，其次 `~/.runex`。
    pub fn resolve() -> Result<Self> {
        if let Some(value) = std::env::var_os(HOME_ENV) {
            let path = PathBuf::from(value);
            if path.as_os_str().is_empty() {
                anyhow::bail!("{HOME_ENV} is set but empty");
            }
            return Ok(Self::at(path));
        }
        let home = home_dir()
            .context("cannot determine the user home directory; set RUNEX_HOME explicitly")?;
        Ok(Self::at(home.join(DEFAULT_HOME_DIR)))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 供 shell 片段使用的正斜杠路径（Windows 的 `\` 在 sh 里会被当成转义）。
    pub fn display_for_shell(&self, path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn runtimes_dir(&self) -> PathBuf {
        self.root.join("runtimes")
    }

    pub fn language_dir(&self, language_id: &str) -> PathBuf {
        self.runtimes_dir().join(language_id)
    }

    pub fn version_dir(&self, language_id: &str, version: &str) -> PathBuf {
        self.language_dir(language_id).join(version)
    }

    pub fn shims_dir(&self) -> PathBuf {
        self.root.join("shims")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.root.join("cache").join("downloads")
    }

    pub fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    /// 创建运行所需的全部目录。
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [self.runtimes_dir(), self.shims_dir(), self.downloads_dir()] {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        Ok(())
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_derived_from_root() {
        let home = RuntimeHome::at(PathBuf::from("/tmp/runex"));
        assert_eq!(home.runtimes_dir(), PathBuf::from("/tmp/runex/runtimes"));
        assert_eq!(
            home.version_dir("node", "20.11.1"),
            PathBuf::from("/tmp/runex/runtimes/node/20.11.1")
        );
        assert_eq!(home.shims_dir(), PathBuf::from("/tmp/runex/shims"));
        assert_eq!(
            home.downloads_dir(),
            PathBuf::from("/tmp/runex/cache/downloads")
        );
        assert_eq!(home.state_path(), PathBuf::from("/tmp/runex/state.json"));
    }
}
