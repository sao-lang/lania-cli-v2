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

/// 返回应执行的二进制与转发 argv。
///
/// 总包静态注册的命令（仅 config）与全局 flag 直接走总包、argv 原样；
/// 其余命令按 launcher 二级路由语义调度（§5.3 C.4）：
/// - `lan workspace <sub>` → `lan-workspace[.exe]`，剥掉 "workspace" 后转发；
/// - `lan product <sub>`（dev/build/pack/publish/inspect/doctor）→ `lan-workspace[.exe]`，
///   argv **完整保留**（含 "product"）——子包内同时有 workspace.* 与 product.*，靠
///   "product" 区分；
/// - 其它命令调度到 `lan-<cmd>[.exe]`，剥掉命令名后转发。
pub(crate) fn resolve_command_argv<'a>(args: &'a [&'a str]) -> (PathBuf, &'a [&'a str]) {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_lania-cli"));
    let Some(command) = args.first() else {
        return (cli, args);
    };
    if *command == "config" || command.starts_with('-') {
        return (cli, args);
    }
    const PRODUCT_SUBCOMMANDS: [&str; 6] = ["dev", "build", "pack", "publish", "inspect", "doctor"];
    let (exe_command, forwarded): (&str, &[&str]) = if *command == "workspace" {
        ("workspace", &args[1..])
    } else if *command == "product"
        && args
            .get(1)
            .is_some_and(|sub| PRODUCT_SUBCOMMANDS.contains(sub))
    {
        ("workspace", args)
    } else {
        (command, &args[1..])
    };
    let exe_name = if cfg!(windows) {
        format!("lan-{exe_command}.exe")
    } else {
        format!("lan-{exe_command}")
    };
    let candidate = cli
        .parent()
        .expect("cli binary should have a parent dir")
        .join(exe_name);
    if candidate.exists() {
        (candidate, forwarded)
    } else {
        (cli, args)
    }
}

pub(crate) fn run_cli(
    cwd: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    let (binary, forwarded) = resolve_command_argv(args);
    run_program(&binary, cwd, forwarded, extra_env)
}

pub(crate) fn run_cli_interactive(
    cwd: &Path,
    args: &[&str],
    answers: &[&str],
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    let (binary, forwarded) = resolve_command_argv(args);
    run_program_interactive(&binary, cwd, forwarded, answers, extra_env)
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

#[cfg(not(unix))]
pub(crate) fn run_program_interactive(_program: &std::path::Path, _cwd: &std::path::Path, _args: &[&str], _answers: &[&str], _extra_env: &[(&str, &str)],
) -> std::process::Output {
    panic!("run_program_interactive is only supported on Unix");
}

#[cfg(unix)]
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
    // 临时仓库必须与开发机的全局 git 配置隔离：core.hooksPath（husky）会让
    // `git commit` 在测试里挂死等输入；gpgsign/autocrlf 同理会造成假失败。
    let git_guard: &[&str] = if program == "git" {
        &["-c", "core.hooksPath=", "-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"]
    } else {
        &[]
    };
    let status = Command::new(program)
        .args(git_guard.iter().chain(args.iter()))
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

pub(crate) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root resolves")
}

pub(crate) fn node_bridge_package_dir() -> PathBuf {
    repo_root().join("packages/foundation/bridge-runtime/ts")
}

pub(crate) fn templates_package_dir() -> PathBuf {
    repo_root().join("packages/foundation/templates")
}

pub(crate) fn agent_package_dir() -> PathBuf {
    repo_root().join("packages/products/agent/ts/agent")
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
        let output = Command::new("pnpm")
            .current_dir(repo_root())
            .args(["-r", "build"])
            .output()
            .map_err(|error| {
                format!("missing built TS workspace assets and failed to spawn pnpm: {error}")
            })?;
        if !output.status.success() {
            return Err(format!(
                "missing built TS workspace assets; attempted `pnpm -r build`, but it failed://nstdout://n{}//nstderr://n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        if bridge_dist.exists() && templates_dist.exists() {
            Ok(())
        } else {
            Err(format!(
                "missing built TS workspace assets after `pnpm -r build`: expected {}, {}",
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
            fs::copy(&source_path, &target_path).expect("file copied");
        }
    }
}

/// 复刻 `scripts/pack.mjs` 的 payload 组装（§6.2）：核 + 全部切片 + templates +
/// agent，运行时依赖直接从仓库根 node_modules 复制（测试里不跑 npm install）。
pub(crate) fn stage_installed_bridge_package(root: &Path) -> PathBuf {
    ensure_node_bridge_dist();
    let target = root.join("lib/bridge");
    let node_modules = target.join("node_modules");
    fs::create_dir_all(&node_modules).expect("bridge dir created");

    let stage_package = |source_pkg: &Path, npm_name: &str| {
        let dest = node_modules.join(npm_name);
        fs::create_dir_all(&dest).expect("payload package dir created");
        fs::copy(source_pkg.join("package.json"), dest.join("package.json"))
            .expect("package.json copied");
        copy_dir_recursive(&source_pkg.join("dist"), &dest.join("dist"));
    };
    stage_package(&node_bridge_package_dir(), "@lania-cli-v2/bridge-runtime");
    let slices_root = repo_root().join("packages/foundation/bridge-slices");
    for entry in fs::read_dir(&slices_root).expect("slices dir readable") {
        let slice_dir = entry.expect("slice entry readable").path().join("ts");
        let name = slice_dir
            .parent()
            .expect("slice dir has a parent")
            .file_name()
            .expect("slice name")
            .to_string_lossy()
            .into_owned();
        stage_package(&slice_dir, &format!("@lania-cli-v2/bridge-slice-{name}"));
    }
    stage_package(&templates_package_dir(), "@lania-cli-v2/templates");
    stage_package(&agent_package_dir(), "@lania-cli-v2/agent");

    for dep in [
        "ejs",
        "tsx",
        "yaml",
        "@earendil-works/pi-agent-core",
        "@earendil-works/pi-ai",
    ] {
        let source = repo_root().join("node_modules").join(dep);
        if source.exists() {
            copy_dir_recursive(&source, &node_modules.join(dep));
        }
    }
    target
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
