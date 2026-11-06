//! git 工作流：把 git 读写操作收敛到同一套 plan → apply 模型。
//!
//! 设计要点：
//! - 只读子命令（`status` / `log` / `diff` / `branch list` / `tag list`）不产生命令计划，
//!   直接把结构化结果放进 `notes`，并透传给上层渲染。
//! - 写操作一律先拼 `git` 参数，再决定是否执行；`--dry-run` 只输出计划。
//! - 破坏性操作（删除分支/标签）由命令层要求 `--yes`，工作流本身不重复校验。
//! - `sync` / `commit` / `push` 复用 `sync` 工作流的内核，避免两条命令行为漂移。
use anyhow::{anyhow, Result};
use lania_git::GitCommitLogOptions;
use serde_json::json;

use crate::models::{
    GitWorkflow, GitWorkflowInput, GitWorkflowMode, SyncMode, SyncWorkflowInput, WorkflowExecution,
    WorkflowServices, WorkflowState,
};
use crate::sync::{require_git_repo, run_sync_plan, sync_mode_label};
use crate::workflow_hooks::{call_shell_command_after, call_shell_command_before};

impl GitWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: GitWorkflowInput,
    ) -> Result<WorkflowExecution> {
        let zh = services.locale == "zh";
        services
            .tasks
            .start("git", if zh { "Git 工作流" } else { "Git workflow" });

        match input.mode {
            GitWorkflowMode::Sync | GitWorkflowMode::Commit | GitWorkflowMode::Push => {
                let mode = input.mode;
                let sync_input = input.into_sync_input();
                let mut execution = run_sync_plan(services, sync_input).await?;
                execution.workflow = "git".into();
                execution
                    .prompts
                    .insert("mode".into(), json!(sync_mode_label(mode.sync_mode())));
                Ok(execution)
            }
            mode if is_read_only(mode) => inspect(services, input).await,
            _ => {
                let commands = planned_write_commands(&input)?;
                run_write_plan(services, input, commands).await
            }
        }
    }
}

fn planned_write_commands(input: &GitWorkflowInput) -> Result<Vec<Vec<String>>> {
    Ok(match input.mode {
        GitWorkflowMode::Add => vec![vec!["add".to_string(), "-A".to_string()]],
        GitWorkflowMode::Restore => vec![planned_restore(input)],
        GitWorkflowMode::BranchCreate => vec![vec![
            "branch".to_string(),
            required_target(input, "branch")?.to_string(),
        ]],
        GitWorkflowMode::BranchSwitch => vec![vec![
            "switch".to_string(),
            required_target(input, "branch")?.to_string(),
        ]],
        GitWorkflowMode::BranchDelete => {
            let mut args = vec![
                "branch".to_string(),
                if input.force { "-D" } else { "-d" }.to_string(),
            ];
            args.push(required_target(input, "branch")?.to_string());
            vec![args]
        }
        GitWorkflowMode::TagCreate => {
            let mut args = vec![
                "tag".to_string(),
                required_target(input, "tag")?.to_string(),
            ];
            if let Some(message) = &input.message {
                args.push("-m".into());
                args.push(message.clone());
            }
            vec![args]
        }
        GitWorkflowMode::TagDelete => vec![vec![
            "tag".to_string(),
            "-d".to_string(),
            required_target(input, "tag")?.to_string(),
        ]],
        other => return Err(anyhow!("git mode `{}` is not a write command", other.label())),
    })
}

/// 只读子命令：读取结构化结果，不产生命令计划。
async fn inspect(
    services: &WorkflowServices,
    input: GitWorkflowInput,
) -> Result<WorkflowExecution> {
    require_git_repo(&services.git, &input.cwd)?;
    let mut notes = Vec::new();
    let mut prompts = std::collections::BTreeMap::new();
    prompts.insert("mode".into(), json!(input.mode.label()));

    match input.mode {
        GitWorkflowMode::Status => {
            let status_lines = services.git.status_porcelain(&input.cwd)?;
            let upstream = services.git.upstream(&input.cwd)?;
            let current = services.git.current_branch(&input.cwd)?;
            prompts.insert("branch".into(), json!(current));
            prompts.insert(
                "upstream".into(),
                json!(upstream.map(|item| format!("{}/{}", item.remote, item.branch))),
            );
            prompts.insert("changedFiles".into(), json!(status_lines.len()));
            prompts.insert("clean".into(), json!(status_lines.is_empty()));
            for line in &status_lines {
                notes.push(format!("status: {line}"));
            }
            if status_lines.is_empty() {
                notes.push("working tree clean".into());
            }
        }
        GitWorkflowMode::Log => {
            let limit = input.limit.unwrap_or(10);
            let entries = services.git.commit_log(
                &input.cwd,
                GitCommitLogOptions {
                    limit: Some(limit),
                    oneline: true,
                    ..GitCommitLogOptions::default()
                },
            )?;
            prompts.insert("limit".into(), json!(limit));
            prompts.insert("commitCount".into(), json!(entries.len()));
            for entry in &entries {
                notes.push(format!("commit: {} {}", entry.hash, entry.message));
            }
            if entries.is_empty() {
                notes.push("no commits".into());
            }
        }
        GitWorkflowMode::Diff => {
            let diff = planned_diff(&input);
            let output = run_git_capture(services, &input.cwd, &diff).await?;
            let empty = output.trim().is_empty();
            prompts.insert("staged".into(), json!(input.staged));
            prompts.insert("hasDiff".into(), json!(!empty));
            if empty {
                notes.push("no diff".into());
            } else {
                for line in output.lines() {
                    notes.push(format!("diff: {line}"));
                }
            }
        }
        GitWorkflowMode::BranchList => {
            let (local, remote) = services.git.list_all_branches(&input.cwd)?;
            let current = services.git.current_branch(&input.cwd)?;
            prompts.insert("current".into(), json!(current));
            prompts.insert("localCount".into(), json!(local.len()));
            for name in &local {
                notes.push(format!("branch: {name}"));
            }
            for name in &remote {
                notes.push(format!("remoteBranch: {name}"));
            }
        }
        GitWorkflowMode::TagList => {
            let tags = services.git.tags(&input.cwd)?;
            prompts.insert("tagCount".into(), json!(tags.len()));
            for tag in tags {
                notes.push(format!("tag: {tag}"));
            }
        }
        _ => unreachable!("inspect only handles read-only modes"),
    }

    Ok(WorkflowExecution {
        workflow: "git".into(),
        state: WorkflowState::Completed,
        target_dir: input.cwd.display().to_string(),
        prompts,
        bridge_steps: Vec::new(),
        written_files: Vec::new(),
        conflicts: Vec::new(),
        command_plans: Vec::new(),
        git_status: services.git.status(&input.cwd).ok(),
        notes,
        interactive_rendered: false,
    })
}

/// 写操作：先输出命令计划，`--dry-run` 时到此为止。
async fn run_write_plan(
    services: &WorkflowServices,
    input: GitWorkflowInput,
    commands: Vec<Vec<String>>,
) -> Result<WorkflowExecution> {
    let status = require_git_repo(&services.git, &input.cwd)?;
    let mut command_plans = Vec::new();
    for args in &commands {
        command_plans.push(
            std::iter::once("git".to_string())
                .chain(args.clone())
                .collect::<Vec<_>>(),
        );
        if !input.dry_run {
            let command = services
                .git
                .command(args.clone())
                .in_dir(input.cwd.display().to_string());
            call_shell_command_before(services, "git", &input.cwd, "git", args).await;
            let result = services.exec.run_checked_async(command).await?;
            call_shell_command_after(services, "git", &input.cwd, result.exit_code).await;
        }
    }

    let mut prompts = std::collections::BTreeMap::new();
    prompts.insert("mode".into(), json!(input.mode.label()));
    prompts.insert("dryRun".into(), json!(input.dry_run));

    let mut notes = vec![format!("git mode: {}", input.mode.label())];
    if input.dry_run {
        notes.push("dry-run: commands were planned but not executed".into());
    }

    Ok(WorkflowExecution {
        workflow: "git".into(),
        state: WorkflowState::Completed,
        target_dir: input.cwd.display().to_string(),
        prompts,
        bridge_steps: Vec::new(),
        written_files: Vec::new(),
        conflicts: Vec::new(),
        command_plans,
        git_status: Some(status),
        notes,
        interactive_rendered: false,
    })
}

fn required_target<'a>(input: &'a GitWorkflowInput, name: &str) -> Result<&'a str> {
    match name {
        "branch" => input
            .branch
            .as_deref()
            .ok_or_else(|| anyhow!("git branch name is required")),
        "tag" => input
            .tag
            .as_deref()
            .ok_or_else(|| anyhow!("git tag name is required")),
        _ => Err(anyhow!("unsupported git target `{name}`")),
    }
}

fn planned_restore(input: &GitWorkflowInput) -> Vec<String> {
    let mut args = vec!["restore".to_string()];
    if input.staged {
        args.push("--staged".into());
    }
    args.push("--".into());
    if input.paths.is_empty() {
        args.push(".".into());
    } else {
        args.extend(input.paths.clone());
    }
    args
}

fn planned_diff(input: &GitWorkflowInput) -> Vec<String> {
    let mut args = vec!["diff".to_string(), "--no-color".into()];
    if input.staged {
        args.push("--staged".into());
    } else {
        args.push("-U3".into());
    }
    if let Some(path) = &input.path {
        args.push("--".into());
        args.push(path.clone());
    }
    args
}

/// `diff` 需要读取输出而不是只判断退出码，因此单独走一次捕获执行。
async fn run_git_capture(
    services: &WorkflowServices,
    cwd: &std::path::Path,
    args: &[String],
) -> Result<String> {
    let command = services
        .git
        .command(args.to_vec())
        .in_dir(cwd.display().to_string());
    let result = services.exec.run_checked_async(command).await?;
    Ok(result.stdout)
}

/// 判断某个 mode 是否属于只读，命令层据此决定是否需要 `--yes`。
pub fn is_read_only(mode: GitWorkflowMode) -> bool {
    matches!(
        mode,
        GitWorkflowMode::Status
            | GitWorkflowMode::Log
            | GitWorkflowMode::Diff
            | GitWorkflowMode::BranchList
            | GitWorkflowMode::TagList
    )
}

/// 判断某个 mode 是否属于破坏性操作，命令层据此要求 `--yes`。
pub fn is_destructive(mode: GitWorkflowMode) -> bool {
    matches!(
        mode,
        GitWorkflowMode::BranchDelete | GitWorkflowMode::TagDelete
    )
}

impl GitWorkflowInput {
    /// `GitWorkflowInput` → `SyncWorkflowInput` 的等价映射，供 sync 内核复用。
    pub(crate) fn into_sync_input(self) -> SyncWorkflowInput {
        SyncWorkflowInput {
            cwd: self.cwd,
            remote: self.remote,
            branch: self.branch,
            message: self.message,
            push: self.push,
            amend: self.amend,
            force_with_lease: self.force_with_lease,
            dry_run: self.dry_run,
            interactive: self.interactive,
            mode: self.mode.sync_mode(),
        }
    }
}

impl GitWorkflowMode {
    pub(crate) fn sync_mode(self) -> SyncMode {
        match self {
            GitWorkflowMode::Sync => SyncMode::Sync,
            GitWorkflowMode::Commit => SyncMode::Commit,
            GitWorkflowMode::Push => SyncMode::Push,
            _ => SyncMode::Status,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GitWorkflowMode::Status => "status",
            GitWorkflowMode::Log => "log",
            GitWorkflowMode::Diff => "diff",
            GitWorkflowMode::BranchList => "branch-list",
            GitWorkflowMode::BranchCreate => "branch-create",
            GitWorkflowMode::BranchSwitch => "branch-switch",
            GitWorkflowMode::BranchDelete => "branch-delete",
            GitWorkflowMode::TagList => "tag-list",
            GitWorkflowMode::TagCreate => "tag-create",
            GitWorkflowMode::TagDelete => "tag-delete",
            GitWorkflowMode::Add => "add",
            GitWorkflowMode::Restore => "restore",
            GitWorkflowMode::Sync => "sync",
            GitWorkflowMode::Commit => "commit",
            GitWorkflowMode::Push => "push",
        }
    }
}
