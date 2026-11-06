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

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub const HOME_ENV: &str = "RUNEX_HOME";
pub const DEFAULT_HOME_DIR: &str = ".runex";
pub const PATH_ENV: &str = "PATH";

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

    /// 把 shims 目录前置到给定 PATH 值上（纯函数，可测）。
    ///
    /// **约束（违反会导致 PATH 无限增长）**：必须幂等 —— 先剔除已有的 shims 条目再前置。
    pub fn prepend_shims_to_path(&self, current: Option<&str>) -> String {
        let shims = self.shims_dir();
        let display = shims.display().to_string();
        let mut entries: Vec<PathBuf> = current
            .map(std::env::split_paths)
            .map(|paths| paths.collect())
            .unwrap_or_default();
        entries.retain(|entry| !same_dir(entry, &shims));
        // 空条目在 POSIX 上等价于「当前目录」，属于安全隐患，顺带剔除。
        entries.retain(|entry| !entry.as_os_str().is_empty());
        entries.insert(0, shims);

        std::env::join_paths(&entries)
            .map(|joined| joined.to_string_lossy().to_string())
            // join_paths 只在条目里含分隔符时才失败；此时保守返回“只含 shims”的 PATH，
            // 而不是把带分隔符的脏值写进环境变量。
            .unwrap_or(display)
    }

    /// 让当前进程及其后续子进程优先使用自管运行时（幂等）。
    ///
    /// **为什么要在这里改而不是每个 spawn 点改**：子进程继承进程环境，
    /// 所以入口设置一次就能覆盖 `Rust → node → compiler-worker → tool` 整条进程链。
    ///
    /// **约束**：shims 目录不存在时**不前置** —— 把不存在的目录塞进 PATH 会让部分工具报错。
    pub fn activate(&self) {
        let shims = self.shims_dir();
        if !shims.is_dir() {
            return;
        }
        let current = std::env::var_os(PATH_ENV);
        let current = current.as_deref().map(|value| value.to_string_lossy());
        let next = self.prepend_shims_to_path(current.as_deref());
        // 只在 CLI 入口、命令解析之前调用一次；此时没有其他线程并发读环境变量。
        std::env::set_var(PATH_ENV, OsString::from(next));
    }
}

fn same_dir(left: &Path, right: &Path) -> bool {
    // 只做“字面 + 规范化”比较：真实路径解析（symlink）在 PATH 上意义不大，
    // 且 `canonicalize` 要求路径存在，会在 shims 被删后留下陈旧条目。
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
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

    fn split(value: &str) -> Vec<PathBuf> {
        std::env::split_paths(value).collect()
    }

    #[test]
    fn prepend_shims_to_path_puts_shims_first() {
        let home = RuntimeHome::at(std::env::temp_dir().join("runex-prepend"));
        let existing = std::env::join_paths([PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
            .unwrap()
            .to_string_lossy()
            .to_string();

        let next = split(&home.prepend_shims_to_path(Some(&existing)));
        assert_eq!(next.first(), Some(&home.shims_dir()));
        assert_eq!(next.len(), 3);
    }

    #[test]
    fn prepend_shims_to_path_is_idempotent() {
        let home = RuntimeHome::at(std::env::temp_dir().join("runex-idempotent"));
        let once = home.prepend_shims_to_path(Some("/usr/bin"));
        let twice = home.prepend_shims_to_path(Some(&once));

        assert_eq!(once, twice, "repeated prepend must not grow PATH");
        let entries = split(&twice);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| **entry == home.shims_dir())
                .count(),
            1,
            "shims dir must appear exactly once: {entries:?}"
        );
    }

    #[test]
    fn prepend_shims_to_path_drops_stale_shims_entry() {
        let home = RuntimeHome::at(std::env::temp_dir().join("runex-stale"));
        let stale = std::env::join_paths([PathBuf::from("/usr/bin"), home.shims_dir()])
            .unwrap()
            .to_string_lossy()
            .to_string();

        let entries = split(&home.prepend_shims_to_path(Some(&stale)));
        assert_eq!(entries.first(), Some(&home.shims_dir()));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| **entry == home.shims_dir())
                .count(),
            1
        );
    }

    #[test]
    fn prepend_shims_to_path_handles_empty_path() {
        let home = RuntimeHome::at(std::env::temp_dir().join("runex-empty"));
        assert_eq!(
            split(&home.prepend_shims_to_path(None)),
            vec![home.shims_dir()]
        );
        assert_eq!(
            split(&home.prepend_shims_to_path(Some(""))),
            vec![home.shims_dir()]
        );
    }

    #[test]
    fn activate_skips_when_shims_dir_missing() {
        let home = RuntimeHome::at(std::env::temp_dir().join("runex-activate-missing"));
        let before = std::env::var_os(PATH_ENV);
        home.activate();
        assert_eq!(std::env::var_os(PATH_ENV), before);
    }
}
