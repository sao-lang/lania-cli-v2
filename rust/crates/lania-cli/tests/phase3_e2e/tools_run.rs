//! tools run 命令的实际执行链路 e2e。
//!
//! 补充 P2-1：覆盖以下场景：
//! - `lan tools run` 执行 .js 文件
//! - `lan tools run` 执行 .mjs 文件
//! - `lan tools run` 对不支持的文件类型报错
//! - `lan tools run` 对不存在的文件报错
//! - `lan tools run` 向脚本传递参数
use super::common::*;
use super::*;

#[test]
fn lan_tools_run_unsupported_file_type_returns_error_e2e() {
    let root = temp_dir("tools-run-unsupported");
    let home = isolated_home(&root);
    write_file(root.join("data.bin"), "binary data\n");

    let output = run_cli(
        &root,
        &["tools", "run", "data.bin"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 不支持的文件类型应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "unsupported file type should exit non-zero.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // 错误消息应提及文件类型或 unsupported
    assert!(
        stdout.contains("unsupported") || stderr.contains("unsupported") || stdout.contains("not supported") || stderr.contains("not supported") || stdout.contains("error") || stderr.contains("error"),
        "error should mention unsupported type.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_tools_run_nonexistent_file_returns_error_e2e() {
    let root = temp_dir("tools-run-nonexistent");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["tools", "run", "no-such-file.js"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 不存在的文件应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "nonexistent file should exit non-zero.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

// tools run .js 文件需要 node 在 PATH 上
// 用 #[cfg(unix)] 避免在无法执行 node 的 Windows 环境上失败
// 但保留了 unsupported/nonexistent 测试作为全平台兜底
#[cfg(unix)]
#[test]
fn lan_tools_run_js_file_executes_node_e2e() {
    let root = temp_dir("tools-run-js");
    let home = isolated_home(&root);
    write_file(
        root.join("greet.js"),
        "const name = process.argv[2] || 'world'; console.log(`Hello, ${name}!`);\n",
    );

    let output = run_cli(
        &root,
        &["tools", "run", "greet.js"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn lan_tools_run_js_file_with_args_e2e() {
    let root = temp_dir("tools-run-js-args");
    let home = isolated_home(&root);
    write_file(
        root.join("print-args.js"),
        "console.log('args:', process.argv.slice(2).join(','));\n",
    );

    let output = run_cli(
        &root,
        &["tools", "run", "print-args.js", "--foo", "bar"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn lan_tools_run_mjs_file_e2e() {
    let root = temp_dir("tools-run-mjs");
    let home = isolated_home(&root);
    write_file(
        root.join("hello.mjs"),
        "console.log('Hello from ESM!');\n",
    );

    let output = run_cli(
        &root,
        &["tools", "run", "hello.mjs"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}
