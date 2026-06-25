//! 工作区、暂存区，以及仓库内的本地元数据操作。

use std::path::Path;

use anyhow::Result;

use crate::{split_lines, GitUser};

use super::GitService;

impl GitService {
    pub fn tags(&self, cwd: impl AsRef<Path>) -> Result<Vec<String>> {
        self.run(cwd.as_ref(), ["tag"]).map(split_lines)
    }

    pub fn tag_create_lightweight(&self, cwd: impl AsRef<Path>, tag: &str) -> Result<()> {
        let _ = self.run_owned(cwd.as_ref(), self.plan_tag_create_lightweight(tag))?;
        Ok(())
    }

    pub fn tag_create_annotated(
        &self,
        cwd: impl AsRef<Path>,
        tag: &str,
        message: &str,
    ) -> Result<()> {
        let _ = self.run_owned(cwd.as_ref(), self.plan_tag_create_annotated(tag, message))?;
        Ok(())
    }

    pub fn tag_delete(&self, cwd: impl AsRef<Path>, tag: &str) -> Result<()> {
        let _ = self.run_owned(cwd.as_ref(), self.plan_tag_delete(tag))?;
        Ok(())
    }

    pub fn user(&self, cwd: impl AsRef<Path>) -> Result<GitUser> {
        Ok(GitUser {
            name: self.run(cwd.as_ref(), ["config", "user.name"])?,
            email: self.run(cwd.as_ref(), ["config", "user.email"])?,
        })
    }

    pub fn set_user(&self, cwd: impl AsRef<Path>, name: &str, email: &str) -> Result<()> {
        let cwd = cwd.as_ref();
        self.run(cwd, ["config", "user.name", name])?;
        self.run(cwd, ["config", "user.email", email])?;
        Ok(())
    }

    pub fn status_porcelain(&self, cwd: impl AsRef<Path>) -> Result<Vec<String>> {
        self.run(cwd.as_ref(), ["status", "--short"])
            .map(split_lines)
    }

    pub fn has_working_tree_changes(&self, cwd: impl AsRef<Path>) -> Result<bool> {
        Ok(!self.status_porcelain(cwd)?.is_empty())
    }

    pub fn workspace_changed_files(&self, cwd: impl AsRef<Path>) -> Result<Vec<String>> {
        // `git diff --name-only` 会忽略 untracked 文件，这里用 porcelain 统一覆盖。
        let output = self.status_porcelain(cwd)?;
        Ok(output
            .into_iter()
            .filter_map(|line| {
                // Porcelain v1:
                // XY <path>
                // ?? <path>
                if line.len() < 4 {
                    return None;
                }

                let rest = line[3..].trim();
                if rest.is_empty() {
                    return None;
                }

                // rename 形如 `old -> new`，这里统一返回新路径。
                if let Some((_old, new)) = rest.split_once("->") {
                    return Some(new.trim().to_string());
                }
                Some(rest.to_string())
            })
            .collect())
    }

    pub fn workspace_is_clean(&self, cwd: impl AsRef<Path>) -> Result<bool> {
        Ok(self.status_porcelain(cwd)?.is_empty())
    }

    pub fn stage_files(&self, cwd: impl AsRef<Path>) -> Result<Vec<String>> {
        self.run(cwd.as_ref(), ["diff", "--name-only", "--cached"])
            .map(split_lines)
    }

    pub fn stage_reset(&self, cwd: impl AsRef<Path>, file: &str) -> Result<()> {
        self.run(cwd.as_ref(), ["reset", file])?;
        Ok(())
    }

    pub fn stage_diff(&self, cwd: impl AsRef<Path>) -> Result<String> {
        self.run(cwd.as_ref(), ["diff", "--cached"])
            .map(|text| text.trim().to_string())
    }

    pub fn add(&self, cwd: impl AsRef<Path>, files: &[String]) -> Result<()> {
        let mut args = vec!["add".to_string()];
        args.extend(files.iter().cloned());
        self.run_owned(cwd.as_ref(), args)?;
        Ok(())
    }

    pub fn add_all(&self, cwd: impl AsRef<Path>) -> Result<()> {
        self.run(cwd.as_ref(), ["add", "."])?;
        Ok(())
    }

    pub fn commit(&self, cwd: impl AsRef<Path>, message: &str) -> Result<()> {
        self.run_owned(cwd.as_ref(), self.plan_commit_message(message))?;
        Ok(())
    }

    pub fn commit_amend(
        &self,
        cwd: impl AsRef<Path>,
        message: Option<&str>,
        no_edit: bool,
    ) -> Result<()> {
        self.run_owned(cwd.as_ref(), self.plan_commit_amend(message, no_edit))?;
        Ok(())
    }
}
