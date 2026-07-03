use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::Value;

use crate::models::{
    ReleaseWorkflow, ReleaseWorkflowInput, WorkflowExecution, WorkflowServices, WorkflowState,
};

impl ReleaseWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: ReleaseWorkflowInput,
    ) -> Result<WorkflowExecution> {
        let logger = services.logger.scoped("workflow.release");
        logger.log(lania_logger::LogLevel::Info, "Starting release workflow");

        let _ = input;

        Ok(WorkflowExecution {
            workflow: "release".into(),
            state: WorkflowState::Completed,
            target_dir: String::new(),
            prompts: BTreeMap::<String, Value>::new(),
            bridge_steps: Vec::new(),
            written_files: Vec::new(),
            conflicts: Vec::new(),
            command_plans: Vec::new(),
            git_status: None,
            notes: Vec::new(),
            interactive_rendered: false,
        })
    }
}
