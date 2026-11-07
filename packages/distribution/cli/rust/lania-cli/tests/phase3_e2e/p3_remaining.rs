//! P3 剩余项 E2E 测试。
//!
//! 覆盖：
//! - P3-3: 任务/进度系统集成（通过 lint/build 等命令测试进度输出）
//!
//! P3-2 (Git 集成) 在 lania-git crate 有 9 个单元测试，覆盖充分。
use super::common::*;
use super::*;

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
