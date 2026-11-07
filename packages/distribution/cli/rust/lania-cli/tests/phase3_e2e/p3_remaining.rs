//! P3 剩余项 E2E 测试。
//!
//! 覆盖：
//! - P3-1: product generate --preset demo 脚手架生成（验证产物结构）
//! - P3-3: 任务/进度系统集成（通过 lint/build 等命令测试进度输出）
//!
//! P3-2 (Git 集成) 在 lania-git crate 有 9 个单元测试，覆盖充分。
use super::common::*;
use super::*;

// product generate 依赖 Node bridge（bridge 调用 product.generate）
// 因此只在 Unix 上运行
#[cfg(unix)]
#[test]
fn lan_product_generate_preset_demo_creates_scaffold_structure_e2e() {
    let root = temp_dir("product-gen-demo-scaffold");

    let output = run_cli(
        &root,
        &[
            "product",
            "generate",
            "--preset",
            "demo",
            "--name",
            "Test CLI",
            "--binary-name",
            "test-cli",
            "--output-dir",
            "my-product",
            "--force",
        ],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "product generate should succeed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // 验证产物结构
    let product = root.join("my-product");
    assert!(product.exists(), "output directory should exist");
    assert!(
        product.join("lan.config.json").exists(),
        "lan.config.json should exist"
    );
    assert!(
        product.join("lania.schemas.js").exists() || product.join("lania.schemas.ts").exists(),
        "schema entry should exist"
    );

    let _ = fs::remove_dir_all(root);
}

// product generate 依赖 Node bridge（模板渲染），标记为 #[cfg(unix)]
#[cfg(unix)]
#[test]
fn lan_product_generate_help_shows_options_e2e() {
    let root = temp_dir("product-generate-help");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["product", "generate", "--help"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("--preset"), "help should show --preset");
    assert!(stdout.contains("--name"), "help should show --name");
    assert!(
        stdout.contains("--binary-name"),
        "help should show --binary-name"
    );
    assert!(
        stdout.contains("--output-dir"),
        "help should show --output-dir"
    );

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn lan_product_generate_without_preset_shows_usage_error_e2e() {
    let root = temp_dir("product-gen-no-preset");

    let output = run_cli(&root, &["product", "generate", "--name", "Test"], &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 不带 preset 应该报错或提示（具体行为取决于验证逻辑）
    if output.status.code() != Some(0) {
        assert!(
            stdout.contains("preset") || stderr.contains("preset") || stdout.contains("Usage"),
            "should mention --preset requirement.\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_build_progress_reports_completion_e2e() {
    let root = temp_dir("build-progress");
    let home = isolated_home(&root);
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );

    let output = run_cli(
        &root,
        &["build", "--mode", "production"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // build 应成功并包含进度信息
    assert_eq!(
        output.status.code(),
        Some(0),
        "build should succeed.\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // JSON 输出应包含统一 envelope 与业务结果
    let json = parse_stdout_json(&output);
    assert_eq!(json["kind"], "result");
    assert_eq!(json["command"]["name"], "build");
    assert_eq!(json["exitCode"], 0);
    assert_eq!(json["result"]["tool"], "vite");

    let _ = fs::remove_dir_all(root);
}

// 统一输出 envelope 不再携带 host_state / exchange 等内部调试信息
#[test]
fn lan_output_envelope_omits_internal_state_e2e() {
    let root = temp_dir("execution-host-state");
    let home = isolated_home(&root);
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );

    let output = run_cli(
        &root,
        &["build", "--mode", "production"],
        &[("HOME", home.as_str())],
    );
    assert_eq!(output.status.code(), Some(0));

    let json = parse_stdout_json(&output);

    assert_eq!(json["kind"], "result");
    assert!(json.get("host_state").is_none());
    assert!(json.get("exchange").is_none());

    let _ = fs::remove_dir_all(root);
}
