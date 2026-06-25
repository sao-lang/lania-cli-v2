//! sync/release git 工作流 e2e。
use super::common::*;
use super::*;

#[test]
fn lan_sync_e2e() {
    let root = temp_dir("sync");
    run_command(&root, "git", &["init"]);
    run_command(&root, "git", &["config", "user.name", "Lania Test"]);
    run_command(&root, "git", &["config", "user.email", "lania@example.com"]);
    write_file(root.join("README.md"), "hello\n");
    run_command(&root, "git", &["add", "."]);
    run_command(&root, "git", &["commit", "-m", "init"]);
    let remote = root.join("remote.git");
    run_command(
        &root,
        "git",
        &["init", "--bare", remote.to_str().expect("remote path")],
    );
    run_command(
        &root,
        "git",
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().expect("remote path"),
        ],
    );
    let head_branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&root)
        .output()
        .expect("branch query");
    let branch = String::from_utf8_lossy(&head_branch.stdout)
        .trim()
        .to_string();
    run_command(&root, "git", &["push", "-u", "origin", &branch]);
    write_file(root.join("README.md"), "hello sync\n");

    let output = run_cli(
        &root,
        &["sync", "--push", "--message", "chore(sync): e2e"],
        &[],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert!(json["execution"]["command_plans"]
        .as_array()
        .expect("command plans")
        .iter()
        .any(|plan| {
            plan.as_array()
                .map(|items| items.iter().any(|item| item == "push"))
                .unwrap_or(false)
        }));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_sync_status_e2e() {
    let root = temp_dir("sync-status");
    run_command(&root, "git", &["init"]);
    run_command(&root, "git", &["config", "user.name", "Lania Test"]);
    run_command(&root, "git", &["config", "user.email", "lania@example.com"]);
    write_file(root.join("README.md"), "hello\n");
    run_command(&root, "git", &["add", "."]);
    run_command(&root, "git", &["commit", "-m", "init"]);
    write_file(root.join("README.md"), "hello status\n");

    let output = run_cli(&root, &["sync", "status"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert_eq!(json["execution"]["prompts"]["mode"], "status");
    assert_eq!(
        json["execution"]["command_plans"]
            .as_array()
            .expect("command plans")
            .len(),
        0
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_sync_invalid_commit_message_exits_non_zero() {
    let root = temp_dir("sync-invalid-message");
    run_command(&root, "git", &["init"]);
    run_command(&root, "git", &["config", "user.name", "Lania Test"]);
    run_command(&root, "git", &["config", "user.email", "lania@example.com"]);
    write_file(root.join("README.md"), "hello\n");
    run_command(&root, "git", &["add", "."]);
    run_command(&root, "git", &["commit", "-m", "init"]);
    write_file(root.join("README.md"), "hello invalid\n");

    let output = run_cli(
        &root,
        &["sync", "commit", "--message", "invalid message"],
        &[],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("commitlint rejected commit message"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_sync_requires_git_repo_zh_e2e() {
    let root = temp_dir("sync-no-git-zh");
    let home = isolated_home(&root);
    write_file(
        Path::new(&home).join(".lania").join("preferences.json"),
        r#"{
  "locale": "zh",
  "outputMode": "json",
  "logTimestamps": false
}
"#,
    );

    let output = run_cli(&root, &["sync"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("Git 仓库未就绪"));
    assert!(stdout.contains("Git 仓库未就绪"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_release_e2e() {
    let root = temp_dir("release");
    run_command(&root, "git", &["init"]);
    run_command(&root, "git", &["config", "user.name", "Lania Test"]);
    run_command(&root, "git", &["config", "user.email", "lania@example.com"]);
    write_file(
        root.join("package.json"),
        "{\n  \"name\": \"demo-release\",\n  \"version\": \"0.1.0\"\n}\n",
    );
    run_command(&root, "git", &["add", "."]);
    run_command(&root, "git", &["commit", "-m", "init"]);

    let output = run_cli(
        &root,
        &[
            "release",
            "--version",
            "1.2.3",
            "--publish",
            "--tag",
            "next",
            "--changelog",
        ],
        &[],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert_eq!(json["execution"]["workflow"], "release");
    assert_eq!(json["execution"]["prompts"]["mode"], "plan");
    assert_eq!(json["execution"]["prompts"]["profile"], "package");
    assert_eq!(json["execution"]["prompts"]["completed"], false);
    assert!(json["execution"]["command_plans"]
        .as_array()
        .expect("command plans")
        .iter()
        .any(|plan| {
            plan.as_array()
                .map(|items| items.iter().any(|item| item == "publish"))
                .unwrap_or(false)
        }));
    assert!(json["execution"]["notes"]
        .as_array()
        .expect("notes array")
        .iter()
        .any(|note| note
            .as_str()
            .unwrap_or_default()
            .contains("release profile")));
    assert!(root.join(".lania/release-state.json").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_release_web_app_run_e2e() {
    let root = temp_dir("release-web-app");
    run_command(&root, "git", &["init"]);
    run_command(&root, "git", &["config", "user.name", "Lania Test"]);
    run_command(&root, "git", &["config", "user.email", "lania@example.com"]);

    write_file(
        root.join("lan.config.js"),
        r#"export default {
  release: {
    profile: 'web-app',
    env: 'prod',
    channel: 'stable',
    versioning: { enabled: false },
    git: { commit: false, tag: false, push: false },
    artifact: {
      enabled: true,
      command: 'mkdir -p dist && printf artifact > dist/app.txt'
    },
    deploy: {
      provider: 'custom',
      command: 'printf ok > deploy.ok'
    },
    postCheck: {
      command: 'printf ok > post-check.ok'
    }
  }
};
"#,
    );

    let output = run_cli(&root, &["release", "run", "--apply", "--yes"], &[]);
    let json = parse_stdout_json(&output);
    let state = read_json_file(root.join(".lania/release-state.json"));

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["execution"]["workflow"], "release");
    assert_eq!(json["execution"]["prompts"]["profile"], "web-app");
    assert!(json["execution"]["command_plans"]
        .as_array()
        .expect("command plans")
        .iter()
        .any(|plan| plan
            .as_array()
            .expect("plan array")
            .iter()
            .any(|value| value.as_str().unwrap_or_default().contains("deploy.ok"))));
    assert_eq!(state["completed"], true);
    assert_eq!(state["stages"][4]["status"], "completed");
    assert_eq!(state["stages"][5]["status"], "completed");
    assert_eq!(state["stages"][6]["status"], "completed");

    let _ = fs::remove_dir_all(root);
}
