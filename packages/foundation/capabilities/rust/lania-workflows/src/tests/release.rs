//! release 工作流的回归测试。
//!
//! 关键点：
//! - 包含异步/超时/取消等控制流
use super::*;

#[tokio::test]
async fn release_workflow_plans_version_tag_and_publish() {
    let (services, root) = services(ExecService::dry_run());
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).expect("repo dir exists");
    std::fs::write(
        repo.join("package.json"),
        "{\"name\":\"demo\",\"version\":\"0.1.0\"}\n",
    )
    .expect("package written");
    init_git_repo(&repo);

    let workflow = ReleaseWorkflow;
    let result = workflow
        .run(
            &services,
            ReleaseWorkflowInput {
                cwd: repo.clone(),
                mode: ReleaseMode::Plan,
                version: Some("1.2.3".into()),
                tag: Some("next".into()),
                profile: Some("package".into()),
                env: Some("test".into()),
                channel: None,
                from_stage: None,
                to_stage: None,
                skip_stages: Vec::new(),
                state_file: None,
                apply: false,
                dry_run: true,
                yes: false,
                publish: true,
                changelog: true,
                skip_git: false,
            },
        )
        .await
        .expect("release workflow succeeds");

    assert!(result
        .command_plans
        .iter()
        .any(|plan| plan.iter().any(|arg| arg == "publish")));
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("release profile")));
    assert!(repo.join(".lania/release-state.json").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn release_workflow_run_mode_executes_fully() {
    let (services, root) = services(ExecService::dry_run());
    let repo = root.join("repo");
    init_git_repo(&repo);

    let workflow = ReleaseWorkflow;
    let result = workflow
        .run(
            &services,
            ReleaseWorkflowInput {
                cwd: repo.clone(),
                mode: ReleaseMode::Run,
                version: Some("1.2.3".into()),
                tag: None,
                profile: None,
                env: None,
                channel: None,
                from_stage: None,
                to_stage: None,
                skip_stages: Vec::new(),
                state_file: None,
                apply: true,
                dry_run: false,
                yes: true,
                publish: false,
                changelog: true,
                skip_git: false,
            },
        )
        .await
        .expect("release run succeeds");

    assert_eq!(result.state, WorkflowState::Completed);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn release_workflow_resume_mode_uses_state_file() {
    let (services, root) = services(ExecService::dry_run());
    let repo = root.join("repo");
    init_git_repo(&repo);

    let workflow = ReleaseWorkflow;
    let result = workflow
        .run(
            &services,
            ReleaseWorkflowInput {
                cwd: repo.clone(),
                mode: ReleaseMode::Resume,
                version: None,
                tag: None,
                profile: None,
                env: None,
                channel: None,
                from_stage: Some("verify".into()),
                to_stage: None,
                skip_stages: Vec::new(),
                state_file: Some(
                    repo.join(".lania/release-state.json")
                        .display()
                        .to_string(),
                ),
                apply: true,
                dry_run: false,
                yes: true,
                publish: false,
                changelog: false,
                skip_git: false,
            },
        )
        .await
        .expect("release resume succeeds");

    assert_eq!(result.state, WorkflowState::Completed);
    let _ = std::fs::remove_dir_all(root);
}
