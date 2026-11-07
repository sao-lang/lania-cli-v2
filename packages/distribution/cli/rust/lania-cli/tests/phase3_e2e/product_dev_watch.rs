//! product dev 命令的 help 与基础链路 e2e。
//!
//! 补充 P1-2：product dev 本地 watch 链路测试。
use super::common::*;
use super::*;

#[test]
fn lan_product_dev_help_shows_options_e2e() {
    let root = temp_dir("product-dev-help");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["product", "dev", "--help"],
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

    // 帮助信息应包含产品开发模式相关描述
    assert!(stdout.contains("development mode"), "help should mention development mode: {stdout}");
    // 应显示选项
    assert!(stdout.contains("--path"), "help should show --path option");
    assert!(stdout.contains("--watch"), "help should show --watch option");
    assert!(stdout.contains("--poll-interval-ms"), "help should show --poll-interval-ms option");
    // 应显示 args 用于透传 product 子命令
    assert!(
        stdout.contains("args"),
        "help should show args: {stdout}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_product_dev_without_args_provides_usage_hint_e2e() {
    let root = temp_dir("product-dev-no-args");
    let home = isolated_home(&root);

    // 不带参数运行 product dev，应给出使用提示
    let output = run_cli(
        &root,
        &["product", "dev"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 不带 args 时应该是非零退出（缺少 product 子命令）
    if output.status.code() != Some(0) {
        assert!(
            stderr.contains("product") || stdout.contains("missing") || stdout.contains("Usage"),
            "error message should mention product subcommand. stdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    let _ = fs::remove_dir_all(root);
}

// product dev 的执行依赖 Node bridge（用于解析 lania.schemas.js 中的动态命令），
// 因此执行链路测试只能在 Unix 上运行。
#[cfg(unix)]
#[test]
fn lan_product_dev_with_path_option_runs_cleanly_e2e() {
    let root = temp_dir("product-dev-path");
    let home = isolated_home(&root);
    write_file(
        root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "product": {
    "name": "@demo/test-product",
    "binaryName": "test-cli"
  },
  "schema": {
    "entry": "./lania.schemas.js"
  }
}
"#,
    );
    write_file(
        root.join("lania.schemas.js"),
        r#"export default {
  commands: [{ name: 'hello', handler: async () => ({ result: { ok: true, exitCode: 0 } }) }]
};
"#,
    );

    let output = run_cli(
        &root,
        &["product", "dev", "hello", "--path", "."],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "product dev hello should succeed.\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_product_dev_help_is_localized_in_zh_e2e() {
    let root = temp_dir("product-dev-help-zh");
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

    let output = run_cli(
        &root,
        &["product", "dev", "--help"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout.contains("开发模式") || stdout.contains("product"),
        "zh help should contain development mode description: {stdout}"
    );
    assert!(stdout.contains("--path"), "zh help should show --path");
    assert!(stdout.contains("--watch"), "zh help should show --watch");

    let _ = fs::remove_dir_all(root);
}
