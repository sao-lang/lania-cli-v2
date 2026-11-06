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
    let command = service
        .install_command(PackageManager::Yarn, &["eslint".into()], true)
        .expect("yarn add --dev");

    assert_eq!(command.program, "yarn");
    assert_eq!(command.args, vec!["add", "--dev", "eslint"]);
}

#[test]
fn matches_legacy_command_sets() {
    let service = PackageManagerService;
    let npm = service.spec(PackageManager::Npm);
    let pnpm = service.spec(PackageManager::Pnpm);

    assert_eq!(npm.add_subcommand.as_deref(), Some("install"));
    assert_eq!(pnpm.add_subcommand.as_deref(), Some("install"));
    assert_eq!(npm.save_dev_flag.as_deref(), Some("--save-dev"));
    assert_eq!(
        pnpm.strict_peer_flag.as_deref(),
        Some("--strict-peer-dependencies=false")
    );
}

#[test]
fn supports_run_remove_update_and_lockfile_matrix() {
    let service = PackageManagerService;
    let bun_run = service
        .run_script_command(PackageManager::Bun, "dev", &["--hot".into()])
        .expect("bun run");
    let npm_remove = service
        .remove_command(PackageManager::Npm, &["eslint".into()])
        .expect("npm uninstall");
    let managers = service.supported_managers();

    assert_eq!(bun_run.args, vec!["run", "dev", "--hot"]);
    assert_eq!(npm_remove.args, vec!["uninstall", "eslint"]);
    assert_eq!(managers.len(), 12);
    assert!(service
        .lockfile_strategy(PackageManager::Bun)
        .contains("bun.lockb"));
}

#[test]
fn converts_package_command_to_exec_command() {
    let service = PackageManagerService;
    let command = service
        .install_command(PackageManager::Npm, &["react".into()], false)
        .expect("npm install");
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
    let command = service
        .publish_command(PackageManager::Pnpm, Some("next"))
        .expect("pnpm publish");

    assert_eq!(command.program, "pnpm");
    assert_eq!(command.args, vec!["publish", "--tag", "next"]);
    assert!(service
        .publish_command(PackageManager::Cargo, Some("next"))
        .is_err());
}

#[test]
fn builds_dependency_batches() {
    let service = PackageManagerService;
    let commands = service
        .add_dependency_commands(
            PackageManager::Npm,
            &["react".into()],
            &["typescript".into()],
        )
        .expect("dependency batches");

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

#[test]
fn builds_non_node_package_manager_commands() {
    let service = PackageManagerService;

    let pip = service
        .install_all_command(PackageManager::Pip)
        .expect("pip install");
    assert_eq!(pip.program, "python");
    assert_eq!(
        pip.args,
        vec![
            "-m",
            "pip",
            "install",
            "-r",
            "requirements.txt",
            "-r",
            "requirements-dev.txt"
        ]
    );

    let uv = service
        .install_all_command(PackageManager::Uv)
        .expect("uv install");
    assert_eq!(
        uv.args,
        vec![
            "pip",
            "install",
            "-r",
            "requirements.txt",
            "-r",
            "requirements-dev.txt"
        ]
    );

    let cargo = service
        .install_command(PackageManager::Cargo, &["serde".into()], false)
        .expect("cargo add");
    assert_eq!(cargo.args, vec!["add", "serde"]);

    let poetry = service
        .run_script_command(PackageManager::Poetry, "test", &["-x".into()])
        .expect("poetry run");
    assert_eq!(poetry.args, vec!["run", "test", "-x"]);

    let go = service
        .update_command(PackageManager::Go, &["example.com/mod".into()])
        .expect("go get -u");
    assert_eq!(go.args, vec!["get", "-u", "example.com/mod"]);

    let pub_install = service
        .install_all_command(PackageManager::Pub)
        .expect("dart pub get");
    assert_eq!(pub_install.program, "dart");
    assert_eq!(pub_install.args, vec!["pub", "get"]);

    let composer = service
        .install_command(PackageManager::Composer, &["monolog/monolog".into()], false)
        .expect("composer require");
    assert_eq!(composer.args, vec!["require", "monolog/monolog"]);

    assert!(service
        .remove_command(PackageManager::Go, &["x".into()])
        .is_err());
    assert!(service.init_command(PackageManager::Pip).is_err());
    assert!(service
        .run_script_command(PackageManager::Pip, "test", &[])
        .is_err());
    assert!(service
        .install_command(PackageManager::Go, &["x".into()], true)
        .is_err());
}

#[test]
fn detects_all_supported_ecosystems_from_cwd() {
    let service = PackageManagerService;

    let cases: &[(&str, &str, PackageManager)] = &[
        ("uv", "uv.lock", PackageManager::Uv),
        ("poetry", "poetry.lock", PackageManager::Poetry),
        ("cargo", "Cargo.toml", PackageManager::Cargo),
        ("go", "go.mod", PackageManager::Go),
        ("pub", "pubspec.yaml", PackageManager::Pub),
        ("bundler", "Gemfile", PackageManager::Bundler),
        ("composer", "composer.json", PackageManager::Composer),
    ];

    for (name, marker, expected) in cases {
        let root = temp_dir(name);
        fs::write(root.join(marker), "marker").expect("marker written");
        assert_eq!(service.detect_from_cwd(&root), *expected, "{name}");
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn detects_python_manager_from_pyproject_content() {
    let service = PackageManagerService;

    let root = temp_dir("pyproject-poetry");
    fs::write(
        root.join("pyproject.toml"),
        "[tool.poetry]\nname = \"demo\"\n",
    )
    .expect("pyproject written");
    assert_eq!(service.detect_from_cwd(&root), PackageManager::Poetry);
    let _ = fs::remove_dir_all(root);

    let root = temp_dir("pyproject-uv");
    fs::write(root.join("pyproject.toml"), "[tool.uv]\n").expect("pyproject written");
    assert_eq!(service.detect_from_cwd(&root), PackageManager::Uv);
    let _ = fs::remove_dir_all(root);

    let root = temp_dir("pyproject-pip");
    fs::write(root.join("pyproject.toml"), "[project]\nname = \"demo\"\n")
        .expect("pyproject written");
    assert_eq!(service.detect_from_cwd(&root), PackageManager::Pip);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn detects_by_language() {
    let service = PackageManagerService;

    let root = temp_dir("detect-language-node");
    fs::write(root.join("pnpm-lock.yaml"), "lock").expect("lock written");
    assert_eq!(
        service.detect_for_language(&root, "node"),
        Some(PackageManager::Pnpm)
    );
    assert_eq!(
        service.detect_for_language(&root, "python"),
        Some(PackageManager::Pip)
    );
    assert_eq!(
        service.detect_for_language(&root, "rust"),
        Some(PackageManager::Cargo)
    );
    assert_eq!(service.detect_for_language(&root, "unknown"), None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn parses_manager_aliases() {
    assert_eq!(PackageManager::parse("dart"), Some(PackageManager::Pub));
    assert_eq!(
        PackageManager::parse("bundle"),
        Some(PackageManager::Bundler)
    );
    assert_eq!(PackageManager::parse("pip3"), Some(PackageManager::Pip));
    assert_eq!(PackageManager::parse("nope"), None);
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
