//! 执行层接缝：把 `GitService` 的参数计划交给 `ExecService` 执行。

use std::path::Path;

use anyhow::{Context, Result};
use lania_exec::ExecCommand;

use crate::utils::classify_error_code;
use crate::{map_exec_error, GitError};

use super::GitService;

impl GitService {
    pub fn command<I, S>(&self, args: I) -> ExecCommand
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        // 返回 `ExecCommand` 而不是立即执行，方便上层继续补充目录、环境变量等信息。
        ExecCommand::new(self.binary.clone()).with_args(args)
    }

    pub(crate) fn run<I, S>(&self, cwd: &Path, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.run_owned(
            cwd,
            args.into_iter()
                .map(|item| item.as_ref().to_string())
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) fn run_owned(&self, cwd: &Path, args: Vec<String>) -> Result<String> {
        // 这里是 git service 和 exec service 的真正接缝：
        // - `GitService` 决定要执行哪个 git 子命令
        // - `ExecService` 负责真实的子进程生命周期与 I/O 收集
        let command = self.command(args.clone()).in_dir(cwd.display().to_string());
        let result = self
            .exec
            .run(command)
            .with_context(|| format!("failed to run git {} in {}", args.join(" "), cwd.display()))
            .map_err(|error| map_exec_error(error, args.clone()))?;

        if result.exit_code == 0 {
            // 某些 git 子命令把有用结果写到 stderr，因此成功时优先返回非空 stdout，
            // 否则回退到 stderr，减少上层对命令细节的感知。
            let stdout = result.stdout.trim().to_string();
            if stdout.is_empty() {
                return Ok(result.stderr.trim().to_string());
            }
            return Ok(stdout);
        }

        let stderr = result.stderr.trim().to_string();
        let stdout = result.stdout.trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        let code = classify_error_code(&detail);

        Err(GitError {
            code,
            message: if detail.is_empty() {
                format!(
                    "git {} failed with exit code {}",
                    args.join(" "),
                    result.exit_code
                )
            } else {
                detail
            },
            command: args,
        }
        .into())
    }
}
