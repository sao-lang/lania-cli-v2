//! 模板查询与交互式选择类 e2e。
use super::common::*;
use super::*;

#[test]
fn lan_template_list_command_e2e() {
    let root = temp_dir("template-list");

    let output = run_cli(&root, &["template"], &[]);
    let json = parse_stdout_json(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "template_info");
    assert!(json["templates"]
        .as_array()
        .expect("templates")
        .iter()
        .any(|item| item == "toolkit"));
    assert_eq!(json["usage"]["detail"], "lan template <name>");
    assert!(json.get("host_state").is_none());
    assert!(json.get("context").is_none());
    assert!(stdout.contains("\n    \"kind\": \"template_info\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_template_detail_command_e2e() {
    let root = temp_dir("template-detail");

    let output = run_cli(&root, &["template", "toolkit"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "template_info");
    assert_eq!(json["template"], "toolkit");
    assert_eq!(json["metadata"]["name"], "toolkit");
    assert_eq!(json["metadata"]["renderEngine"], "node_bridge");
    assert!(json["availableTemplates"]
        .as_array()
        .expect("available templates")
        .iter()
        .any(|item| item == "toolkit"));
    assert!(json.get("host_state").is_none());
    assert!(json.get("context").is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_template_info_command_is_rejected_e2e() {
    let root = temp_dir("template-info-rejected");
    let home = isolated_home(&root);

    let output = run_cli(&root, &["template", "info"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("unknown command: `lan template info`"));
    assert!(stdout.contains("\"kind\": \"error\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_template_interactive_selects_template_e2e() {
    let root = temp_dir("template-interactive");
    let home = isolated_home(&root);

    let output = run_cli_interactive(
        &root,
        &["template"],
        &["\u{1b}[B"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("Choose a template"));
    assert!(stdout.contains("Template: spa-vue"));
    assert!(stdout.contains("Summary: A Vue 3 single-file component template"));
    assert!(stdout.contains("Highlights"));
    assert!(stdout.contains("Uses Vue 3 with a single-file component (`App.vue`) structure"));
    assert!(
        !stdout.contains("\"kind\":\"template_info\""),
        "interactive template info should not print raw json: {stdout}"
    );
    assert!(
        !stdout.contains("spa-react (create, add)"),
        "template list should not include technical use-case tags: {stdout}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_template_interactive_zh_locale_e2e() {
    let root = temp_dir("template-interactive-zh");
    write_file(
        root.join("lan.config.json"),
        r#"{
  "ui": {
    "locale": "zh"
  }
}
"#,
    );

    let output = run_cli_interactive(&root, &["template"], &[""], &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("选择模板"));
    assert!(stdout.contains("模板: spa-react"));
    assert!(stdout.contains("简介: 基于 React 的单页应用项目模板"));
    assert!(stdout.contains("关键特征"));
    assert!(stdout.contains("支持 Vite 或 Webpack 构建"));
    assert!(
        !stdout.contains("\"kind\":\"template_info\""),
        "interactive zh template info should not print raw json: {stdout}"
    );
    assert!(
        !stdout.contains("spa-react (创建项目, 添加内容)"),
        "template list should not include localized use-case tags: {stdout}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_template_info_human_zh_locale_e2e() {
    let root = temp_dir("template-info-human-zh");
    write_file(
        root.join("lan.config.json"),
        r#"{
  "ui": {
    "locale": "zh",
    "output": {
      "mode": "human"
    }
  }
}
"#,
    );

    let output = run_cli(&root, &["template", "toolkit"], &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("模板: toolkit"));
    assert!(stdout.contains("简介: 面向工具库或 SDK 的单包项目模板"));
    assert!(stdout.contains("关键特征"));
    assert!(stdout.contains("默认基于 Vite 构建"));

    let _ = fs::remove_dir_all(root);
}
