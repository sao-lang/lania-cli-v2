//! create/add 工作流 e2e。
use super::common::*;
use super::*;

#[test]
fn lan_create_e2e() {
    let root = temp_dir("create");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );

    let output = run_cli(
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
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert!(root.join("demo-app/src/index.ts").exists());
    assert!(root.join("demo-app/package.json").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_create_with_template_options_e2e() {
    let root = temp_dir("create-vue");
    let output = run_cli(
        &root,
        &[
            "create",
            "--name",
            "vue-app",
            "--template",
            "spa-vue",
            "--package-manager",
            "bun",
        ],
        &[],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert!(root.join("vue-app/src/main.ts").exists());
    assert_eq!(json["execution"]["prompts"]["packageManager"], "bun");
    assert!(json["execution"]["notes"]
        .as_array()
        .expect("notes array")
        .iter()
        .any(|note| note.as_str().unwrap_or_default().contains("template")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_create_preview_e2e() {
    let root = temp_dir("create-preview");
    let output = run_cli(
        &root,
        &[
            "create",
            "--name",
            "preview-app",
            "--template",
            "toolkit",
            "--preview",
        ],
        &[],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["execution"]["state"], "planned");
    assert_eq!(json["execution"]["prompts"]["preview"], true);
    assert_eq!(json["execution"]["prompts"]["dryRun"], true);
    assert!(!root.join("preview-app").exists());
    assert!(json["execution"]["notes"]
        .as_array()
        .expect("notes array")
        .iter()
        .any(|note| note
            .as_str()
            .unwrap_or_default()
            .contains("template preview")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_create_interactive_e2e() {
    let root = temp_dir("create-interactive");

    let output = run_cli_interactive(
        &root,
        &[
            "create",
            "--name",
            "interactive-kit",
            "--template",
            "toolkit-monorepo",
        ],
        &["", "", "", ""],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(!stdout.contains("\"kind\":\"workflow\""));
    assert!(!stdout.contains("\"kind\": \"workflow\""));
    assert!(root.join("interactive-kit/pnpm-workspace.yaml").exists());
    assert!(root
        .join("interactive-kit/packages/core/src/index.ts")
        .exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_create_multiselect_interactive_e2e() {
    let root = temp_dir("create-multiselect-interactive");

    let output = run_cli_interactive(
        &root,
        &[
            "create",
            "--name",
            "interactive-kit",
            "--template",
            "toolkit-monorepo",
        ],
        // MultiSelect starts on the first default-selected item.
        // Press Space to toggle `eslint` off, then Enter to confirm.
        // The next Select prompt only has one option, so Enter accepts it.
        &[" ", "", "", ""],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(!stdout.contains("\"kind\":\"workflow\""));
    assert!(!stdout.contains("\"kind\": \"workflow\""));
    assert!(root.join("interactive-kit/pnpm-workspace.yaml").exists());
    assert!(root
        .join("interactive-kit/packages/core/src/index.ts")
        .exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_create_current_directory_interactive_e2e() {
    let root = temp_dir("create-current-dir-interactive");
    let expected_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temp dir name")
        .to_string();

    // dialoguer Select: start at default ("spa-react"), press Down then Enter to choose the next item ("spa-vue").
    // This ensures the selector is not the legacy "type a number" prompt.
    let output = run_cli_interactive(
        &root,
        &["create", ".", "--skip-install"],
        &[
            "\u{1b}[B",
            "",
            "",
            "",
            "",
            "\u{1b}[B\u{1b}[B",
            "",
            "https://example.com/demo.git",
        ],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(!stdout.contains("\"kind\":\"workflow\""));
    assert!(!stdout.contains("\"kind\": \"workflow\""));
    assert!(root.join("src/main.ts").exists());
    assert!(root.join("src/App.vue").exists());
    assert!(root.join("lan.config.js").exists());
    let package_json = read_json_file(root.join("package.json"));
    assert_eq!(package_json["name"], expected_name);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_create_current_directory_noninteractive_requires_template_e2e() {
    let root = temp_dir("create-current-dir-noninteractive");

    let output = run_cli(&root, &["create", "."], &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_ne!(output.status.code(), Some(0));
    assert!(!root.join("package.json").exists());
    assert!(
        stdout.contains("template")
            || stdout.contains("Template")
            || stderr.contains("template")
            || stderr.contains("Template")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_add_e2e() {
    let root = temp_dir("add");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite', language: 'TypeScript', cssProcessor: 'scss' };\n",
    );

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
        ],
        &[],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert!(root.join("src/components/Button.tsx").exists());
    assert!(json["execution"]["notes"]
        .as_array()
        .expect("notes array")
        .iter()
        .any(|note| note
            .as_str()
            .unwrap_or_default()
            .contains("dedicated add template render applied")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_add_interactive_e2e() {
    let root = temp_dir("add-interactive");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite', language: 'TypeScript', cssProcessor: 'scss' };\n",
    );

    let output = run_cli_interactive(
        &root,
        &["add", "--target", "src/components"],
        &["", "Button"],
        &[],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert_eq!(json["execution"]["prompts"]["template"], "rfc");
    assert_eq!(json["execution"]["prompts"]["name"], "Button");
    assert!(root.join("src/components/Button.tsx").exists());
    assert!(fs::read_to_string(root.join("src/components/Button.tsx"))
        .expect("generated component readable")
        .contains("const MyComponent"));

    let _ = fs::remove_dir_all(root);
}
