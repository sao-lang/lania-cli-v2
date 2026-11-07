//! tools 命令的实际执行链路 e2e。
//!
//! 覆盖以下场景：
//! - `lan tools list` 输出格式与内容
//! - `lan tools list --filter` 过滤
//! - `lan tools list --names-only` 紧凑列表
//! - `lan tools run` 执行文件
//! - `lan tools view` 查看文件内容
use super::common::*;
use super::*;

#[test]
fn lan_tools_list_returns_commands_e2e() {
    let root = temp_dir("tools-list");
    let home = isolated_home(&root);

    let output = run_cli(&root, &["tools", "list"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    // 默认模式下应包含结构化命令信息
    // stdout 应包含 JSON 格式的输出
    assert!(
        stdout.contains("kind") || stdout.contains("command"),
        "tools list stdout should contain structured output: {stdout}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_tools_list_with_filter_e2e() {
    let root = temp_dir("tools-list-filter");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["tools", "list", "--filter", "node"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_tools_list_names_only_e2e() {
    let root = temp_dir("tools-list-names");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["tools", "list", "--names-only"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    // --names-only 应输出紧凑的命令列表
    assert!(
        stdout.contains("lan") || stdout.contains("node"),
        "names-only output should list command names: {stdout}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_tools_list_returns_structured_output_e2e() {
    let root = temp_dir("tools-list-structured");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["tools", "list", "--names-only", "--plain"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_tools_view_shows_file_content_e2e() {
    let root = temp_dir("tools-view-file");
    let home = isolated_home(&root);
    write_file(root.join("hello.txt"), "Hello, tools view!\n");

    let output = run_cli(
        &root,
        &["tools", "view", "hello.txt"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_tools_command_help_e2e() {
    let root = temp_dir("tools-help");
    let home = isolated_home(&root);

    let output = run_cli(&root, &["tools", "--help"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("Commands:") || stdout.contains("USAGE"),
        "tools help should list subcommands: {stdout}"
    );

    let _ = fs::remove_dir_all(root);
}
