//! 跨命令共享的 workflow 入口与公共数据模型导出。
mod create;
mod git;
mod models;
mod release;
mod sync;
mod workflow_hooks;

#[cfg(test)]
mod tests;

pub use create::add_workflow::generator_catalog;
pub use create::templates::declared_template_names;
pub use git::{is_destructive, is_read_only};

pub use models::{
    AddWorkflow, AddWorkflowInput, CreateWorkflow, CreateWorkflowInput, GitWorkflow,
    GitWorkflowInput, GitWorkflowMode, ReleaseMode, ReleaseStage, ReleaseStageSnapshot,
    ReleaseStageStatus, ReleaseStateSnapshot, ReleaseWorkflow, ReleaseWorkflowInput, SyncMode,
    SyncWorkflow, SyncWorkflowInput, TemplateCapability, WorkflowBridgeStep, WorkflowExecution,
    WorkflowServices, WorkflowState,
};
