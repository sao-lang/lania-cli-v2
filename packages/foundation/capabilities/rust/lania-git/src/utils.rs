//! Git 错误分类与输出后处理工具函数。
//!
//! 这里主要做两件事：
//! - 根据 stderr 文本把失败分类成更语义化的 `GitErrorCode`
//! - 把 `ExecError` 映射成 `GitError`，让上层 workflow 能按“缺少 upstream/不是仓库/冲突”等分支处理

use lania_exec::{ExecError, ExecErrorCode};

use crate::{GitError, GitErrorCode};

pub(crate) fn classify_error_code(stderr: &str) -> GitErrorCode {
    let stderr_lower = stderr.to_ascii_lowercase();
    if stderr_lower.contains("not a git repository") {
        GitErrorCode::NotRepository
    } else if stderr_lower.contains("no upstream configured")
        || stderr_lower.contains("no upstream branch")
        || stderr_lower.contains("no upstream configured for branch")
    {
        GitErrorCode::MissingUpstream
    } else if stderr_lower.contains("merge conflict") || stderr_lower.contains("conflict") {
        GitErrorCode::MergeConflict
    } else if stderr_lower.contains("permission denied") {
        GitErrorCode::PermissionDenied
    } else if stderr_lower.contains("not found")
        || stderr_lower.contains("no such file or directory")
    {
        GitErrorCode::BinaryMissing
    } else {
        GitErrorCode::CommandFailed
    }
}

pub(crate) fn map_exec_error(error: anyhow::Error, command: Vec<String>) -> anyhow::Error {
    if let Some(exec_error) = error.downcast_ref::<ExecError>() {
        let code = match exec_error.code {
            ExecErrorCode::BinaryMissing => GitErrorCode::BinaryMissing,
            ExecErrorCode::PermissionDenied => GitErrorCode::PermissionDenied,
            ExecErrorCode::CommandFailed => classify_error_code(&exec_error.stderr),
            ExecErrorCode::SpawnFailed | ExecErrorCode::TimedOut | ExecErrorCode::Cancelled => {
                GitErrorCode::CommandFailed
            }
        };
        return GitError {
            code,
            message: exec_error.message.clone(),
            command,
        }
        .into();
    }
    error
}

pub(crate) fn split_lines(output: String) -> Vec<String> {
    output
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
