//! `env` 命令的 e2e：查询类命令与 dry-run 门控。
//!
//! 覆盖：
//! - `lan env list` 输出受支持工具
//! - `lan env list --installed` 只读 lania 自己的清单
//! - `lan env info <tool>` 结构
//! - `lan env doctor` 结构
//! - `lan env install` 默认 dry-run（不下载、不落盘）
//! - 非法版本号被拒绝（防注入 / 防路径穿越）
//! - 未实现自管的语言给出可操作错误
use super::common::*;
use super::*;

fn run_env(root: &Path, args: &[&str]) -> serde_json::Value {
    let home = isolated_home(root);
    let runex = root.join("runex");
    let output = run_cli(
        root,
        args,
        &[
            ("HOME", home.as_str()),
            ("RUNEX_HOME", runex.to_str().unwrap()),
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "args={args:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("env output is not JSON ({error}):\n{stdout}"));
    parsed
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("env output is missing result envelope:\n{stdout}"))
}

#[test]
fn lan_env_list_reports_supported_toolchains() {
    let root = temp_dir("env-list");
    let value = run_env(&root, &["env", "list"]);
    assert_eq!(value["kind"], "env_toolchains");
    let tools = value["tools"].as_array().expect("tools array");
    assert!(!tools.is_empty());

    let ids: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["id"].as_str())
        .collect();
    for expected in ["node", "python", "rust", "go", "deno", "bun", "zig"] {
        assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
    }
    // 第一批语言必须标记为自管，并带上上游渠道。
    for tool in tools {
        assert!(tool["installed"].is_boolean(), "{tool:?}");
        if matches!(
            tool["id"].as_str(),
            Some("node" | "python" | "rust" | "go" | "deno" | "bun" | "zig")
        ) {
            assert_eq!(tool["managed"], true, "{tool:?}");
            assert!(tool["upstream"].is_string(), "{tool:?}");
        }
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_list_installed_is_empty_on_clean_home() {
    let root = temp_dir("env-installed");
    let value = run_env(&root, &["env", "list", "--installed"]);
    assert_eq!(value["kind"], "env_installed");
    assert_eq!(value["installed"].as_array().expect("array").len(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_info_returns_details_for_node() {
    let root = temp_dir("env-info");
    let value = run_env(&root, &["env", "info", "node"]);
    assert_eq!(value["kind"], "env_toolchain_info");
    assert_eq!(value["tool"]["id"], "node");
    assert_eq!(value["tool"]["managed"], true);
    assert!(value["installedVersions"].is_array());
    // 上游能力描述应带出 provider 信息。
    assert_eq!(value["provider"]["id"], "node");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_doctor_returns_structured_report() {
    let root = temp_dir("env-doctor");
    let value = run_env(&root, &["env", "doctor"]);
    assert_eq!(value["kind"], "env_doctor");
    assert!(value["missing"].is_array());
    assert!(value["external"].is_array());
    assert!(value["managed"].is_array());
    assert!(value["diagnostics"].is_array());
    assert!(value["ok"].is_boolean());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_install_is_dry_run_by_default() {
    let root = temp_dir("env-install-dry");
    let value = run_env(&root, &["env", "install", "node", "20"]);
    assert_eq!(value["kind"], "env_plan");
    assert_eq!(value["status"], "dry_run");
    assert_eq!(value["dryRun"], true);
    assert_eq!(value["action"], "install");
    assert_eq!(value["tool"], "node");
    assert_eq!(value["managed"], true);
    let steps = value["steps"].as_array().expect("steps array");
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["kind"], "install");
    // dry-run 不允许留下任何执行痕迹（没有 executed 字段），也不允许落盘。
    assert!(value.get("executed").is_none());
    assert!(!root.join("runex").join("runtimes").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_rejects_shell_metacharacters_in_version() {
    let root = temp_dir("env-inject");
    let home = isolated_home(&root);
    let runex = root.join("runex");
    let output = run_cli(
        &root,
        &["env", "use", "node", "20;whoami"],
        &[
            ("HOME", home.as_str()),
            ("RUNEX_HOME", runex.to_str().unwrap()),
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(output.status.code(), Some(0), "stdout:\n{stdout}");
    assert!(
        stderr.contains("invalid version") || stdout.contains("invalid version"),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_rejects_path_traversal_in_version() {
    let root = temp_dir("env-traversal");
    let home = isolated_home(&root);
    let runex = root.join("runex");
    let output = run_cli(
        &root,
        &["env", "install", "node", "../../escape", "--yes"],
        &[
            ("HOME", home.as_str()),
            ("RUNEX_HOME", runex.to_str().unwrap()),
        ],
    );
    assert_ne!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid version") || stdout.contains("invalid version"),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_install_rejects_unmanaged_language() {
    let root = temp_dir("env-unmanaged");
    let home = isolated_home(&root);
    let runex = root.join("runex");
    let output = run_cli(
        &root,
        &["env", "install", "ruby", "3.3.0", "--yes"],
        &[
            ("HOME", home.as_str()),
            ("RUNEX_HOME", runex.to_str().unwrap()),
        ],
    );
    assert_ne!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not self-managed") || stdout.contains("not self-managed"),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // 不应因为错误而在磁盘上留下运行时目录。
    assert!(!runex.join("runtimes").join("ruby").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_versions_returns_list_shape() {
    let root = temp_dir("env-versions");
    let value = run_env(&root, &["env", "versions", "rust"]);
    assert_eq!(value["kind"], "env_versions");
    assert_eq!(value["tool"], "rust");
    assert_eq!(value["managed"], true);
    assert!(value["versions"].is_array());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_init_prints_shims_snippet() {
    let root = temp_dir("env-init");
    let value = run_env(&root, &["env", "init", "--shell", "bash"]);
    assert_eq!(value["kind"], "env_init");
    assert_eq!(value["shell"], "bash");
    let snippet = value["snippet"].as_str().expect("snippet");
    assert!(snippet.contains("shims"), "{snippet}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_which_fails_clearly_when_nothing_is_installed() {
    let root = temp_dir("env-which");
    let home = isolated_home(&root);
    let runex = root.join("runex");
    let output = run_cli(
        &root,
        &["env", "which", "node"],
        &[
            ("HOME", home.as_str()),
            ("RUNEX_HOME", runex.to_str().unwrap()),
        ],
    );
    assert_ne!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no managed runtime") || stdout.contains("no managed runtime"),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_env_ui_snapshot_renders_one_frame() {
    let root = temp_dir("env-ui-snapshot");
    let value = run_env(
        &root,
        &[
            "env",
            "ui",
            "--snapshot",
            "--no-versions",
            "--cols",
            "80",
            "--rows",
            "24",
        ],
    );
    assert_eq!(value["kind"], "env_ui_snapshot");
    let frame = value["frame"].as_str().expect("frame string");
    assert_eq!(frame.lines().count(), 24, "{frame}");
    assert!(!frame.trim().is_empty(), "snapshot should not be blank");
}

#[test]
fn lan_env_requires_tool_argument() {
    let root = temp_dir("env-missing-tool");
    let home = isolated_home(&root);
    let output = run_cli(&root, &["env", "info"], &[("HOME", home.as_str())]);
    assert_ne!(output.status.code(), Some(0));
    let _ = fs::remove_dir_all(root);
}
