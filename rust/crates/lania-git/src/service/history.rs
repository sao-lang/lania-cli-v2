//! 提交历史查询与解析。

use std::path::Path;

use anyhow::Result;

use crate::{split_lines, GitCommitLogEntry, GitCommitLogOptions};

use super::GitService;

impl GitService {
    pub fn last_commit_message(&self, cwd: impl AsRef<Path>) -> Result<String> {
        self.run(cwd.as_ref(), ["log", "-1", "--pretty=%B"])
    }

    pub fn last_commit_hash(&self, cwd: impl AsRef<Path>) -> Result<String> {
        self.run(cwd.as_ref(), ["log", "-1", "--pretty=%H"])
    }

    pub fn commit_files(&self, cwd: impl AsRef<Path>, commit: &str) -> Result<Vec<String>> {
        // 只返回文件路径，避免把提交头或正文混进结果里。
        let output = self.run(
            cwd.as_ref(),
            ["show", "--name-only", "--pretty=format:", commit],
        )?;
        Ok(split_lines(output))
    }

    pub fn commit_log(
        &self,
        cwd: impl AsRef<Path>,
        options: GitCommitLogOptions,
    ) -> Result<Vec<GitCommitLogEntry>> {
        // 这个 API 不尝试穷举 `git log` 的全部能力，只覆盖 CLI 最常见的过滤参数。
        let mut args = vec!["log".to_string()];
        if let Some(limit) = options.limit {
            args.push("-n".into());
            args.push(limit.to_string());
        }
        if let Some((from, to)) = options.range {
            args.push(format!("{from}..{to}"));
        }
        if let Some(author) = options.author {
            args.push("--author".into());
            args.push(author);
        }
        if let Some(since) = options.since {
            args.push(format!("--since={since}"));
        }
        if let Some(until) = options.until {
            args.push(format!("--until={until}"));
        }
        if options.oneline {
            args.push("--oneline".into());
        }
        if let Some(format) = options.format {
            args.push(format!("--pretty={format}"));
        }

        let output = self.run_owned(cwd.as_ref(), args)?;
        Ok(split_lines(output)
            .into_iter()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }

                let (hash, message) = trimmed
                    .split_once(' ')
                    .map(|(hash, message)| (hash.to_string(), message.to_string()))
                    .unwrap_or_else(|| (trimmed.to_string(), String::new()));
                Some(GitCommitLogEntry { hash, message })
            })
            .collect())
    }
}
