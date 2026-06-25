//! `GitService` 的主入口与跨职责的仓库级能力。
//!
//! 这个目录按职责拆分实现，避免单文件持续膨胀：
//! - `planning`: 只拼接 git 参数，适合 dry-run / 日志 / 回放
//! - `branch`: 分支、upstream，以及 merge/rebase/revert 等历史改写操作
//! - `remote`: clone、远端列表、push/pull 等远程交互
//! - `history`: 最近提交、提交文件、commit log 查询
//! - `workspace`: 工作区、暂存区、标签与用户配置
//! - `exec`: `GitService` 与 `ExecService` 的执行接缝

mod branch;
mod exec;
mod history;
mod planning;
mod remote;
mod workspace;

use std::path::Path;

use anyhow::Result;
use lania_exec::ExecService;

use crate::{GitError, GitErrorCode, GitStatus};

#[derive(Debug, Clone)]
pub struct GitService {
    binary: String,
    exec: ExecService,
}

impl GitService {
    pub fn new(binary: impl Into<String>) -> Self {
        Self::with_exec(binary, ExecService::default())
    }

    pub fn with_exec(binary: impl Into<String>, exec: ExecService) -> Self {
        Self {
            binary: binary.into(),
            exec,
        }
    }

    pub fn status(&self, cwd: impl AsRef<Path>) -> Result<GitStatus> {
        let cwd = cwd.as_ref();
        // 非 git 仓库对上层来说是“待初始化”的状态，而不是硬错误。
        let inside = match self.run(cwd, ["rev-parse", "--is-inside-work-tree"]) {
            Ok(output) => output,
            Err(error)
                if error
                    .downcast_ref::<GitError>()
                    .is_some_and(|git| git.code == GitErrorCode::NotRepository) =>
            {
                return Ok(GitStatus {
                    ready: false,
                    branch: None,
                });
            }
            Err(error) => return Err(error),
        };

        if inside.trim() != "true" {
            return Ok(GitStatus {
                ready: false,
                branch: None,
            });
        }

        Ok(GitStatus {
            ready: true,
            branch: self.current_branch(cwd)?,
        })
    }

    pub fn init(&self, cwd: impl AsRef<Path>) -> Result<()> {
        let _ = self.run_owned(cwd.as_ref(), self.plan_init())?;
        Ok(())
    }

    pub fn is_installed(&self) -> bool {
        self.run(Path::new("."), ["version"]).is_ok()
    }

    pub fn version(&self) -> Result<String> {
        self.run(Path::new("."), ["version"])
    }

    pub fn is_init(&self, cwd: impl AsRef<Path>) -> bool {
        self.run(cwd.as_ref(), ["rev-parse", "--git-dir"]).is_ok()
    }
}

impl Default for GitService {
    fn default() -> Self {
        Self::new("git")
    }
}
