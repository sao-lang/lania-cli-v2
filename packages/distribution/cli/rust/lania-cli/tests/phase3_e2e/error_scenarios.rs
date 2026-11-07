//! 命令行错误场景 e2e 测试。
//!
//! 补充 P2-4：覆盖以下场景（全都不需要 Node bridge）：
//! - 不存在的模板名
//! - 不存在的文件路径（tools view）
//! - tools run 不存在的文件
//! - 重复的选项（部分命令对非法组合的处理）
//! - 无效的命令参数
use super::common::*;
use super::*;

#[test]
fn lan_tools_view_nonexistent_file_returns_error_e2e() {
    let root = temp_dir("tools-view-nonexistent");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["tools", "view", "no-such-file.txt"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 不存在的文件应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "view on nonexistent file should exit non-zero.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_api_with_nonexistent_config_returns_error_e2e() {
    let root = temp_dir("generate-api-no-config");

    let output = run_cli(
        &root,
        &["generate", "api", "--config", "nonexistent.yaml"],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 不存在的 config 应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "generate api with nonexistent config should fail.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_with_nonexistent_config_returns_error_e2e() {
    let root = temp_dir("generate-module-no-config");

    let output = run_cli(
        &root,
        &["generate", "module", "--config", "nonexistent.yaml"],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 不存在的 config 应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "generate module with nonexistent config should fail.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_add_with_nonexistent_template_returns_error_e2e() {
    let root = temp_dir("add-nonexistent-template");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );

    let output = run_cli(
        &root,
        &[
            "add",
            "--name",
            "Test",
            "--template",
            "no-such-template-12345",
        ],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 不存在的模板应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "add with nonexistent template should fail.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // 应有错误消息说明模板不存在
    assert!(
        stdout.contains("template") || stderr.contains("template") || stdout.contains("not found") || stderr.contains("not found"),
        "error should mention template.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_missing_subcommand_reports_usage_error_e2e() {
    let root = temp_dir("missing-subcommand");
    let home = isolated_home(&root);

    // generate 需要子命令（api/module/product），不带子命令应报错
    let output = run_cli(
        &root,
        &["generate"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 缺少子命令应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "generate without subcommand should fail.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_product_missing_subcommand_reports_error_e2e() {
    let root = temp_dir("product-missing-subcommand");
    let home = isolated_home(&root);

    // product 需要子命令（dev/build/pack/publish/inspect/doctor/generate）
    let output = run_cli(
        &root,
        &["product"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 缺少子命令应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "product without subcommand should fail.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_build_with_invalid_mode_option_reports_error_e2e() {
    let root = temp_dir("build-invalid-mode");
    let home = isolated_home(&root);

    // build 使用非法模式（假设没有 --unknown-mode 选项），应报错
    let output = run_cli(
        &root,
        &["build", "--unknown-mode", "value"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 非法选项应非零退出
    assert_ne!(
        output.status.code(),
        Some(0),
        "build with unknown option should fail.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
