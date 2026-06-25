//! `create` 工作流的辅助模块入口。
//!
//! 这里按职责把原先的单个 helpers 文件拆成更小的子模块，
//! 主流程仍然通过统一的 re-export 使用这些函数，先保持调用层稳定。

mod env;
mod package;
mod target_path;
mod template_context;

pub(super) use env::{ensure_directory_empty, find_available_port};
pub(super) use package::{
    command_to_vec, resolve_dependency_versions, resolve_package_manager, run_package_command,
};
pub(super) use target_path::{
    normalize_add_target, resolve_add_output_path, validate_relative_target,
};
pub(super) use template_context::load_add_template_context;
