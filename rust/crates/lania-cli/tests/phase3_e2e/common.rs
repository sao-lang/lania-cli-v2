//! phase3 e2e 共享夹具与文件系统辅助。
//!
//! 保持这些辅助函数集中，便于各子模块共享同一套 CLI/Node bridge 搭建逻辑。
use super::*;

pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lania-cli-phase3-{name}-{unique}"));
    fs::create_dir_all(&root).expect("temp dir created");
    root
}

pub(crate) fn write_file(path: impl AsRef<Path>, content: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent dir created");
    }
    fs::write(path, content).expect("file written");
}

pub(crate) fn isolated_home(root: &Path) -> String {
    let home = root.join("home");
    fs::create_dir_all(&home).expect("home dir created");
    home.display().to_string()
}

pub(crate) fn read_json_file(path: impl AsRef<Path>) -> serde_json::Value {
    let content = fs::read_to_string(path).expect("json file readable");
    serde_json::from_str(&content).expect("valid json file")
}

fn default_home_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    let home = std::env::temp_dir().join(format!("lania-cli-phase3-home-{unique}"));
    fs::create_dir_all(&home).expect("default home dir created");
    home
}

pub(crate) fn run_cli(
    cwd: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    run_program(
        Path::new(env!("CARGO_BIN_EXE_lania-cli")),
        cwd,
        args,
        extra_env,
    )
}

pub(crate) fn run_cli_interactive(
    cwd: &Path,
    args: &[&str],
    answers: &[&str],
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    run_program_interactive(
        Path::new(env!("CARGO_BIN_EXE_lania-cli")),
        cwd,
        args,
        answers,
        extra_env,
    )
}

pub(crate) fn run_program(
    program: &Path,
    cwd: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    // Test cases mutate installed-mode routing env vars in-process. Clear them here so child
    // CLI invocations only see explicitly requested values and do not inherit cross-test state.
    command.env_remove("LANIA_PRODUCT_ROOT");
    command.env_remove("LANIA_NODE_BRIDGE_DIR");
    command.env_remove("LANIA_RUNTIME_MODE");
    if extra_env.iter().all(|(key, _)| *key != "HOME") {
        command.env("HOME", default_home_dir());
    }
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.output().expect("cli runs")
}

pub(crate) fn run_program_interactive(
    program: &Path,
    cwd: &Path,
    args: &[&str],
    answers: &[&str],
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    let mut command = Command::new("script");
    command
        .arg("-q")
        .arg("/dev/null")
        .arg(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env_remove("LANIA_PRODUCT_ROOT");
    command.env_remove("LANIA_NODE_BRIDGE_DIR");
    command.env_remove("LANIA_RUNTIME_MODE");
    if extra_env.iter().all(|(key, _)| *key != "HOME") {
        command.env("HOME", default_home_dir());
    }
    for (key, value) in extra_env {
        command.env(key, value);
    }

    let mut child = command.spawn().expect("interactive cli runs");
    std::thread::sleep(std::time::Duration::from_millis(150));
    {
        let stdin = child.stdin.as_mut().expect("stdin available");
        for answer in answers {
            stdin
                .write_all(format!("{answer}\n").as_bytes())
                .expect("answer written");
        }
    }
    child.wait_with_output().expect("interactive cli output")
}

pub(crate) fn parse_stdout_json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let start = stdout
            .find('{')
            .expect("stdout transcript should contain json payload");
        serde_json::from_str(stdout[start..].trim())
            .expect("stdout transcript should end with valid json")
    })
}

pub(crate) fn run_command(cwd: &Path, program: &str, args: &[&str]) {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("command should run");
    assert!(status.success(), "{program} {:?} should succeed", args);
}

pub(crate) fn write_fake_package(root: &Path, name: &str, main_js: &str) {
    let package_root = root.join("node_modules").join(name);
    write_file(
        package_root.join("package.json"),
        &format!("{{\"name\":\"{name}\",\"version\":\"1.0.0\",\"main\":\"index.js\"}}\n"),
    );
    write_file(package_root.join("index.js"), main_js);
}

pub(crate) fn write_contract_fixture(root: &Path, targets: &[&str]) {
    let targets_yaml = targets
        .iter()
        .map(|target| format!("      - {target}\n"))
        .collect::<String>();
    write_file(
        root.join("lania.contract.yaml"),
        &format!(
            "version: 1\nentries:\n  - name: user-service\n    source:\n      kind: proto\n      inputs:\n        - schemas/proto/user.proto\n    targets:\n{targets_yaml}"
        ),
    );
    write_file(
        root.join("schemas/proto/user.proto"),
        "message User {\n  string id = 1;\n}\n\nservice UserService {\n  rpc GetUser (User) returns (User);\n}\n",
    );
}

pub(crate) fn write_module_fixture(root: &Path, targets: &[&str], inject: bool) {
    let targets_yaml = targets
        .iter()
        .map(|target| format!("  - kind: {target}\n"))
        .collect::<String>();
    write_file(
        root.join("lania.module.yaml"),
        &format!(
            "version: 1\nframework:\n  name: lania-g\n  language: go\n  main: main.go\ninputs:\n  - name: user\n    source: protobuf\n    path: schemas/proto\n    include:\n      - \"**/*.proto\"\ntargets:\n{targets_yaml}output:\n  root: generated/lania\n  moduleDir: generated/lania/modules\n  adapterDir: generated/lania/adapters\n  contractDir: generated/lania/contracts\n  manifest: .lania/module-gen.lock.json\ninject:\n  enabled: {}\n  targetMain: main.go\n  marker:\n    start: \"lania:modules:start\"\n    end: \"lania:modules:end\"\n",
            if inject { "true" } else { "false" }
        ),
    );
    write_file(
        root.join("schemas/proto/user.proto"),
        "message User {\n  string id = 1;\n}\n\nservice UserService {\n  rpc GetUser (User) returns (User);\n}\n",
    );
    if inject {
        write_file(
            root.join("main.go"),
            "package main\n\nfunc main() {\n    // lania:modules:start\n    // lania:modules:end\n}\n",
        );
    }
}

pub(crate) fn write_graphql_module_fixture(root: &Path, targets: &[&str]) {
    let targets_yaml = targets
        .iter()
        .map(|target| format!("  - kind: {target}\n"))
        .collect::<String>();
    write_file(
        root.join("lania.module.yaml"),
        &format!(
            "version: 1\nframework:\n  name: lania-g\ninputs:\n  - name: gateway\n    source: graphql\n    path: schemas/graphql\n    include:\n      - \"**/*.graphql\"\ntargets:\n{targets_yaml}inject:\n  enabled: false\n",
        ),
    );
    write_file(
        root.join("schemas/graphql/schema.graphql"),
        "type User {\n  id: ID!\n}\n\ntype Query {\n  user(id: ID!): User\n}\n\ntype Subscription {\n  userUpdated: User\n}\n",
    );
}

pub(crate) fn write_json_module_fixture(root: &Path, targets: &[&str]) {
    let targets_yaml = targets
        .iter()
        .map(|target| format!("  - kind: {target}\n"))
        .collect::<String>();
    write_file(
        root.join("lania.module.yaml"),
        &format!(
            "version: 1\nframework:\n  name: lania-g\ninputs:\n  - name: account\n    source: json\n    path: schemas/json\ntargets:\n{targets_yaml}overrides:\n  operations:\n    GetAccount:\n      service: AccountService\n      input: GetAccountRequest\n      output: Account\n      kind: query\n      http:\n        method: GET\n        path: /accounts/:id\n      grpc:\n        service: AccountService\n        method: GetAccount\ninject:\n  enabled: false\n",
        ),
    );
    write_file(
        root.join("schemas/json/account.json"),
        "{\n  \"title\": \"Account\",\n  \"type\": \"object\",\n  \"properties\": {\n    \"id\": { \"type\": \"string\" },\n    \"enabled\": { \"type\": \"boolean\" }\n  }\n}\n",
    );
}

pub(crate) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root resolves")
}

pub(crate) fn node_bridge_package_dir() -> PathBuf {
    repo_root().join("ts/packages/node-bridge")
}

pub(crate) fn templates_package_dir() -> PathBuf {
    repo_root().join("ts/packages/templates")
}

pub(crate) fn ensure_node_bridge_dist() {
    static BUILD_RESULT: OnceLock<Result<(), String>> = OnceLock::new();
    let result = BUILD_RESULT.get_or_init(|| {
        let bridge_dist = node_bridge_package_dir().join("dist/entry/stdio.js");
        let templates_dist = templates_package_dir().join("dist/index.js");
        if bridge_dist.exists() && templates_dist.exists() {
            return Ok(());
        }

        // Try to build on demand for a smoother local dev experience.
        // This makes tests less sensitive to whether someone remembered to run the TS build.
        let ts_root = repo_root().join("ts");
        let output = Command::new("pnpm")
            .current_dir(&ts_root)
            .args(["-r", "build"])
            .output()
            .map_err(|error| {
                format!(
                    "missing built TS workspace assets and failed to spawn pnpm in {}: {error}",
                    ts_root.display()
                )
            })?;
        if !output.status.success() {
            return Err(format!(
                "missing built TS workspace assets; attempted `pnpm -r build` in {}, but it failed:\nstdout:\n{}\nstderr:\n{}",
                ts_root.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        if bridge_dist.exists() && templates_dist.exists() {
            Ok(())
        } else {
            Err(format!(
                "missing built TS workspace assets after `pnpm -r build` in {}: expected {}, {}",
                ts_root.display(),
                bridge_dist.display(),
                templates_dist.display(),
            ))
        }
    });
    if let Err(message) = result {
        panic!("{message}");
    }
}

pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("target dir created");
    for entry in fs::read_dir(src).expect("source dir readable") {
        let entry = entry.expect("dir entry readable");
        let source_path = entry.path();
        let target_path = dst.join(entry.file_name());
        let file_type = fs::symlink_metadata(&source_path)
            .expect("file type readable")
            .file_type();
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path);
        } else if file_type.is_symlink() {
            let link_target = fs::read_link(&source_path).expect("symlink readable");
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).expect("parent dir created");
            }
            #[cfg(unix)]
            unix_fs::symlink(&link_target, &target_path).expect("symlink copied");
        } else {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).expect("parent dir created");
            }
            fs::copy(&source_path, &target_path).expect("file copied");
        }
    }
}

pub(crate) fn remove_path_if_exists(path: &Path) {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            fs::remove_file(path).expect("path removed");
        }
        Ok(metadata) if metadata.is_dir() => {
            fs::remove_dir_all(path).expect("directory removed");
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("failed to inspect {}: {error}", path.display()),
    }
}

pub(crate) fn stage_installed_bridge_package(root: &Path) -> PathBuf {
    ensure_node_bridge_dist();
    let source = node_bridge_package_dir();
    let templates = templates_package_dir();
    let target = root.join("lib/node-bridge");
    let node_modules_target = target.join("node_modules");
    let templates_target = target.join("node_modules/@lania-cli/templates");
    fs::create_dir_all(&target).expect("bridge dir created");
    fs::copy(source.join("package.json"), target.join("package.json")).expect("package copied");
    copy_dir_recursive(&source.join("dist"), &target.join("dist"));
    copy_dir_recursive(&source.join("node_modules"), &node_modules_target);
    remove_path_if_exists(&templates_target);
    fs::create_dir_all(&templates_target).expect("templates dir created");
    fs::copy(
        templates.join("package.json"),
        templates_target.join("package.json"),
    )
    .expect("templates package copied");
    copy_dir_recursive(&templates.join("dist"), &templates_target.join("dist"));
    copy_dir_recursive(
        &templates.join("src/templates"),
        &templates_target.join("src/templates"),
    );
    target
}
