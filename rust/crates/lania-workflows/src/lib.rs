//! 跨命令共享的 workflow 入口与公共数据模型导出。
mod create;
mod generate;
mod models;
mod release;
mod sync;
mod workflow_hooks;

#[cfg(test)]
mod tests;

pub(crate) use generate::api_support as generate_api_support;
pub(crate) use generate::module_inject as generate_module_inject;
pub(crate) use generate::module_manifest as generate_module_manifest;
pub(crate) use generate::module_prepare as generate_module_prepare;
pub(crate) use generate::module_render as generate_module_render;
pub(crate) use generate::schema as generate_schema;
pub(crate) use generate::types as generate_types;

pub use models::{
    AddWorkflow, AddWorkflowInput, CreateWorkflow, CreateWorkflowInput, GenerateApiMode,
    GenerateApiWorkflow, GenerateApiWorkflowInput, GenerateModuleMode, GenerateModuleWorkflow,
    GenerateModuleWorkflowInput, ReleaseMode, ReleaseStage, ReleaseStageSnapshot,
    ReleaseStageStatus, ReleaseStateSnapshot, ReleaseWorkflow, ReleaseWorkflowInput, SyncMode,
    SyncWorkflow, SyncWorkflowInput, TemplateCapability, WorkflowBridgeStep, WorkflowExecution,
    WorkflowServices, WorkflowState,
};
