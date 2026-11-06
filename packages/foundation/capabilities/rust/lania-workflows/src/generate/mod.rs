//! generate 工作流入口，按 API 和模块生成拆分子流程。
//!
//! 这一层几乎不放业务逻辑，而是只负责模块拆分。
//! 可以把它理解成“generate 领域的目录导航”：
//! - `api` / `module`：两个主工作流入口
//! - `schema*` / `types`：共享的数据结构和配置解析
//! - `module_*` / `api_support`：具体的准备、渲染、manifest 维护细节
pub(crate) mod api;
pub(crate) mod api_support;
pub(crate) mod module;
pub(crate) mod module_inject;
pub(crate) mod module_manifest;
pub(crate) mod module_prepare;
pub(crate) mod module_render;
pub(crate) mod schema;
pub(crate) mod schema_utils;
pub(crate) mod types;
