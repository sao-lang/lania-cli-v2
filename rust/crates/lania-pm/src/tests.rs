use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{PackageManager, PackageManagerService};

#[test]
fn detects_lockfile_preference() {
    let service = PackageManagerService;
    let manager = service.detect_from_files(["package.json", "pnpm-lock.yaml"]);
    assert_eq!(manager, PackageManager::Pnpm);
}

#[test]
fn builds_dev_install_command() {
    let service = PackageManagerService;
    let command = service.install_command(PackageManager::Yarn, &["eslint".into()], true);

    assert_eq!(command.program, "yarn");
    assert_eq!(command.args, vec!["add", "--dev", "eslint"]);
}

#[test]
fn matches_legacy_command_sets() {
    let service = PackageManagerService;
    let npm = service.spec(PackageManager::Npm);
    let pnpm = service.spec(PackageManager::Pnpm);

    assert_eq!(npm.add_subcommand, "install");
    assert_eq!(pnpm.add_subcommand, "install");
    assert_eq!(npm.save_dev_flag, "--save-dev");
    assert_eq!(
        pnpm.strict_peer_flag.as_deref(),
        Some("--strict-peer-dependencies=false")
    );
}

#[test]
fn supports_run_remove_update_and_lockfile_matrix() {
    let service = PackageManagerService;
    let bun_run = service.run_script_command(PackageManager::Bun, "dev", &["--hot".into()]);
    let npm_remove = service.remove_command(PackageManager::Npm, &["eslint".into()]);
    let managers = service.supported_managers();

    assert_eq!(bun_run.args, vec!["run", "dev", "--hot"]);
    assert_eq!(npm_remove.args, vec!["uninstall", "eslint"]);
    assert_eq!(managers.len(), 4);
    assert!(service
        .lockfile_strategy(PackageManager::Bun)
        .contains("bun.lockb"));
}

#[test]
fn converts_package_command_to_exec_command() {
    let service = PackageManagerService;
    let command = service.install_command(PackageManager::Npm, &["react".into()], false);
    let exec = command.to_exec_command();

    assert_eq!(exec.program, "npm");
    assert_eq!(
        exec.args,
        vec!["install", "--legacy-peer-deps", "--save", "react"]
    );
}

#[test]
fn builds_publish_command_with_tag() {
    let service = PackageManagerService;
    let command = service.publish_command(PackageManager::Pnpm, Some("next"));

    assert_eq!(command.program, "pnpm");
    assert_eq!(command.args, vec!["publish", "--tag", "next"]);
}

#[test]
fn builds_dependency_batches() {
    let service = PackageManagerService;
    let commands = service.add_dependency_commands(
        PackageManager::Npm,
        &["react".into()],
        &["typescript".into()],
    );

    assert_eq!(commands.len(), 2);
    assert_eq!(
        commands[0].args,
        vec!["install", "--legacy-peer-deps", "--save", "react"]
    );
    assert_eq!(
        commands[1].args,
        vec!["install", "--legacy-peer-deps", "--save-dev", "typescript"]
    );
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("lania-pm-{name}-{unique}"));
    fs::create_dir_all(&path).expect("temp dir created");
    path
}

#[test]
fn detects_from_cwd_lockfiles() {
    let service = PackageManagerService;
    let root = temp_dir("detect");
    fs::write(root.join("pnpm-lock.yaml"), "lock").expect("lock written");
    assert_eq!(service.detect_from_cwd(&root), PackageManager::Pnpm);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn loads_package_json_scripts_and_validates() {
    let service = PackageManagerService;
    let root = temp_dir("scripts");
    fs::write(
        root.join("package.json"),
        r#"{ "name": "demo", "scripts": { "dev": "vite", "build": "vite build" } }"#,
    )
    .expect("package.json written");

    let snapshot = service
        .load_package_json_snapshot(&root)
        .expect("snapshot loads");
    assert!(snapshot.exists);
    assert!(snapshot.scripts.contains_key("dev"));

    assert!(service.script_exists(&root, "build").expect("exists"));
    assert!(!service.script_exists(&root, "test").expect("exists"));

    service.require_script(&root, "dev").expect("dev exists");
    assert!(service.require_script(&root, "test").is_err());

    let command = service
        .run_script_command_checked(&root, PackageManager::Pnpm, "dev", &[])
        .expect("checked command");
    assert_eq!(command.program, "pnpm");
    assert_eq!(command.args[0], "run");

    let _ = fs::remove_dir_all(root);
}
