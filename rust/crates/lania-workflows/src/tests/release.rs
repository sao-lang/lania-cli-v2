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
async fn release_workflow_executes_web_app_deploy_pipeline() {
    let (services, root) = services(ExecService::new(false));
    let repo = root.join("repo");
    init_git_repo(&repo);
    let state_file = repo.join(".lania/release-state.json");
    let plan = release_test_plan(&repo, &state_file, "printf ok > deploy.ok");
    let status = services.git.status(&repo).expect("git status available");
    let snapshot = release_state_from_plan(&plan, &status).expect("release state prepared");

    let execution = execute_release_plan(&services, &plan, snapshot, status)
        .await
        .expect("release execution succeeds");

    let persisted = read_release_state(&state_file).expect("release state persisted");
    assert_eq!(execution.state, WorkflowState::Completed);
    assert!(repo.join("dist/app.txt").exists());
    assert!(repo.join("deploy.ok").exists());
    assert!(repo.join("post-check.ok").exists());
    assert_eq!(
        stage_status(&persisted, ReleaseStage::Artifact),
        ReleaseStageStatus::Completed
    );
    assert_eq!(
        stage_status(&persisted, ReleaseStage::PublishOrDeploy),
        ReleaseStageStatus::Completed
    );
    assert_eq!(
        stage_status(&persisted, ReleaseStage::PostCheck),
        ReleaseStageStatus::Completed
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn release_workflow_resumes_after_failed_deploy() {
    let (services, root) = services(ExecService::new(false));
    let repo = root.join("repo");
    init_git_repo(&repo);
    let state_file = repo.join(".lania/release-state.json");
    let failing_plan = release_test_plan(&repo, &state_file, "false");
    let status = services.git.status(&repo).expect("git status available");
    let snapshot = release_state_from_plan(&failing_plan, &status).expect("release state prepared");

    let first_execution = execute_release_plan(&services, &failing_plan, snapshot, status.clone())
        .await
        .expect("release execution returns failed workflow");
    let failed_state = read_release_state(&state_file).expect("failed release state persisted");

    assert_eq!(first_execution.state, WorkflowState::Failed);
    assert_eq!(
        stage_status(&failed_state, ReleaseStage::Artifact),
        ReleaseStageStatus::Completed
    );
    assert_eq!(
        stage_status(&failed_state, ReleaseStage::PublishOrDeploy),
        ReleaseStageStatus::Failed
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("artifact.count")).unwrap(),
        "first\n"
    );

    let resumed_plan = release_test_plan(&repo, &state_file, "printf ok > deploy.ok");
    let resumed_snapshot = merge_release_state(
        release_state_from_plan(&resumed_plan, &status).expect("resume state prepared"),
        Some(failed_state),
    );
    let resumed_execution =
        execute_release_plan(&services, &resumed_plan, resumed_snapshot, status)
            .await
            .expect("resume execution succeeds");
    let resumed_state = read_release_state(&state_file).expect("resumed state persisted");

    assert_eq!(resumed_execution.state, WorkflowState::Completed);
    assert_eq!(
        std::fs::read_to_string(repo.join("artifact.count")).unwrap(),
        "first\n"
    );
    assert!(repo.join("deploy.ok").exists());
    assert!(repo.join("post-check.ok").exists());
    assert_eq!(
        stage_status(&resumed_state, ReleaseStage::PublishOrDeploy),
        ReleaseStageStatus::Completed
    );
    assert_eq!(
        stage_status(&resumed_state, ReleaseStage::PostCheck),
        ReleaseStageStatus::Completed
    );
    let _ = std::fs::remove_dir_all(root);
}
