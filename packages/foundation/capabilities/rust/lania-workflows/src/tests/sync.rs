//! sync 工作流的回归测试。
//!
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含子进程/环境变量交互
use super::*;

#[tokio::test]
async fn sync_workflow_builds_git_command_plan() {
    let (services, root) = services(ExecService::dry_run());
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).expect("repo dir exists");
    std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo)
        .output()
        .expect("git init succeeds");
    std::process::Command::new("git")
        .args(["config", "user.name", "Lania Test"])
        .current_dir(&repo)
        .output()
        .expect("git user name succeeds");
    std::process::Command::new("git")
        .args(["config", "user.email", "lania@example.com"])
        .current_dir(&repo)
        .output()
        .expect("git user email succeeds");
    std::fs::write(repo.join("README.md"), "hello\n").expect("readme written");
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&repo)
        .output()
        .expect("git add succeeds");
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&repo)
        .output()
        .expect("git commit succeeds");
    let remote = repo.join("remote.git");
    std::process::Command::new("git")
        .args(["init", "--bare", remote.display().to_string().as_str()])
        .current_dir(&repo)
        .output()
        .expect("git bare remote succeeds");
    std::process::Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            remote.display().to_string().as_str(),
        ])
        .current_dir(&repo)
        .output()
        .expect("git remote add succeeds");
    let branch_output = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo)
        .output()
        .expect("branch query succeeds");
    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();
    std::process::Command::new("git")
        .args(["push", "-u", "origin", &branch])
        .current_dir(&repo)
        .output()
        .expect("git push succeeds");
    std::fs::write(repo.join("README.md"), "hello sync\n").expect("readme updated");

    let workflow = SyncWorkflow;
    let result = workflow
        .run(
            &services,
            SyncWorkflowInput {
                cwd: repo.clone(),
                remote: Some("origin".into()),
                branch: Some(branch.clone()),
                message: Some("chore(sync): save changes".into()),
                push: Some(true),
                amend: false,
                force_with_lease: false,
                dry_run: false,
                interactive: false,
                mode: SyncMode::Sync,
            },
        )
        .await
        .expect("sync workflow succeeds");

    assert!(result
        .command_plans
        .iter()
        .any(|plan| plan == &vec!["git".to_string(), "add".to_string(), "-A".to_string(),]));
    assert!(result.command_plans.iter().any(|plan| plan
        == &vec![
            "git".to_string(),
            "commit".to_string(),
            "-m".to_string(),
            "chore(sync): save changes".to_string(),
        ]));
    assert!(result.command_plans.iter().any(|plan| plan
        == &vec![
            "git".to_string(),
            "push".to_string(),
            "origin".to_string(),
            branch.clone(),
        ]));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn sync_workflow_validates_remote_and_branch_errors() {
    let (services, root) = services(ExecService::dry_run());
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).expect("repo dir exists");
    std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo)
        .output()
        .expect("git init succeeds");

    let workflow = SyncWorkflow;
    let error = workflow
        .run(
            &services,
            SyncWorkflowInput {
                cwd: repo.clone(),
                remote: Some("origin".into()),
                branch: Some("missing".into()),
                message: None,
                push: Some(true),
                amend: false,
                force_with_lease: false,
                dry_run: false,
                interactive: false,
                mode: SyncMode::Push,
            },
        )
        .await
        .expect_err("sync workflow should reject invalid targets");

    assert!(!error.to_string().trim().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn sync_workflow_rejects_invalid_commit_message_via_commitlint() {
    let (services, root) = services(ExecService::dry_run());
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).expect("repo dir exists");
    std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo)
        .output()
        .expect("git init succeeds");
    std::process::Command::new("git")
        .args(["config", "user.name", "Lania Test"])
        .current_dir(&repo)
        .output()
        .expect("git user name succeeds");
    std::process::Command::new("git")
        .args(["config", "user.email", "lania@example.com"])
        .current_dir(&repo)
        .output()
        .expect("git user email succeeds");
    std::fs::write(repo.join("README.md"), "hello\n").expect("readme written");
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&repo)
        .output()
        .expect("git add succeeds");
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&repo)
        .output()
        .expect("git commit succeeds");
    std::fs::write(repo.join("README.md"), "hello invalid\n").expect("readme updated");
    let branch_output = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo)
        .output()
        .expect("branch query succeeds");
    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    let workflow = SyncWorkflow;
    let error = workflow
        .run(
            &services,
            SyncWorkflowInput {
                cwd: repo.clone(),
                remote: None,
                branch: Some(branch),
                message: Some("invalid message".into()),
                push: Some(false),
                amend: false,
                force_with_lease: false,
                dry_run: true,
                interactive: false,
                mode: SyncMode::Commit,
            },
        )
        .await
        .expect_err("invalid commit message should be rejected");

    assert!(error
        .to_string()
        .contains("commitlint rejected commit message"));
    let _ = std::fs::remove_dir_all(root);
}
