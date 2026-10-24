//! generate api/module 工作流 plan/diff 链路 e2e。
//!
//! 补充 P0-4：generate api plan/diff 完整链路。
//! 补充 P1-4：generate module plan/diff 完整链路。
//! 这些测试不依赖 Node bridge，全平台可运行。
use super::common::*;
use super::*;

#[test]
fn lan_generate_api_plan_e2e() {
    let root = temp_dir("generate-api-plan");
    write_contract_fixture(&root, &["grpc", "http"]);

    let output = run_cli(&root, &["generate", "api", "plan"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // plan 模式应输出 planned 状态且不写入文件
    assert_eq!(json["kind"], "result");
    assert_eq!(json["result"]["state"], "planned");
    assert_eq!(json["result"]["prompts"]["mode"], "plan");

    // plan 模式不应实际写入文件
    assert!(
        !root
            .join("contracts/generated/user_service.contract.gen.go")
            .exists(),
        "plan mode should not write files"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_api_diff_e2e() {
    let root = temp_dir("generate-api-diff");
    write_contract_fixture(&root, &["grpc"]);

    // 先 apply 写入基线
    let apply_output = run_cli(&root, &["generate", "api"], &[]);
    assert_eq!(apply_output.status.code(), Some(0));

    // 再 diff 应无差异（刚 apply 完）
    let diff_output = run_cli(&root, &["generate", "api", "diff"], &[]);
    let diff_json = parse_stdout_json(&diff_output);
    assert_eq!(
        diff_output.status.code(),
        Some(0),
        "diff after apply should succeed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&diff_output.stdout),
        String::from_utf8_lossy(&diff_output.stderr)
    );
    assert_eq!(diff_json["kind"], "result");
    assert_eq!(diff_json["result"]["state"], "planned");
    assert_eq!(diff_json["result"]["prompts"]["mode"], "diff");

    // 修改合约后 diff 应检测到差异
    write_contract_fixture(&root, &["http"]);
    let diff_after_change = run_cli(&root, &["generate", "api", "diff"], &[]);
    let diff_after_json = parse_stdout_json(&diff_after_change);
    assert_eq!(
        diff_after_change.status.code(),
        Some(0),
        "diff after contract change should succeed"
    );
    assert_eq!(diff_after_json["result"]["prompts"]["mode"], "diff");
    // 应该检测到目标变更 -> 预期产生 notes
    let notes = diff_after_json["result"]["notes"].as_array();
    assert!(
        notes.is_some() && !notes.unwrap().is_empty(),
        "diff after changing targets should produce notes"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_api_plan_shows_preview_before_apply_e2e() {
    let root = temp_dir("generate-api-plan-preview");
    write_contract_fixture(&root, &["grpc", "http"]);

    // plan 先预览
    let plan_output = run_cli(&root, &["generate", "api", "plan"], &[]);
    assert_eq!(plan_output.status.code(), Some(0));
    // 确认没有写入文件
    assert!(
        !root
            .join("contracts/generated/user_service.contract.gen.go")
            .exists(),
        "plan should not write output files"
    );

    // 然后 apply 实际写入
    let apply_output = run_cli(&root, &["generate", "api"], &[]);
    assert_eq!(apply_output.status.code(), Some(0));
    assert!(
        root.join("contracts/generated/user_service.contract.gen.go")
            .exists(),
        "apply should write output files"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_plan_e2e() {
    let root = temp_dir("generate-module-plan");
    write_module_fixture(&root, &["grpc", "http"], false);

    let output = run_cli(&root, &["generate", "module", "plan"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // plan 模式应输出 planned 状态
    assert_eq!(json["kind"], "result");
    assert_eq!(json["result"]["state"], "planned");
    assert_eq!(json["result"]["prompts"]["mode"], "plan");

    // plan 模式不应实际写入文件
    assert!(
        !root
            .join("generated/lania/contracts/user.contract.gen.go")
            .exists(),
        "plan mode should not write generated files"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_diff_e2e() {
    let root = temp_dir("generate-module-diff");
    write_module_fixture(&root, &["grpc"], false);

    // 先 apply 写入基线
    let apply_output = run_cli(&root, &["generate", "module"], &[]);
    assert_eq!(apply_output.status.code(), Some(0));

    // 再 diff 应无差异
    let diff_output = run_cli(&root, &["generate", "module", "diff"], &[]);
    let diff_json = parse_stdout_json(&diff_output);
    assert_eq!(
        diff_output.status.code(),
        Some(0),
        "diff after apply should succeed"
    );
    assert_eq!(diff_json["result"]["state"], "planned");
    assert_eq!(diff_json["result"]["prompts"]["mode"], "diff");

    // 修改目标后 diff 应检测到差异
    write_module_fixture(&root, &["http"], false);
    let diff_after_change = run_cli(&root, &["generate", "module", "diff"], &[]);
    let diff_after_json = parse_stdout_json(&diff_after_change);
    assert_eq!(
        diff_after_change.status.code(),
        Some(0),
        "diff after target change should succeed"
    );
    assert_eq!(diff_after_json["result"]["prompts"]["mode"], "diff");
    let notes = diff_after_json["result"]["notes"].as_array();
    assert!(
        notes.is_some() && !notes.unwrap().is_empty(),
        "diff after changing targets should produce notes"
    );

    let _ = fs::remove_dir_all(root);
}
