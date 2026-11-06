//! generate API 工作流的辅助逻辑门面。
//!
//! 这一层按职责拆分实现，同时维持历史导出接口：
//! - `init`：初始化脚手架与默认配置
//! - `planning`：准备 plan、摘要和说明
//! - `compile` / `render`：entry 编译与文件内容渲染
//! - `manifest`：manifest 读写和 managed paths 维护
//! - `apply`：写盘、冲突处理与 stale 文件清理

mod apply;
mod compile;
mod init;
mod manifest;
mod planning;
mod render;

#[allow(unused_imports)]
pub(crate) use apply::{apply_contract_generation, remove_stale_generated_files};
#[allow(unused_imports)]
pub(crate) use compile::{compile_contract_entry, matches_generate_filters};
#[allow(unused_imports)]
pub(crate) use init::{
    default_contract_config, default_contract_proto, initialize_generate_api,
    resolve_contract_config_path,
};
#[allow(unused_imports)]
pub(crate) use manifest::{
    load_contract_manifest, manifest_managed_paths, resolve_manifest_path,
    update_contract_manifest, write_contract_manifest,
};
#[allow(unused_imports)]
pub(crate) use planning::{
    append_contract_plan_notes, has_generate_filters, prepare_generate_plan,
    summarize_contract_plan, validate_contract_config,
};
#[allow(unused_imports)]
pub(crate) use render::{render_contract_entry, render_generate_command_plan, render_module_file};
