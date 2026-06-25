//! workflow 执行结果、文件写入、命令计划与提示信息等通用模型。
//!
//! 主要导出：WorkflowBridgeStep、WorkflowExecution、WorkflowServices、TemplateCapability、CreateWorkflowInput、AddWorkflowInput。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use lania_config::{
    ReleaseGitConfig, ReleasePostCheckConfig, ReleaseProfile, ReleaseStepConfig,
    ReleaseVerifyConfig, ReleaseVersioningConfig,
};
use lania_exec::ExecService;
use lania_fs::FsService;
use lania_git::{GitService, GitStatus};
use lania_hooks::HookRuntime;
use lania_logger::LoggerService;
use lania_node_bridge::{BridgeExchange, BridgeRequest, NodeBridgeClient};
use lania_pm::{PackageManager, PackageManagerService};
use lania_progress::ProgressService;
use lania_prompt::PromptService;
use lania_task::TaskService;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Planned,
    Prompted,
    Rendered,
    FilesWritten,
    CommandsPlanned,
    GitReady,
    Failed,
    Completed,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowBridgeStep {
    pub method: String,
    pub request: BridgeRequest,
    pub exchange: BridgeExchange,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowExecution {
    pub workflow: String,
    pub state: WorkflowState,
    pub target_dir: String,
    pub prompts: BTreeMap<String, Value>,
    pub bridge_steps: Vec<WorkflowBridgeStep>,
    pub written_files: Vec<String>,
    pub conflicts: Vec<String>,
    pub command_plans: Vec<Vec<String>>,
    pub git_status: Option<GitStatus>,
    pub notes: Vec<String>,
    #[serde(rename = "_interactiveRendered")]
    pub interactive_rendered: bool,
}

#[derive(Clone)]
pub struct WorkflowServices {
    pub logger: LoggerService,
    pub prompt: PromptService,
    pub fs: FsService,
    pub git: GitService,
    pub package_manager: PackageManagerService,
    pub exec: ExecService,
    pub tasks: TaskService,
    pub progress: ProgressService,
    pub bridge: NodeBridgeClient,
    pub hooks: Arc<dyn HookRuntime>,
    pub hook_cwd: String,
    pub hook_trace_id: String,
    pub hook_command_handler_id: String,
    pub locale: String,
}

#[derive(Debug, Clone)]
pub struct TemplateCapability<'a> {
    pub(crate) bridge: &'a NodeBridgeClient,
}

#[derive(Debug, Clone)]
pub struct CreateWorkflowInput {
    pub cwd: PathBuf,
    pub path: Option<String>,
    pub project_name: Option<String>,
    pub template: Option<String>,
    pub package_manager: Option<String>,
    pub language: Option<String>,
    pub init_git: bool,
    pub skip_install: bool,
    pub skip_install_specified: bool,
    pub dry_run: bool,
    pub preview: bool,
}

#[derive(Debug, Clone)]
pub struct AddWorkflowInput {
    pub cwd: PathBuf,
    pub name: Option<String>,
    pub template: Option<String>,
    pub target: Option<String>,
    pub force: bool,
}

#[derive(Debug, Clone)]
pub struct SyncWorkflowInput {
    pub cwd: PathBuf,
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub message: Option<String>,
    pub push: Option<bool>,
    pub amend: bool,
    pub force_with_lease: bool,
    pub dry_run: bool,
    pub interactive: bool,
    pub mode: SyncMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Sync,
    Status,
    Commit,
    Push,
}

#[derive(Debug, Clone)]
pub struct ReleaseWorkflowInput {
    pub cwd: PathBuf,
    pub mode: ReleaseMode,
    pub version: Option<String>,
    pub tag: Option<String>,
    pub profile: Option<String>,
    pub env: Option<String>,
    pub channel: Option<String>,
    pub from_stage: Option<String>,
    pub to_stage: Option<String>,
    pub skip_stages: Vec<String>,
    pub state_file: Option<String>,
    pub apply: bool,
    pub dry_run: bool,
    pub yes: bool,
    pub publish: bool,
    pub changelog: bool,
    pub skip_git: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseMode {
    Plan,
    Run,
    Resume,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStage {
    Preflight,
    Verify,
    Version,
    Changelog,
    Artifact,
    PublishOrDeploy,
    PostCheck,
    Finalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStageStatus {
    Planned,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseStageSnapshot {
    pub stage: ReleaseStage,
    pub status: ReleaseStageStatus,
    pub commands: Vec<Vec<String>>,
    pub notes: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseStateSnapshot {
    pub version: u32,
    pub cwd: String,
    pub profile: ReleaseProfile,
    pub env: Option<String>,
    pub channel: Option<String>,
    pub mode: String,
    pub state_file: String,
    pub active_range: Vec<String>,
    pub updated_at_epoch_ms: u128,
    pub stages: Vec<ReleaseStageSnapshot>,
    pub completed: bool,
    pub summary: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReleasePlan {
    pub(crate) cwd: PathBuf,
    pub(crate) profile: ReleaseProfile,
    pub(crate) env: Option<String>,
    pub(crate) channel: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) publish: bool,
    pub(crate) state_file: PathBuf,
    pub(crate) from_stage: Option<ReleaseStage>,
    pub(crate) to_stage: Option<ReleaseStage>,
    pub(crate) skip_stages: BTreeSet<ReleaseStage>,
    pub(crate) apply: bool,
    pub(crate) dry_run: bool,
    pub(crate) verify: ReleaseVerifyConfig,
    pub(crate) versioning: ReleaseVersioningConfig,
    pub(crate) changelog: ReleaseStepConfig,
    pub(crate) artifact: ReleaseStepConfig,
    pub(crate) deploy: lania_config::ReleaseDeployConfig,
    pub(crate) post_check: ReleasePostCheckConfig,
    pub(crate) git: ReleaseGitConfig,
    pub(crate) package_manager: PackageManager,
}

#[derive(Debug, Clone)]
pub struct GenerateApiWorkflowInput {
    pub cwd: PathBuf,
    pub config_path: Option<String>,
    pub manifest_path: Option<String>,
    pub source_filter: Vec<String>,
    pub target_filter: Vec<String>,
    pub entry_filter: Vec<String>,
    pub dry_run: bool,
    pub check: bool,
    pub clean: bool,
    pub force: bool,
    pub mode: GenerateApiMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateApiMode {
    Apply,
    Plan,
    Diff,
    Init,
}

#[derive(Debug, Clone)]
pub struct GenerateModuleWorkflowInput {
    pub cwd: PathBuf,
    pub config_path: Option<String>,
    pub manifest_path: Option<String>,
    pub input_path: Option<String>,
    pub source_filter: Vec<String>,
    pub target_filter: Vec<String>,
    pub entry_filter: Vec<String>,
    pub framework: Option<String>,
    pub main_path: Option<String>,
    pub module_name: Option<String>,
    pub package_name: Option<String>,
    pub dry_run: bool,
    pub check: bool,
    pub clean: bool,
    pub force: bool,
    pub no_inject: bool,
    pub mode: GenerateModuleMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateModuleMode {
    Apply,
    Plan,
    Diff,
    Init,
}

#[derive(Debug, Clone, Default)]
pub struct CreateWorkflow;

#[derive(Debug, Clone, Default)]
pub struct AddWorkflow;

#[derive(Debug, Clone, Default)]
pub struct SyncWorkflow;

#[derive(Debug, Clone, Default)]
pub struct ReleaseWorkflow;

#[derive(Debug, Clone, Default)]
pub struct GenerateApiWorkflow;

#[derive(Debug, Clone, Default)]
pub struct GenerateModuleWorkflow;

pub(crate) fn step(request: BridgeRequest, exchange: BridgeExchange) -> WorkflowBridgeStep {
    WorkflowBridgeStep {
        method: request.method.clone(),
        request,
        exchange,
    }
}

pub(crate) fn redact_prompt_answers(
    answers: &BTreeMap<String, Value>,
    secret_fields: &[String],
) -> BTreeMap<String, Value> {
    answers
        .iter()
        .map(|(key, value)| {
            if secret_fields.iter().any(|field| field == key) {
                (key.clone(), serde_json::json!("***"))
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect()
}
