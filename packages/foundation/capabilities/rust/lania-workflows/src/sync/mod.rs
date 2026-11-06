//! sync 工作流：同步仓库状态并串联常见维护步骤。
//!
//! 主要导出：run。
//! 关键点：
//! - `sync` 的 plan/apply 内核在 `plan.rs`，`GitWorkflow` 也复用同一份实现
//! - 这里只保留 `SyncWorkflow` 入口与命令/Lint 相关的辅助函数
use std::path::Path;

use anyhow::{anyhow, Result};
use lania_git::GitService;
use lania_node_bridge::CommitBridgeCapability;
use lania_prompt::{PromptFlow, PromptService, PromptStep};
use serde_json::{json, Value};

mod plan;

use crate::models::{
    step, SyncMode, SyncWorkflow, SyncWorkflowInput, WorkflowBridgeStep, WorkflowExecution,
    WorkflowServices,
};

pub(crate) use plan::{require_git_repo, run_sync_plan, sync_mode_label};

impl SyncWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: SyncWorkflowInput,
    ) -> Result<WorkflowExecution> {
        run_sync_plan(services, input).await
    }
}

fn complete_sync_targets_via_prompt(
    prompt: &PromptService,
    locale: &str,
    git: &GitService,
    cwd: &Path,
    should_push: bool,
    remote: &mut Option<String>,
    branch: &mut Option<String>,
) -> Result<()> {
    // push 相关目标（remote/branch）允许在缺省时通过 prompt 补齐，
    // 这样 sync 命令既能脚本化执行，也能在交互场景下更友好地使用。
    if !should_push {
        return Ok(());
    }
    let zh = locale == "zh";
    let remotes = git.remotes(cwd)?;
    let local_branches = git.list_local_branches(cwd)?;

    let mut flow = PromptFlow::new();
    if remote.is_none() {
        let mut step = PromptStep::new(
            "sync-remote",
            if zh { "Git 远程仓库" } else { "Git remote" },
            "remote",
        )
        .kind(lania_prompt::PromptStepKind::Select)
        .default_value(json!("origin"));
        for remote_item in &remotes {
            step = step.choice(&remote_item.name, json!(remote_item.name));
        }
        flow = flow.step(step);
    }
    if branch.is_none() {
        let default_branch = local_branches
            .first()
            .cloned()
            .unwrap_or_else(|| "main".into());
        let mut step = PromptStep::new(
            "sync-branch",
            if zh { "Git 分支" } else { "Git branch" },
            "branch",
        )
        .kind(lania_prompt::PromptStepKind::Select)
        .default_value(json!(default_branch));
        for branch_name in &local_branches {
            step = step.choice(branch_name, json!(branch_name));
        }
        flow = flow.step(step);
    }
    if flow.steps.is_empty() {
        return Ok(());
    }
    let state = prompt.run_cli_with_options(
        &flow,
        lania_prompt::PromptRunOptions {
            fallback: Some(lania_prompt::PromptFallbackStrategy::UseDefault),
            ..lania_prompt::PromptRunOptions::default()
        },
    )?;
    if remote.is_none() {
        *remote = state
            .answers
            .get("remote")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    if branch.is_none() {
        *branch = state
            .answers
            .get("branch")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    Ok(())
}

async fn validate_sync_message(
    services: &WorkflowServices,
    cwd: &Path,
    message: &str,
    bridge_steps: &mut Vec<WorkflowBridgeStep>,
) -> Result<()> {
    let cwd = cwd.display().to_string();
    let lint_request = services
        .bridge
        .commitlint_run_request(cwd.clone(), message.to_string());
    let lint_exchange = services
        .bridge
        .run_commitlint(cwd, message.to_string())
        .await?;
    let valid = lint_exchange
        .response
        .result
        .as_ref()
        .and_then(|result| result["valid"].as_bool())
        .unwrap_or(false);
    bridge_steps.push(step(lint_request, lint_exchange));
    if !valid {
        return Err(anyhow!("commitlint rejected commit message: {}", message));
    }
    Ok(())
}

fn validate_sync_targets(
    git: &GitService,
    cwd: &Path,
    remote: Option<&str>,
    branch: Option<&str>,
    mode: SyncMode,
    should_push: bool,
) -> Result<()> {
    if matches!(mode, SyncMode::Status) {
        return Ok(());
    }
    if should_push {
        let remote = remote.ok_or_else(|| anyhow!("unable to determine git remote for sync"))?;
        let branch = branch.ok_or_else(|| anyhow!("unable to determine git branch for sync"))?;
        if !git.remote_exists(cwd, remote)? {
            return Err(anyhow!("git remote `{remote}` does not exist"));
        }
        if !git.branch_exists_local(cwd, branch)? && !git.branch_exists_remote(cwd, branch)? {
            return Err(anyhow!(
                "git branch `{branch}` does not exist locally or remotely"
            ));
        }
    }
    Ok(())
}
