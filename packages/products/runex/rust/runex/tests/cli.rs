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
        // 测试断言英文文案，固定 locale，避免跟随 CI 的 LANG。
        .env("RUNEX_LOCALE", "en")
        .output()
        .expect("runex runs")
}

fn runex_without_locale_env(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_runex"))
        .args(args)
        .env("RUNEX_HOME", home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("RUNEX_LOCALE")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANGUAGE")
        .env("LANG", "en_US.UTF-8")
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
        "install", "use", "remove", "list", "which", "init", "doctor", "gc", "ui", "locale",
    ] {
        assert!(
            text.contains(command),
            "missing `{command}` in help:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn help_follows_the_locale() {
    let home = temp_home("help-zh");
    let output = runex(&home, &["--locale", "zh", "--help"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("不依赖 nvm"), "{text}");
    assert!(text.contains("用法："), "{text}");
    assert!(text.contains("命令:"), "{text}");
    assert!(text.contains("选项:"), "{text}");
    assert!(text.contains("打印帮助"), "{text}");
    assert!(text.contains("下载、校验并安装"), "{text}");
    assert!(!text.contains("Self-managed multi-runtime"), "{text}");

    let install = runex(&home, &["--locale", "zh", "install", "--help"]);
    assert!(install.status.success(), "{}", stderr(&install));
    let text = stdout(&install);
    assert!(text.contains("版本 / 前缀"), "{text}");
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
fn install_defaults_to_latest() {
    let home = temp_home("latest");
    let output = runex(&home, &["install", "node", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("would install node latest"), "{text}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn use_reads_the_project_version_file_when_the_version_is_omitted() {
    let home = temp_home("use-file");
    let project = home.join("project");
    std::fs::create_dir_all(&project).expect("project dir");
    std::fs::write(project.join(".nvmrc"), "22\n").expect("nvmrc");

    let output = Command::new(env!("CARGO_BIN_EXE_runex"))
        .args(["use", "node", "--dry-run"])
        .current_dir(&project)
        .env("RUNEX_HOME", &home)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUNEX_LOCALE", "en")
        .output()
        .expect("runex runs");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("would point node at 22"), "{text}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn use_without_version_or_project_file_fails_clearly() {
    let home = temp_home("use-none");
    let output = runex(&home, &["use", "node"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.contains("pass a version explicitly"),
        "expected an actionable hint, got: {message}"
    );
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
fn unknown_tool_error_follows_the_locale() {
    let home = temp_home("unknown-zh");
    let output = runex(
        &home,
        &["--locale", "zh", "install", "cobol", "1", "--dry-run"],
    );
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("不支持的工具"), "{message}");
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

#[test]
fn gc_dry_run_on_clean_home_reports_zero() {
    let home = temp_home("gc");
    let output = runex(&home, &["gc", "--all", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains('0'), "{text}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn locale_zh_localizes_cli_output() {
    let home = temp_home("locale-zh");
    let zh = runex(
        &home,
        &["--locale", "zh", "install", "node", "22", "--dry-run"],
    );
    assert!(zh.status.success(), "{}", stderr(&zh));
    assert!(stdout(&zh).contains("将安装 node 22"), "{}", stdout(&zh));

    let en = runex(
        &home,
        &["--locale", "en", "install", "node", "22", "--dry-run"],
    );
    assert!(en.status.success(), "{}", stderr(&en));
    assert!(
        stdout(&en).contains("would install node 22"),
        "{}",
        stdout(&en)
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn ui_snapshot_renders_one_frame_without_a_tty() {
    let home = temp_home("ui-snapshot");
    let output = runex(
        &home,
        &[
            "ui",
            "--snapshot",
            "--no-versions",
            "--cols",
            "80",
            "--rows",
            "24",
            "--locale",
            "en",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(text.lines().count(), 24, "{text}");
    assert!(!text.trim().is_empty(), "snapshot should not be blank");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn ui_snapshot_localizes_header_and_mode_chrome() {
    let home = temp_home("ui-zh");
    let output = runex(
        &home,
        &[
            "ui",
            "--snapshot",
            "--no-versions",
            "--cols",
            "80",
            "--rows",
            "20",
            "--locale",
            "zh",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("普通"), "{text}");
    assert!(text.contains("目录"), "{text}");
    assert!(text.contains("仓库"), "{text}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn ui_snapshot_emits_json_when_requested() {
    let home = temp_home("ui-json");
    let output = runex(
        &home,
        &[
            "ui",
            "--snapshot",
            "--no-versions",
            "--cols",
            "80",
            "--rows",
            "20",
            "--json",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("json output");
    assert_eq!(value["kind"], "runex_ui_snapshot");
    assert!(value["frame"]
        .as_str()
        .is_some_and(|frame| !frame.trim().is_empty()));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn doctor_human_output_follows_the_locale() {
    let home = temp_home("doctor-zh");
    let output = runex(&home, &["--locale", "zh", "doctor"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("尚不存在") || text.contains("一切正常"),
        "{text}"
    );
    assert!(!text.contains("does not exist yet"), "{text}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn locale_command_persists_the_choice_for_later_runs() {
    let home = temp_home("locale-set");

    let set = runex_without_locale_env(&home, &["locale", "zh"]);
    assert!(set.status.success(), "{}", stderr(&set));
    assert!(stdout(&set).contains("语言已设为 zh"), "{}", stdout(&set));
    assert!(
        home.join("config.json").is_file(),
        "config.json should exist"
    );

    // 未设置 RUNEX_LOCALE 的后续调用沿用保存的 zh。
    let show = runex_without_locale_env(&home, &["locale"]);
    assert!(show.status.success(), "{}", stderr(&show));
    assert!(stdout(&show).contains("语言：zh"), "{}", stdout(&show));

    let plan = runex_without_locale_env(&home, &["install", "node", "22", "--dry-run"]);
    assert!(plan.status.success(), "{}", stderr(&plan));
    assert!(
        stdout(&plan).contains("将安装 node 22"),
        "{}",
        stdout(&plan)
    );

    // auto 清除后回到环境 / 系统 locale。
    let auto = runex_without_locale_env(&home, &["locale", "auto"]);
    assert!(auto.status.success(), "{}", stderr(&auto));
    let plan = runex_without_locale_env(&home, &["install", "node", "22", "--dry-run"]);
    assert!(plan.status.success(), "{}", stderr(&plan));
    assert!(
        stdout(&plan).contains("would install node 22"),
        "{}",
        stdout(&plan)
    );

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn locale_command_rejects_unknown_values() {
    let home = temp_home("locale-bad");
    let output = runex_without_locale_env(&home, &["locale", "fr"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("unsupported locale"),
        "{}",
        stderr(&output)
    );
    assert!(!home.join("config.json").exists());
    let _ = std::fs::remove_dir_all(home);
}
