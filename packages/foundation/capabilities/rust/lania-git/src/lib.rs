//! Git 工作区探测、状态收集与常用命令计划封装。
//!
//! 主要导出：new、status、init、current_branch、list_local_branches、list_remote_branches，以及常用 git 操作的工具层封装。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
//! - 包含子进程/环境变量交互

mod models;
mod service;
mod utils;

pub use models::*;
pub use service::GitService;

pub(crate) use utils::{map_exec_error, non_empty, split_lines};

#[cfg(test)]
mod tests;
