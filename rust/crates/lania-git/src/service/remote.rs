//! 远端仓库交互：clone、remote 管理以及同步相关操作。

use std::path::Path;

use anyhow::Result;

use crate::{split_lines, GitRemote};

use super::GitService;

impl GitService {
    pub fn clone_repo(
        &self,
        cwd: impl AsRef<Path>,
        repo_url: &str,
        target_dir: Option<&str>,
    ) -> Result<()> {
        let mut args = vec!["clone".to_string(), repo_url.to_string()];
        if let Some(dir) = target_dir {
            args.push(dir.to_string());
        }
        self.run_owned(cwd.as_ref(), args)?;
        Ok(())
    }

    pub fn remotes(&self, cwd: impl AsRef<Path>) -> Result<Vec<GitRemote>> {
        let output = self.run(cwd.as_ref(), ["remote", "-v"])?;
        let mut remotes = Vec::new();

        for line in split_lines(output) {
            let mut parts = line.split('\t');
            let name = parts.next().unwrap_or_default().trim();
            let url = parts
                .next()
                .unwrap_or_default()
                .replace(" (fetch)", "")
                .replace(" (push)", "");

            if !name.is_empty() && remotes.iter().all(|remote: &GitRemote| remote.name != name) {
                remotes.push(GitRemote {
                    name: name.to_string(),
                    url,
                });
            }
        }

        Ok(remotes)
    }

    pub fn remote_exists(&self, cwd: impl AsRef<Path>, remote: &str) -> Result<bool> {
        Ok(self.remotes(cwd)?.iter().any(|item| item.name == remote))
    }

    pub fn remote_add(&self, cwd: impl AsRef<Path>, name: &str, url: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["remote", "add", name, url])?;
        Ok(())
    }

    pub fn remote_pull(&self, cwd: impl AsRef<Path>, remote: &str, branch: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["pull", remote, branch])?;
        Ok(())
    }

    pub fn remote_push(&self, cwd: impl AsRef<Path>, remote: &str, branch: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["push", remote, branch])?;
        Ok(())
    }

    pub fn remote_status(&self, cwd: impl AsRef<Path>, remote: &str) -> Result<String> {
        self.run(cwd.as_ref(), ["ls-remote", remote])
    }

    pub fn push(&self, cwd: impl AsRef<Path>, remote: &str, branch: &str) -> Result<()> {
        let _ = self.run_owned(cwd.as_ref(), self.plan_push(remote, branch))?;
        Ok(())
    }

    pub fn pull(&self, cwd: impl AsRef<Path>, remote: &str, branch: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["pull", remote, branch])?;
        Ok(())
    }
}
