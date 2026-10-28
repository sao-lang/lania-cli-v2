//! `runex` 的进程级冒烟测试：只覆盖不需要网络的路径。
//!
//! 所有用例都把 `RUNEX_HOME` 指向临时目录，保证不会碰用户真实的 `~/.runex`。
//! 需要下载的 `install` / `versions --available` 只断言 `--dry-run` 的计划文本。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_home(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("runex-cli-{name}-{unique}"));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn runex(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_runex"))
        .args(args)
        .env("RUNEX_HOME", home)
        // 隔离 HOME，避免探测到开发者本机的 ~/.runex 状态。
        .env("HOME", home)
        .env("USERPROFILE", home)
        .output()
        .expect("runex runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn help_lists_the_nvm_style_commands() {
    let home = temp_home("help");
    let output = runex(&home, &["--help"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    for command in [
        "install", "use", "remove", "list", "which", "init", "doctor",
    ] {
        assert!(
            text.contains(command),
            "missing `{command}` in help:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn list_installed_is_empty_on_a_clean_home() {
    let home = temp_home("list");
    let output = runex(&home, &["list", "--installed", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("json output");
    assert_eq!(value["installed"], serde_json::json!([]));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn init_prints_the_shims_snippet_for_the_requested_shell() {
    let home = temp_home("init");
    let output = runex(&home, &["init", "--shell", "bash"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("PATH"), "{text}");
    assert!(
        text.contains("shims") || text.contains("runex"),
        "snippet should mention the shims dir: {text}"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn which_fails_clearly_when_nothing_is_installed() {
    let home = temp_home("which");
    let output = runex(&home, &["which", "node"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.contains("no managed runtime") && message.contains("runex install"),
        "expected a runex-branded hint, got: {message}"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn install_dry_run_prints_the_plan_without_network() {
    let home = temp_home("dry-run");
    let output = runex(&home, &["install", "node", "22", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("would install node 22"), "{text}");
    assert!(text.contains("nodejs.org/dist"), "{text}");
    // dry-run 不能落盘
    assert!(!home.join("runtimes").exists());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn doctor_emits_structured_diagnostics() {
    let home = temp_home("doctor");
    let output = runex(&home, &["doctor", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("json output");
    assert!(value["diagnostics"].is_array());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn unknown_tool_is_rejected_with_the_supported_list() {
    let home = temp_home("unknown");
    let output = runex(&home, &["install", "cobol", "1", "--dry-run"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("unsupported tool"), "{message}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn home_flag_overrides_the_environment_variable() {
    let env_home = temp_home("env-home");
    let flag_home = temp_home("flag-home");
    let output = Command::new(env!("CARGO_BIN_EXE_runex"))
        .args(["init", "--shell", "bash", "--home"])
        .arg(&flag_home)
        .env("RUNEX_HOME", &env_home)
        .output()
        .expect("runex runs");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains(&flag_home.display().to_string().replace('\\', "/")),
        "--home should win over RUNEX_HOME: {text}"
    );
    let _ = std::fs::remove_dir_all(env_home);
    let _ = std::fs::remove_dir_all(flag_home);
}
