//! sync 工作流：同步仓库状态并串联常见维护步骤。
//!
//! 主要导出：run。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use std::{collections::BTreeMap, path::Path};

use anyhow::{anyhow, Result};
use lania_git::{GitService, GitStatus, GitUpstream};
use lania_node_bridge::CommitBridgeCapability;
use lania_prompt::{PromptFlow, PromptService, PromptStep};
use serde_json::{json, Value};

use crate::models::{
    redact_prompt_answers, step, SyncMode, SyncWorkflow, SyncWorkflowInput, WorkflowBridgeStep,
    WorkflowExecution, WorkflowServices, WorkflowState,
};
use crate::workflow_hooks::{call_shell_command_after, call_shell_command_before};

impl SyncWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: SyncWorkflowInput,
    ) -> Result<WorkflowExecution> {
        // sync 工作流的整体结构也遵循“先 plan，后 apply”：
        // 1. 读 Git 当前状态
        // 2. 计算这次应该执行哪些 git 命令
        // 3. 如果不是 dry-run，再逐条执行
        let zh = services.locale == "zh";
        services.tasks.start(
            "sync",
            if zh {
                "同步工作流"
            } else {
                "Sync workflow"
            },
        );
        services.progress.begin("sync", Some(4));
        let status = services.git.status(&input.cwd)?;
        if !status.ready {
            return Err(anyhow!(
                "git repository not ready in {}",
                input.cwd.display()
            ));
        }
        services.progress.advance("sync", 1);
        let plan = build_sync_plan(services, &status, &input).await?;
        services.progress.advance("sync", 1);

        let mut command_plans = Vec::new();
        for args in &plan.commands {
            command_plans.push(
                std::iter::once("git".to_string())
                    .chain(args.clone().into_iter())
                    .collect(),
            );
            if !input.dry_run {
                let command = services
                    .git
                    .command(args.clone())
                    .in_dir(input.cwd.display().to_string());
                call_shell_command_before(services, "sync", &input.cwd, "git", args).await;
                let result = services.exec.run_checked_async(command).await?;
                call_shell_command_after(services, "sync", &input.cwd, result.exit_code).await;
            }
        }
        services.progress.advance("sync", 1);
        services.progress.finish("sync");

        Ok(WorkflowExecution {
            workflow: "sync".into(),
            state: WorkflowState::Completed,
            target_dir: input.cwd.display().to_string(),
            prompts: redact_prompt_answers(&plan.prompts, &services.prompt.secret_fields()),
            bridge_steps: plan.bridge_steps,
            written_files: Vec::new(),
            conflicts: Vec::new(),
            command_plans,
            git_status: Some(status),
            notes: plan.notes,
            interactive_rendered: false,
        })
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

#[derive(Debug, Clone)]
struct SyncPlan {
    prompts: BTreeMap<String, Value>,
    bridge_steps: Vec<WorkflowBridgeStep>,
    commands: Vec<Vec<String>>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct SyncCommandPlanning<'a> {
    input: &'a SyncWorkflowInput,
    upstream: Option<&'a GitUpstream>,
    remote: Option<&'a str>,
    branch: Option<&'a str>,
    message: Option<&'a str>,
    has_changes: bool,
    has_unpushed: bool,
    should_push: bool,
}

async fn build_sync_plan(
    services: &WorkflowServices,
    status: &GitStatus,
    input: &SyncWorkflowInput,
) -> Result<SyncPlan> {
    // `build_sync_plan` 做的是“语义决策”：
    // - 要不要 commit
    // - 要不要 push
    // - remote/branch/message 从哪里来
    // - 最终应该拼出哪些 git 命令
    //
    // 真正执行 git 命令的细节留给外层 `run()`。
    let upstream = services.git.upstream(&input.cwd)?;
    let has_changes = services.git.has_working_tree_changes(&input.cwd)?;
    let status_lines = services.git.status_porcelain(&input.cwd)?;
    let has_unpushed = services.git.has_unpushed_commits(&input.cwd)?;
    let should_push = resolve_sync_push(input.mode, input.push);
    let mut remote = resolve_sync_remote(input, upstream.as_ref(), should_push);
    let mut branch = resolve_sync_branch(input, status, upstream.as_ref(), should_push)?;
    {
        let _progress_guard = services.progress.suspend_terminal_guard();
        complete_sync_targets_via_prompt(
            &services.prompt,
            services.locale.as_str(),
            &services.git,
            &input.cwd,
            should_push,
            &mut remote,
            &mut branch,
        )?;
    }

    validate_sync_targets(
        &services.git,
        &input.cwd,
        remote.as_deref(),
        branch.as_deref(),
        input.mode,
        should_push,
    )?;

    let mut bridge_steps = Vec::new();
    let needs_commit =
        matches!(input.mode, SyncMode::Sync | SyncMode::Commit) && (has_changes || input.amend);
    let message = resolve_sync_message(
        services,
        input,
        needs_commit,
        &mut bridge_steps,
        remote.as_deref(),
        branch.as_deref(),
    )
    .await?;
    let commands = planned_sync_commands(SyncCommandPlanning {
        input,
        upstream: upstream.as_ref(),
        remote: remote.as_deref(),
        branch: branch.as_deref(),
        message: message.as_deref(),
        has_changes,
        has_unpushed,
        should_push,
    })?;

    let mut prompts = BTreeMap::new();
    prompts.insert("mode".into(), json!(sync_mode_label(input.mode)));
    prompts.insert("remote".into(), json!(remote.clone()));
    prompts.insert("branch".into(), json!(branch.clone()));
    prompts.insert("message".into(), json!(message.clone()));
    prompts.insert("push".into(), json!(should_push));
    prompts.insert("amend".into(), json!(input.amend));
    prompts.insert("dryRun".into(), json!(input.dry_run));
    prompts.insert("interactive".into(), json!(input.interactive));
    prompts.insert("forceWithLease".into(), json!(input.force_with_lease));
    prompts.insert("hasChanges".into(), json!(has_changes));
    prompts.insert("hasUnpushedCommits".into(), json!(has_unpushed));

    let mut notes = vec![
        format!("sync mode: {}", sync_mode_label(input.mode)),
        format!(
            "target: {}/{}",
            remote.clone().unwrap_or_else(|| "-".into()),
            branch.clone().unwrap_or_else(|| "-".into())
        ),
        format!("working tree changes: {}", status_lines.len()),
        format!("unpushed commits: {}", has_unpushed),
    ];
    if let Some(message) = &message {
        notes.push(format!("commit message: {message}"));
    } else {
        notes.push("commit message: not needed".into());
    }
    if input.dry_run {
        notes.push("dry-run: commands were planned but not executed".into());
    }
    if commands.is_empty() {
        notes.push("nothing to sync: no git command needed".into());
    }
    if !status_lines.is_empty() && matches!(input.mode, SyncMode::Status) {
        notes.extend(status_lines.iter().map(|line| format!("status: {line}")));
    }

    Ok(SyncPlan {
        prompts,
        bridge_steps,
        commands,
        notes,
    })
}

fn resolve_sync_push(mode: SyncMode, push: Option<bool>) -> bool {
    // mode 决定默认值，而 CLI 显式 `--push/--no-push` 可以覆盖默认值。
    match mode {
        SyncMode::Status => false,
        SyncMode::Push => true,
        SyncMode::Commit => push.unwrap_or(false),
        SyncMode::Sync => push.unwrap_or(true),
    }
}

fn resolve_sync_remote(
    input: &SyncWorkflowInput,
    upstream: Option<&GitUpstream>,
    should_push: bool,
) -> Option<String> {
    if matches!(input.mode, SyncMode::Status) && !should_push {
        return input
            .remote
            .clone()
            .or_else(|| upstream.map(|candidate| candidate.remote.clone()));
    }
    input
        .remote
        .clone()
        .or_else(|| upstream.map(|candidate| candidate.remote.clone()))
        .or_else(|| should_push.then(|| "origin".into()))
}

fn resolve_sync_branch(
    input: &SyncWorkflowInput,
    status: &GitStatus,
    upstream: Option<&GitUpstream>,
    should_push: bool,
) -> Result<Option<String>> {
    let branch = input
        .branch
        .clone()
        .or_else(|| status.branch.clone())
        .or_else(|| upstream.map(|candidate| candidate.branch.clone()));
    if should_push && branch.is_none() {
        return Err(anyhow!("unable to determine git branch for sync"));
    }
    Ok(branch)
}

async fn resolve_sync_message(
    services: &WorkflowServices,
    input: &SyncWorkflowInput,
    needs_commit: bool,
    bridge_steps: &mut Vec<WorkflowBridgeStep>,
    remote: Option<&str>,
    branch: Option<&str>,
) -> Result<Option<String>> {
    if !needs_commit {
        return Ok(None);
    }
    let message = if let Some(message) = input.message.clone() {
        Some(message)
    } else if input.interactive {
        let default_subject = format!("sync {}", branch.or(remote).unwrap_or("workspace"));
        let mut kind_step = PromptStep::new("sync-kind", "Commit type", "kind")
            .kind(lania_prompt::PromptStepKind::Select)
            .default_value(json!("chore"));
        for kind in [
            "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
            "revert",
        ] {
            kind_step = kind_step.choice(kind, json!(kind));
        }
        let flow = PromptFlow {
            steps: vec![
                kind_step,
                PromptStep::new("sync-scope", "Commit scope", "scope").default_value(json!("sync")),
                PromptStep::new("sync-subject", "Commit subject", "subject")
                    .default_value(json!(default_subject)),
            ],
        };
        let state = {
            let _progress_guard = services.progress.suspend_terminal_guard();
            services.prompt.run_cli_with_options(
                &flow,
                lania_prompt::PromptRunOptions {
                    fallback: Some(lania_prompt::PromptFallbackStrategy::UseDefault),
                    ..lania_prompt::PromptRunOptions::default()
                },
            )?
        };
        let kind = state
            .answers
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("chore")
            .to_string();
        let scope = state
            .answers
            .get("scope")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let subject = state
            .answers
            .get("subject")
            .and_then(Value::as_str)
            .unwrap_or("sync workspace")
            .to_string();

        let cwd = input.cwd.display().to_string();
        let request =
            services
                .bridge
                .commitizen_run_request(cwd.clone(), &kind, scope.clone(), &subject);
        let exchange = services
            .bridge
            .run_commitizen(cwd.clone(), kind, scope.clone(), subject)
            .await?;
        let message = exchange
            .response
            .result
            .as_ref()
            .and_then(|result| result["message"].as_str())
            .ok_or_else(|| anyhow!("commitizen.run returned no message"))?
            .to_string();
        bridge_steps.push(step(request, exchange));
        Some(message)
    } else if let Some(message) = {
        let _progress_guard = services.progress.suspend_terminal_guard();
        prompt_sync_message(&services.prompt, input.amend, remote, branch)?
    } {
        Some(message)
    } else if input.amend {
        None
    } else {
        Some("chore(sync): sync workspace".into())
    };

    if let Some(message) = &message {
        validate_sync_message(services, &input.cwd, message, bridge_steps).await?;
    }
    Ok(message)
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

fn prompt_sync_message(
    prompt: &PromptService,
    amend: bool,
    remote: Option<&str>,
    branch: Option<&str>,
) -> Result<Option<String>> {
    if amend {
        return Ok(None);
    }
    let default_message = format!(
        "chore(sync): sync {}",
        branch.or(remote).unwrap_or("workspace")
    );
    let flow = PromptFlow {
        steps: vec![PromptStep::new("sync-message", "Commit message", "message")
            .default_value(json!(default_message))],
    };
    let state = prompt.run_cli_with_options(
        &flow,
        lania_prompt::PromptRunOptions {
            fallback: Some(lania_prompt::PromptFallbackStrategy::UseDefault),
            ..lania_prompt::PromptRunOptions::default()
        },
    )?;
    Ok(state
        .answers
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned))
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

fn planned_sync_commands(plan: SyncCommandPlanning<'_>) -> Result<Vec<Vec<String>>> {
    let mut commands = Vec::new();
    let needs_commit = matches!(plan.input.mode, SyncMode::Sync | SyncMode::Commit)
        && (plan.has_changes || plan.input.amend);

    if needs_commit {
        if plan.has_changes {
            // git 参数生成统一走 GitService 的 plan helper，避免这里手写命令参数细节。
            let git = GitService::default();
            commands.push(git.plan_add_all());
        }
        commands.push(planned_sync_commit_command(plan.input.amend, plan.message)?);
    }

    if plan.should_push
        && matches!(
            plan.input.mode,
            SyncMode::Sync | SyncMode::Commit | SyncMode::Push
        )
        && (matches!(plan.input.mode, SyncMode::Push) || needs_commit || plan.has_unpushed)
    {
        commands.push(planned_sync_push_command(
            plan.remote
                .ok_or_else(|| anyhow!("unable to determine git remote for push"))?,
            plan.branch
                .ok_or_else(|| anyhow!("unable to determine git branch for push"))?,
            plan.input.force_with_lease,
            plan.upstream.is_none(),
        ));
    }

    Ok(commands)
}

fn planned_sync_commit_command(amend: bool, message: Option<&str>) -> Result<Vec<String>> {
    let git = GitService::default();
    if amend {
        // 保持旧行为：如果传了 message 就 amend with message，否则走 `--no-edit`。
        return Ok(git.plan_commit_amend(message, true));
    }
    let message = message.ok_or_else(|| anyhow!("sync commit requires a commit message"))?;
    Ok(git.plan_commit_message(message))
}

fn planned_sync_push_command(
    remote: &str,
    branch: &str,
    force_with_lease: bool,
    set_upstream: bool,
) -> Vec<String> {
    // 基础 push 参数统一来自 GitService，保持不同调用点行为一致。
    let git = GitService::default();
    let mut command = git.plan_push(remote, branch);
    // 把 flag 插到 `push` 后面，确保它们仍然被 git 识别为选项，而不是 refspec。
    let mut insert_index = 1;
    if force_with_lease {
        command.insert(insert_index, "--force-with-lease".into());
        insert_index += 1;
    }
    if set_upstream {
        // `-u` 必须出现在 remote/branch 之前，因此也要插在 `push` 之后。
        command.insert(insert_index, "-u".into());
    }
    command
}

fn sync_mode_label(mode: SyncMode) -> &'static str {
    match mode {
        SyncMode::Sync => "sync",
        SyncMode::Status => "status",
        SyncMode::Commit => "commit",
        SyncMode::Push => "push",
    }
}
