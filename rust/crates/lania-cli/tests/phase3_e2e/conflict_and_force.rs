//! 文件冲突与 force 覆盖场景 e2e。
//!
//! 补充 P2-3：
//! - `add` 冲突检测（无 force 时检测 existing 文件且报告冲突路径）
//! - `add --force` 覆盖已有文件
//! - `generate api --force` 覆盖已有生成文件
//!
//! create 和 generate module 依赖 Node bridge（模板渲染），标记为 #[cfg(unix)]。
use super::common::*;
use super::*;

#[test]
fn lan_add_without_force_reports_conflicts_in_conflicts_field_e2e() {
    let root = temp_dir("add-without-force");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite', language: 'TypeScript', cssProcessor: 'scss' };\n",
    );

    // 先添加一个组件
    let first = run_cli(
        &root,
        &[
            "add",
            "--name",
            "Button",
            "--template",
            "rfc",
            "--target",
            "src/components",
        ],
        &[],
    );
    assert_eq!(first.status.code(), Some(0));
    assert!(root.join("src/components/Button.tsx").exists());

    // 不指定 --force 再次添加同一组件
    let second = run_cli(
        &root,
        &[
            "add",
            "--name",
            "Button",
            "--template",
            "rfc",
            "--target",
            "src/components",
        ],
        &[],
    );
    let stdout = String::from_utf8_lossy(&second.stdout);
    // 即使不指定 --force，add 也可能成功（exit 0），但应该在 conflicts 字段报告冲突路径
    let json = parse_stdout_json(&second);
    let conflicts = json["execution"]["conflicts"].as_array();
    assert!(
        conflicts.is_some() && !conflicts.unwrap().is_empty(),
        "re-adding without --force should populate conflicts field: {stdout}"
    );
    // 冲突路径应包含 Button.tsx
    let has_button_conflict = conflicts.unwrap().iter().any(|c| {
        c.as_str().unwrap_or("").contains("Button")
    });
    assert!(has_button_conflict, "conflicts should mention Button.tsx: {stdout}");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_add_with_force_overwrites_existing_file_e2e() {
    let root = temp_dir("add-force-overwrite");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite', language: 'TypeScript', cssProcessor: 'scss' };\n",
    );

    // 先创建一个初始文件
    fs::create_dir_all(root.join("src/components")).expect("dir created");
    write_file(
        root.join("src/components/Button.tsx"),
        "// old content\n",
    );

    // 使用 --force 添加同名文件
    let output = run_cli(
        &root,
        &[
            "add",
            "--name",
            "Button",
            "--template",
            "rfc",
            "--target",
            "src/components",
            "--force",
        ],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        output.status.code(),
        Some(0),
        "add --force should succeed: {stdout}",
    );

    // 文件应被覆盖（不再包含 old content）
    let content = fs::read_to_string(root.join("src/components/Button.tsx"))
        .expect("file readable");
    assert!(
        !content.contains("old content"),
        "file should be overwritten with new content: {content}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_add_with_force_overwrites_duplicate_without_crashing_e2e() {
    let root = temp_dir("add-force-duplicate");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite', language: 'TypeScript', cssProcessor: 'scss' };\n",
    );

    // 先用 --force 添加
    let first = run_cli(
        &root,
        &[
            "add",
            "--name",
            "Button",
            "--template",
            "rfc",
            "--target",
            "src/components",
            "--force",
        ],
        &[],
    );
    assert_eq!(
        first.status.code(),
        Some(0),
        "first add --force should succeed"
    );

    // 再次用 --force 添加同一文件不应崩溃
    let second = run_cli(
        &root,
        &[
            "add",
            "--name",
            "Button",
            "--template",
            "rfc",
            "--target",
            "src/components",
            "--force",
        ],
        &[],
    );
    assert_eq!(
        second.status.code(),
        Some(0),
        "second add --force on same file should not crash"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_api_with_force_overwrites_existing_file_e2e() {
    let root = temp_dir("generate-api-force");
    write_contract_fixture(&root, &["grpc", "http"]);

    // 首次 apply
    let first = run_cli(&root, &["generate", "api"], &[]);
    assert_eq!(first.status.code(), Some(0));
    assert!(root.join("contracts/generated/user_service.contract.gen.go").exists());

    // 修改生成的文件
    let contract_path = root.join("contracts/generated/user_service.contract.gen.go");
    fs::write(&contract_path, "// manual edit\n").expect("write custom content");

    // 带 --force 再次 apply 应覆盖
    let second = run_cli(&root, &["generate", "api", "--force"], &[]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "generate api --force should succeed"
    );

    let content = fs::read_to_string(&contract_path).expect("file readable");
    assert!(
        !content.contains("manual edit"),
        "force should overwrite custom edits: {content}"
    );

    let _ = fs::remove_dir_all(root);
}

// create 和 generate module 依赖 Node bridge，只在 Unix 上运行
#[cfg(unix)]
#[test]
fn lan_create_on_existing_dir_reports_conflict_e2e() {
    let root = temp_dir("create-existing-dir");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );

    let first = run_cli(
        &root,
        &[
            "create",
            "--name",
            "demo-app",
            "--template",
            "toolkit",
            "--package-manager",
            "pnpm",
        ],
        &[],
    );
    assert_eq!(first.status.code(), Some(0));
    assert!(root.join("demo-app/src/index.ts").exists());

    let second = run_cli(
        &root,
        &[
            "create",
            "--name",
            "demo-app",
            "--template",
            "toolkit",
            "--package-manager",
            "pnpm",
        ],
        &[],
    );
    let stdout = String::from_utf8_lossy(&second.stdout);

    if second.status.code() == Some(0) {
        let json = parse_stdout_json(&second);
        let notes = json["execution"]["notes"].as_array();
        assert!(
            notes.is_some() && notes.unwrap().iter().any(|note| {
                let text = note.as_str().unwrap_or("");
                text.contains("conflict") || text.contains("exist") || text.contains("skip") || text.contains("overwrite")
            }),
            "re-running create should report conflicts in notes: {stdout}"
        );
    } else {
        assert!(
            stdout.contains("conflict") || stdout.contains("exist") || stdout.contains("already") || stdout.contains("error"),
            "conflict should be reported: {stdout}"
        );
    }

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn lan_generate_module_with_force_overwrites_existing_file_e2e() {
    let root = temp_dir("generate-module-force");
    write_module_fixture(&root, &["grpc"], false);

    let first = run_cli(&root, &["generate", "module"], &[]);
    assert_eq!(first.status.code(), Some(0));
    assert!(root.join("generated/lania/contracts/user.contract.gen.go").exists());

    let contract_path = root.join("generated/lania/contracts/user.contract.gen.go");
    fs::write(&contract_path, "// custom manual edit\n").expect("write custom content");

    let second = run_cli(&root, &["generate", "module", "--force"], &[]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "generate module --force should succeed"
    );

    let content = fs::read_to_string(&contract_path).expect("file readable");
    assert!(
        !content.contains("custom manual edit"),
        "force should overwrite custom edits: {content}"
    );

    let _ = fs::remove_dir_all(root);
}
