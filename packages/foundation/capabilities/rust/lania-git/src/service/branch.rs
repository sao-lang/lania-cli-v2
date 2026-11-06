//! 分支拓扑、upstream 元数据，以及会改写提交图的命令。

use std::path::Path;

use anyhow::{anyhow, Result};

use crate::{
    non_empty, split_lines, GitError, GitErrorCode, GitMergeOptions, GitRebaseOptions,
    GitRevertOptions, GitUpstream,
};

use super::GitService;

impl GitService {
    pub fn current_branch(&self, cwd: impl AsRef<Path>) -> Result<Option<String>> {
        let branch = self.run(cwd.as_ref(), ["branch", "--show-current"])?;
        Ok(non_empty(branch))
    }

    pub fn list_local_branches(&self, cwd: impl AsRef<Path>) -> Result<Vec<String>> {
        self.run(
            cwd.as_ref(),
            ["branch", "--list", "--format=%(refname:short)"],
        )
        .map(split_lines)
    }

    pub fn list_remote_branches(&self, cwd: impl AsRef<Path>) -> Result<Vec<String>> {
        self.run(
            cwd.as_ref(),
            ["branch", "-r", "--list", "--format=%(refname:short)"],
        )
        .map(split_lines)
    }

    pub fn list_all_branches(&self, cwd: impl AsRef<Path>) -> Result<(Vec<String>, Vec<String>)> {
        let cwd = cwd.as_ref();
        Ok((
            self.list_local_branches(cwd)?,
            self.list_remote_branches(cwd)?,
        ))
    }

    pub fn branch_exists(&self, cwd: impl AsRef<Path>, branch: &str) -> Result<bool> {
        let (local, remote) = self.list_all_branches(cwd)?;
        Ok(local.iter().any(|item| item == branch)
            || remote
                .iter()
                .any(|item| item.ends_with(&format!("/{branch}"))))
    }

    pub fn branch_exists_local(&self, cwd: impl AsRef<Path>, branch: &str) -> Result<bool> {
        Ok(self
            .list_local_branches(cwd)?
            .iter()
            .any(|item| item == branch))
    }

    pub fn branch_exists_remote(&self, cwd: impl AsRef<Path>, branch: &str) -> Result<bool> {
        Ok(self
            .list_remote_branches(cwd)?
            .iter()
            .any(|item| item.ends_with(&format!("/{branch}"))))
    }

    pub fn branch_create(&self, cwd: impl AsRef<Path>, branch: &str) -> Result<()> {
        // 先尝试 `switch`，失败再回退到老版本 git 也支持的 `checkout`。
        match self.run(cwd.as_ref(), ["switch", "-c", branch]) {
            Ok(_) => Ok(()),
            Err(_) => {
                self.run(cwd.as_ref(), ["checkout", "-b", branch])?;
                Ok(())
            }
        }
    }

    pub fn branch_switch(&self, cwd: impl AsRef<Path>, branch: &str) -> Result<()> {
        match self.run(cwd.as_ref(), ["switch", branch]) {
            Ok(_) => Ok(()),
            Err(_) => {
                self.run(cwd.as_ref(), ["checkout", branch])?;
                Ok(())
            }
        }
    }

    pub fn branch_delete(&self, cwd: impl AsRef<Path>, branch: &str, force: bool) -> Result<()> {
        self.run(
            cwd.as_ref(),
            ["branch", if force { "-D" } else { "-d" }, branch],
        )?;
        Ok(())
    }

    pub fn upstream(&self, cwd: impl AsRef<Path>) -> Result<Option<GitUpstream>> {
        match self.run(
            cwd.as_ref(),
            ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        ) {
            Ok(upstream) => {
                let upstream = upstream.trim();
                let (remote, branch) = upstream
                    .split_once('/')
                    .ok_or_else(|| anyhow!("invalid upstream ref `{upstream}`"))?;
                Ok(Some(GitUpstream {
                    remote: remote.to_string(),
                    branch: branch.to_string(),
                }))
            }
            Err(error) => {
                if error
                    .downcast_ref::<GitError>()
                    .is_some_and(|git| git.code == GitErrorCode::MissingUpstream)
                {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }

    pub fn needs_upstream(&self, cwd: impl AsRef<Path>) -> Result<bool> {
        Ok(self.upstream(cwd)?.is_none())
    }

    pub fn set_upstream(&self, cwd: impl AsRef<Path>, remote: &str, branch: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["push", "--set-upstream", remote, branch])?;
        Ok(())
    }

    pub fn has_unpushed_commits(&self, cwd: impl AsRef<Path>) -> Result<bool> {
        match self.run(cwd.as_ref(), ["rev-list", "--count", "@{u}.."]) {
            Ok(output) => Ok(output.trim().parse::<u64>().unwrap_or_default() > 0),
            Err(error) => {
                if error
                    .downcast_ref::<GitError>()
                    .is_some_and(|git| git.code == GitErrorCode::MissingUpstream)
                {
                    Ok(true)
                } else {
                    Err(error)
                }
            }
        }
    }

    pub fn merge(&self, cwd: impl AsRef<Path>, branch: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["merge", branch])?;
        Ok(())
    }

    pub fn merge_with_options(
        &self,
        cwd: impl AsRef<Path>,
        branch: &str,
        options: GitMergeOptions,
    ) -> Result<()> {
        // 先拼常见的结构化参数，再原样透传额外 flags，兼顾可读性与扩展性。
        let mut args = vec!["merge".to_string()];
        if let Some(strategy) = options.strategy {
            args.push("-s".into());
            args.push(strategy);
        }
        if let Some(message) = options.message {
            args.push("-m".into());
            args.push(message);
        }
        args.push(branch.to_string());
        args.extend(options.flags);
        self.run_owned(cwd.as_ref(), args)?;
        Ok(())
    }

    pub fn merge_no_ff(&self, cwd: impl AsRef<Path>, branch: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["merge", "--no-ff", branch])?;
        Ok(())
    }

    pub fn merge_abort(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["merge", "--abort"])?;
        Ok(())
    }

    pub fn cherry_pick(&self, cwd: impl AsRef<Path>, commit: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["cherry-pick", commit])?;
        Ok(())
    }

    pub fn cherry_pick_continue(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["cherry-pick", "--continue"])?;
        Ok(())
    }

    pub fn cherry_pick_abort(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["cherry-pick", "--abort"])?;
        Ok(())
    }

    pub fn rebase(
        &self,
        cwd: impl AsRef<Path>,
        target_branch: &str,
        options: GitRebaseOptions,
    ) -> Result<()> {
        let mut args = vec!["rebase".to_string()];
        if options.interactive {
            args.push("-i".into());
        }
        if options.root {
            args.push("--root".into());
        }
        if let Some(onto) = options.onto {
            args.push("--onto".into());
            args.push(onto);
        }
        args.push(target_branch.to_string());
        self.run_owned(cwd.as_ref(), args)?;
        Ok(())
    }

    pub fn rebase_abort(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["rebase", "--abort"])?;
        Ok(())
    }

    pub fn rebase_continue(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["rebase", "--continue"])?;
        Ok(())
    }

    pub fn rebase_skip(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["rebase", "--skip"])?;
        Ok(())
    }

    pub fn revert(
        &self,
        cwd: impl AsRef<Path>,
        commits: &[String],
        options: GitRevertOptions,
    ) -> Result<()> {
        let mut args = vec!["revert".to_string()];
        if options.no_commit {
            args.push("--no-commit".into());
        }
        if options.no_edit {
            args.push("--no-edit".into());
        }
        if let Some(mainline) = options.mainline {
            args.push("-m".into());
            args.push(mainline.to_string());
        }
        args.extend(commits.iter().cloned());
        self.run_owned(cwd.as_ref(), args)?;
        Ok(())
    }

    pub fn revert_abort(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["revert", "--abort"])?;
        Ok(())
    }

    pub fn revert_continue(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["revert", "--continue"])?;
        Ok(())
    }
}
