//! Git 相关的数据结构定义（纯模型层）。
//!
//! 这个文件不负责执行 git 命令，它只定义“git 状态/结果长什么样”：
//! - `GitStatus`：是否在仓库内、当前分支
//! - `GitRemote` / `GitUpstream` / `GitUser`：上游信息与用户信息
//! - `GitCommitLogEntry` / options：供上层 workflow 构建查询
//! - `GitErrorCode` / `GitError`：把执行失败分类成可处理的语义错误
//!
//! 执行层请看 `service.rs`，错误映射工具请看 `utils.rs`。

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitStatus {
    pub ready: bool,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitUser {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitUpstream {
    pub remote: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitCommitLogEntry {
    pub hash: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitCommitLogOptions {
    pub limit: Option<usize>,
    pub author: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub range: Option<(String, String)>,
    pub oneline: bool,
    pub format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitMergeOptions {
    pub flags: Vec<String>,
    pub strategy: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitRebaseOptions {
    pub interactive: bool,
    pub onto: Option<String>,
    pub root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitRevertOptions {
    pub no_commit: bool,
    pub mainline: Option<u32>,
    pub no_edit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitErrorCode {
    NotRepository,
    MissingUpstream,
    MergeConflict,
    BinaryMissing,
    PermissionDenied,
    CommandFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitError {
    pub code: GitErrorCode,
    pub message: String,
    pub command: Vec<String>,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code_as_str(), self.message)
    }
}

impl std::error::Error for GitError {}

impl GitError {
    fn code_as_str(&self) -> &'static str {
        match self.code {
            GitErrorCode::NotRepository => "not_repository",
            GitErrorCode::MissingUpstream => "missing_upstream",
            GitErrorCode::MergeConflict => "merge_conflict",
            GitErrorCode::BinaryMissing => "binary_missing",
            GitErrorCode::PermissionDenied => "permission_denied",
            GitErrorCode::CommandFailed => "command_failed",
        }
    }
}
