use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use lania_config::ReleaseProfile;
use serde_json::json;

use crate::models::{
    redact_prompt_answers, ReleaseMode, ReleaseStage, ReleaseStageSnapshot, ReleaseStageStatus,
    ReleaseStateSnapshot, ReleaseWorkflow, ReleaseWorkflowInput, WorkflowExecution,
    WorkflowServices, WorkflowState,
};

impl ReleaseWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: ReleaseWorkflowInput,
    ) -> Result<WorkflowExecution> {
        let zh = services.locale == "zh";
        let logger = services.logger.scoped("workflow.release");
        logger.log(lania_logger::LogLevel::Info, "Starting release workflow");

        services.tasks.start(
            "release",
            if zh {
                "发布工作流"
            } else {
                "Release workflow"
            },
        );
        services.progress.begin("release", Some(6));

        // 1. Determine release profile
        let profile = input.profile.as_deref().unwrap_or("package");
        services.progress.advance("release", 1);

        // 2. Check git status
        let git_status = services.git.status(&input.cwd).ok();
        let git_ready = git_status.as_ref().map(|s| s.ready).unwrap_or(false);
        if !input.skip_git && !git_ready {
            return Err(anyhow!(
                "{}",
                if zh {
                    "当前目录不是有效的 Git 仓库"
                } else {
                    "not a valid git repository"
                }
            ));
        }
        services.progress.advance("release", 1);

        // 3. Build stage plan based on profile
        let stage_list = build_stage_plan(&input);
        let stages: Vec<ReleaseStageSnapshot> = stage_list
            .iter()
            .map(|stage| ReleaseStageSnapshot {
                stage: *stage,
                status: ReleaseStageStatus::Planned,
                commands: build_stage_commands(*stage, &input),
                notes: vec![],
                error: None,
            })
            .collect();
        services.progress.advance("release", 1);

        // 4. Build command plans
        let command_plans: Vec<Vec<String>> =
            stages.iter().flat_map(|s| s.commands.clone()).collect();
        services.progress.advance("release", 1);

        // 5. Collect notes
        let mut notes = vec![format!("release profile: {profile}")];
        if input.publish {
            notes.push("publish enabled".into());
        }
        if input.dry_run {
            notes.push("dry-run mode".into());
        }

        // 6. Build prompts map for E2E assertions
        let mut prompts = BTreeMap::new();
        prompts.insert(
            "mode".into(),
            json!(format!("{:?}", input.mode).to_lowercase()),
        );
        prompts.insert("profile".into(), json!(profile));
        prompts.insert("completed".into(), json!(false));

        // Create state file path
        let state_file = input.state_file.clone().unwrap_or_else(|| {
            input
                .cwd
                .join(".lania")
                .join("release-state.json")
                .display()
                .to_string()
        });
        let state_path = std::path::Path::new(&state_file);

        // Execute or plan based on mode
        let execution_state: WorkflowState;

        match input.mode {
            ReleaseMode::Plan | ReleaseMode::Status => {
                // Plan/Status mode: just return the plan without executing
                execution_state = WorkflowState::Planned;

                // Write state file for plan
                write_state_file(state_path, profile, &stages, &input)?;
                prompts.insert("completed".into(), json!(false));
            }
            ReleaseMode::Run | ReleaseMode::Resume => {
                if input.apply || input.mode == ReleaseMode::Resume {
                    // Execute stages sequentially
                    let mut executed_stages: Vec<ReleaseStageSnapshot> = Vec::new();
                    let mut failed = false;

                    for stage_snapshot in &stages {
                        if failed {
                            executed_stages.push(ReleaseStageSnapshot {
                                stage: stage_snapshot.stage,
                                status: ReleaseStageStatus::Skipped,
                                commands: vec![],
                                notes: vec![],
                                error: None,
                            });
                            continue;
                        }

                        let stage_commands = stage_snapshot.commands.clone();
                        let mut stage_notes = Vec::new();
                        let mut stage_error = None;
                        let mut stage_status = ReleaseStageStatus::Completed;

                        // Execute commands for this stage
                        for cmd in &stage_commands {
                            if input.dry_run {
                                stage_notes.push(format!("[dry-run] would execute: {:?}", cmd));
                                continue;
                            }
                            let result = services
                                .exec
                                .run_checked_async(
                                    services
                                        .git
                                        .command(cmd.clone())
                                        .in_dir(input.cwd.display().to_string()),
                                )
                                .await;
                            match result {
                                Ok(_) => {
                                    stage_notes.push(format!("ok: {:?}", cmd));
                                }
                                Err(e) => {
                                    stage_error = Some(format!("{}", e));
                                    stage_status = ReleaseStageStatus::Failed;
                                    failed = true;
                                    break;
                                }
                            }
                        }

                        executed_stages.push(ReleaseStageSnapshot {
                            stage: stage_snapshot.stage,
                            status: stage_status,
                            commands: stage_commands,
                            notes: stage_notes,
                            error: stage_error,
                        });
                    }

                    execution_state = if failed {
                        WorkflowState::Failed
                    } else {
                        WorkflowState::Completed
                    };
                    prompts.insert("completed".into(), json!(!failed));

                    // Write final state
                    let final_stages: Vec<ReleaseStageSnapshot> = executed_stages;
                    write_state_file(state_path, profile, &final_stages, &input)?;
                } else {
                    // --apply not set, just plan
                    execution_state = WorkflowState::Planned;
                    write_state_file(state_path, profile, &stages, &input)?;
                }
            }
        }

        services.progress.advance("release", 1);
        services.progress.finish("release");

        Ok(WorkflowExecution {
            workflow: "release".into(),
            state: execution_state,
            target_dir: input.cwd.display().to_string(),
            prompts: redact_prompt_answers(&prompts, &services.prompt.secret_fields()),
            bridge_steps: Vec::new(),
            written_files: Vec::new(),
            conflicts: Vec::new(),
            command_plans,
            git_status,
            notes,
            interactive_rendered: false,
        })
    }
}

/// Build the list of stages for a release run.
fn build_stage_plan(input: &ReleaseWorkflowInput) -> Vec<ReleaseStage> {
    let all_stages = vec![
        ReleaseStage::Preflight,
        ReleaseStage::Verify,
        ReleaseStage::Version,
        ReleaseStage::Changelog,
        ReleaseStage::Artifact,
        ReleaseStage::PublishOrDeploy,
        ReleaseStage::PostCheck,
        ReleaseStage::Finalize,
    ];

    let skip_set: BTreeSet<ReleaseStage> = input
        .skip_stages
        .iter()
        .filter_map(|s| match s.to_lowercase().as_str() {
            "preflight" => Some(ReleaseStage::Preflight),
            "verify" => Some(ReleaseStage::Verify),
            "version" => Some(ReleaseStage::Version),
            "changelog" => Some(ReleaseStage::Changelog),
            "artifact" => Some(ReleaseStage::Artifact),
            "publish" | "publishordeploy" => Some(ReleaseStage::PublishOrDeploy),
            "postcheck" | "post-check" => Some(ReleaseStage::PostCheck),
            "finalize" => Some(ReleaseStage::Finalize),
            _ => None,
        })
        .collect();

    let start_idx = input
        .from_stage
        .as_ref()
        .and_then(|s| stage_index(s))
        .unwrap_or(0);
    let end_idx = input
        .to_stage
        .as_ref()
        .and_then(|s| stage_index(s))
        .unwrap_or(all_stages.len() - 1);

    all_stages
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i >= start_idx && *i <= end_idx)
        .map(|(_, s)| s)
        .filter(|s| !skip_set.contains(s))
        .collect()
}

fn stage_index(name: &str) -> Option<usize> {
    match name.to_lowercase().as_str() {
        "preflight" => Some(0),
        "verify" => Some(1),
        "version" => Some(2),
        "changelog" => Some(3),
        "artifact" => Some(4),
        "publish" | "publishordeploy" | "deploy" => Some(5),
        "postcheck" | "post-check" => Some(6),
        "finalize" => Some(7),
        _ => None,
    }
}

/// Build the shell commands to execute for a given stage.
fn build_stage_commands(stage: ReleaseStage, input: &ReleaseWorkflowInput) -> Vec<Vec<String>> {
    match stage {
        ReleaseStage::Preflight => {
            let mut cmds = Vec::new();
            cmds.push(vec!["git".into(), "status".into(), "--porcelain".into()]);
            if input.publish {
                cmds.push(vec!["npm".into(), "whoami".into()]);
            }
            cmds
        }
        ReleaseStage::Verify => {
            vec![vec![
                "git".into(),
                "log".into(),
                "--oneline".into(),
                "-5".into(),
            ]]
        }
        ReleaseStage::Version => {
            if let Some(version) = &input.version {
                vec![vec![
                    "npm".into(),
                    "version".into(),
                    version.clone(),
                    "--no-git-tag-version".into(),
                ]]
            } else {
                vec![]
            }
        }
        ReleaseStage::Changelog => {
            if input.changelog {
                vec![vec![
                    "npx".into(),
                    "conventional-changelog".into(),
                    "-p".into(),
                    "angular".into(),
                    "-i".into(),
                    "CHANGELOG.md".into(),
                    "-s".into(),
                ]]
            } else {
                vec![]
            }
        }
        ReleaseStage::Artifact => {
            vec![vec!["npm".into(), "run".into(), "build".into()]]
        }
        ReleaseStage::PublishOrDeploy => {
            let mut cmds: Vec<Vec<String>> = Vec::new();
            if input.publish {
                let mut publish = vec!["npm".into(), "publish".into()];
                if let Some(tag) = &input.tag {
                    publish.push("--tag".into());
                    publish.push(tag.clone());
                }
                cmds.push(publish);
            }
            cmds
        }
        ReleaseStage::PostCheck => {
            vec![]
        }
        ReleaseStage::Finalize => {
            let mut cmds: Vec<Vec<String>> = Vec::new();
            if !input.skip_git {
                cmds.push(vec!["git".into(), "add".into(), "-A".into()]);
                cmds.push(vec![
                    "git".into(),
                    "commit".into(),
                    "-m".into(),
                    format!(
                        "chore(release): version {}",
                        input.version.as_deref().unwrap_or("unknown")
                    ),
                ]);
                if let Some(tag) = &input.tag {
                    cmds.push(vec!["git".into(), "tag".into(), tag.clone()]);
                }
            }
            cmds
        }
    }
}

fn write_state_file(
    path: &std::path::Path,
    profile: &str,
    stages: &[ReleaseStageSnapshot],
    input: &ReleaseWorkflowInput,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let state = ReleaseStateSnapshot {
        version: 1,
        cwd: input.cwd.display().to_string(),
        profile: match profile {
            "web-app" | "web_app" => ReleaseProfile::WebApp,
            "service" => ReleaseProfile::Service,
            "custom" => ReleaseProfile::Custom,
            _ => ReleaseProfile::Package,
        },
        env: input.env.clone(),
        channel: input.channel.clone(),
        mode: format!("{:?}", input.mode).to_lowercase(),
        state_file: path.display().to_string(),
        active_range: vec![],
        updated_at_epoch_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        stages: stages.to_vec(),
        completed: stages
            .iter()
            .all(|s| s.status == ReleaseStageStatus::Completed),
        summary: vec![],
    };
    let json = serde_json::to_string_pretty(&state)?;
    std::fs::write(path, format!("{json}\n"))?;
    Ok(())
}
