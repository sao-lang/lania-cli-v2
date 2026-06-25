use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{GitError, GitErrorCode, GitService};

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    std::env::temp_dir().join(format!("lania-git-{name}-{unique}"))
}

fn init_repo(service: &GitService, root: &Path) {
    fs::create_dir_all(root).expect("repo root created");
    service.init(root).expect("git repo initialized");
    service
        .set_user(root, "Lania Test", "lania@example.com")
        .expect("git user configured");
}

fn init_bare_repo(service: &GitService, root: &Path) {
    fs::create_dir_all(root).expect("repo root created");
    service
        .run(root, ["init", "--bare"])
        .expect("bare repo initialized");
}

fn commit_file(
    service: &GitService,
    root: &Path,
    path: &str,
    content: &str,
    message: &str,
) -> String {
    fs::write(root.join(path), content).expect("file written");
    service.add_all(root).expect("staged");
    service.commit(root, message).expect("commit succeeds");
    service.last_commit_hash(root).expect("hash")
}

#[test]
fn reports_non_repo_as_not_ready() {
    let service = GitService::default();
    let status = service.status("/").expect("status should be queryable");
    assert!(!status.ready);
    assert_eq!(status.branch, None);
}

#[test]
fn lists_branch_remote_tag_and_user_helpers() {
    let service = GitService::default();
    let root = temp_repo("helpers");
    init_repo(&service, &root);
    fs::write(root.join("README.md"), "hello").expect("file written");
    service.add_all(&root).expect("staged");
    service.commit(&root, "init").expect("commit succeeds");
    service.run(&root, ["tag", "v0.1.0"]).expect("tag created");
    service
        .run(
            &root,
            ["remote", "add", "origin", "https://example.com/demo.git"],
        )
        .expect("remote added");

    let status = service.status(&root).expect("status");
    assert!(status.ready);
    assert!(service
        .branch_exists_local(&root, status.branch.as_deref().unwrap_or("master"))
        .expect("branch query"));
    assert!(service
        .remote_exists(&root, "origin")
        .expect("remote query"));
    assert!(service
        .tags(&root)
        .expect("tags")
        .contains(&"v0.1.0".to_string()));
    let user = service.user(&root).expect("user query");
    assert_eq!(user.name, "Lania Test");
    assert_eq!(service.last_commit_message(&root).expect("message"), "init");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn classifies_missing_upstream_errors() {
    let service = GitService::default();
    let root = temp_repo("upstream");
    init_repo(&service, &root);
    fs::write(root.join("README.md"), "hello").expect("file written");
    service.add_all(&root).expect("staged");
    service.commit(&root, "init").expect("commit succeeds");

    let upstream = service.upstream(&root).expect("upstream query");
    assert!(upstream.is_none());
    assert!(service.has_unpushed_commits(&root).expect("unpushed query"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn supports_branch_remote_and_tag_write_operations() {
    let service = GitService::default();
    let root = temp_repo("branch-remote-tag");
    init_repo(&service, &root);

    // branch create + switch
    service
        .branch_create(&root, "feature/test")
        .expect("branch created");
    assert_eq!(
        service.current_branch(&root).expect("branch"),
        Some("feature/test".into())
    );
    service.branch_switch(&root, "master").ok(); // some env default branch could be main; ignore

    // tag create/delete (lightweight)
    commit_file(&service, &root, "README.md", "hello", "init");
    service
        .tag_create_lightweight(&root, "v0.1.0")
        .expect("tag created");
    assert!(service
        .tags(&root)
        .expect("tags")
        .contains(&"v0.1.0".to_string()));
    service.tag_delete(&root, "v0.1.0").expect("tag deleted");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn supports_revert_and_commit_log_and_files() {
    let service = GitService::default();
    let root = temp_repo("revert-log");
    init_repo(&service, &root);

    let first = commit_file(&service, &root, "a.txt", "a1", "feat: add a");
    let second = commit_file(&service, &root, "b.txt", "b1", "feat: add b");

    let files = service.commit_files(&root, &second).expect("commit files");
    assert!(files.iter().any(|item| item.ends_with("b.txt")));

    let log = service
        .commit_log(
            &root,
            super::GitCommitLogOptions {
                limit: Some(10),
                oneline: true,
                ..super::GitCommitLogOptions::default()
            },
        )
        .expect("log");
    assert!(log
        .iter()
        .any(|entry| entry.hash.starts_with(&first[..7]) || entry.hash == first));

    // revert the second commit without opening editor
    service
        .revert(
            &root,
            std::slice::from_ref(&second),
            super::GitRevertOptions {
                no_edit: true,
                ..super::GitRevertOptions::default()
            },
        )
        .expect("revert succeeds");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn supports_stage_and_workspace_helpers() {
    let service = GitService::default();
    let root = temp_repo("stage-workspace");
    init_repo(&service, &root);
    fs::write(root.join("a.txt"), "hello").expect("file written");
    let changed = service
        .workspace_changed_files(&root)
        .expect("changed files");
    assert!(changed.iter().any(|item| item.ends_with("a.txt")));
    assert!(!service.workspace_is_clean(&root).expect("clean"));

    service
        .add(&root, &[String::from("a.txt")])
        .expect("stage add");
    let staged = service.stage_files(&root).expect("staged files");
    assert!(staged.iter().any(|item| item.ends_with("a.txt")));
    let diff = service.stage_diff(&root).expect("diff");
    assert!(!diff.trim().is_empty());
    service.stage_reset(&root, "a.txt").expect("reset");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn supports_push_to_bare_remote() {
    let service = GitService::default();
    let remote = temp_repo("remote-bare");
    let root = temp_repo("push-work");
    init_bare_repo(&service, &remote);
    init_repo(&service, &root);
    service
        .remote_add(&root, "origin", remote.to_string_lossy().as_ref())
        .expect("remote added");
    commit_file(&service, &root, "README.md", "hello", "init");

    // Ensure branch name exists (use current)
    let branch = service
        .current_branch(&root)
        .expect("branch")
        .unwrap_or_else(|| "master".into());
    // First push sets upstream
    service
        .set_upstream(&root, "origin", &branch)
        .expect("set upstream");
    assert!(service
        .remote_exists(&root, "origin")
        .expect("remote exists"));

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(remote);
}

#[test]
fn exposes_typed_git_error() {
    let service = GitService::default();
    let root = temp_repo("error");
    fs::create_dir_all(&root).expect("root created");
    let error = service
        .commit(&root, "no repo")
        .expect_err("commit should fail");
    let git_error = error.downcast_ref::<GitError>().expect("git error");
    assert_eq!(git_error.code, GitErrorCode::NotRepository);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn maps_missing_binary_from_exec() {
    let service = GitService::new("lania-git-command-that-should-not-exist");
    let error = service.version().expect_err("missing binary should fail");
    let git_error = error.downcast_ref::<GitError>().expect("git error");

    assert_eq!(git_error.code, GitErrorCode::BinaryMissing);
}
