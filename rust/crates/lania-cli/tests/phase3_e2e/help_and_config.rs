//! 帮助与全局配置类 e2e。
use super::common::*;
use super::*;

#[test]
fn lan_help_command_e2e() {
    let root = temp_dir("help");
    let home = isolated_home(&root);

    let output = run_cli(&root, &["help", "build"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("Run the project build workflow"));
    assert!(stdout.contains("Aliases: b"));
    assert!(stdout.contains("Examples:"));
    assert!(stdout.contains("lan build --watch --mode development"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_without_args_shows_help_e2e() {
    let root = temp_dir("help-default");
    let home = isolated_home(&root);

    let output = run_cli(&root, &[], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("Lania CLI v2"));
    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("build"));
    assert!(stdout.contains("lint"));
    assert!(stdout.contains("Options:"));
    assert!(stdout.contains("Print help"));
    assert!(!stdout.contains("\"capabilities\""));
    assert!(stderr.trim().is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_root_help_is_localized_in_zh_e2e() {
    let root = temp_dir("help-root-zh");
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

    let output = run_cli(&root, &[], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("用法: lan [COMMAND]"));
    assert!(stdout.contains("命令:"));
    assert!(stdout.contains("选项:"));
    assert!(stdout.contains("显示版本"));
    assert!(stdout.contains("显示帮助"));
    assert!(!stdout.contains("Options:"));
    assert!(!stdout.contains("Print version"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_version_command_e2e() {
    let root = temp_dir("version");

    let output = run_cli(&root, &["version"], &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout.trim(), env!("CARGO_PKG_VERSION"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_unknown_command_returns_builtin_error_e2e() {
    let root = temp_dir("unknown-command");
    let home = isolated_home(&root);

    let output = run_cli(&root, &["app"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("unknown command: `lan app`"));
    assert!(stdout.contains("\"kind\": \"error\""));
    assert!(stdout.contains("\"message\": \"unknown command: `lan app`\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_help_is_not_treated_as_error_e2e() {
    let root = temp_dir("config-help");
    let home = isolated_home(&root);

    let output = run_cli(&root, &["config", "--help"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("Manage global CLI preferences"));
    assert!(stdout.contains("Usage: config [COMMAND]"));
    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("Options:"));
    assert!(stdout.contains("Print help"));
    assert!(!stdout.contains("\"kind\": \"error\""));
    assert!(stderr.trim().is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_help_is_localized_in_zh_e2e() {
    let root = temp_dir("config-help-zh");
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

    let output = run_cli(&root, &["config", "--help"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("管理全局 CLI 配置"));
    assert!(stdout.contains("用法: config [COMMAND]"));
    assert!(stdout.contains("命令:"));
    assert!(stdout.contains("选项:"));
    assert!(stdout.contains("显示帮助"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_tools_help_is_localized_in_zh_e2e() {
    let root = temp_dir("tools-help-zh");
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

    let output = run_cli(&root, &["tools", "--help"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("列出命令、按类型运行文件，或查看本地文件"));
    assert!(stdout.contains("用法: tools [OPTIONS] [COMMAND]"));
    assert!(stdout.contains("列出终端可解析的命令"));
    assert!(stdout.contains("按检测到的运行时执行代码文件"));
    assert!(stdout.contains("显示文件内容或使用系统应用打开媒体文件"));
    assert!(stdout.contains("按子串过滤命令名"));
    assert!(stdout.contains("以纯文本列表渲染，而不是结构化命令条目"));
    assert!(stdout.contains("显示帮助"));
    assert!(!stdout.contains("List terminal-resolvable commands"));
    assert!(!stdout.contains("Filter command names by substring"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_release_help_is_localized_in_zh_e2e() {
    let root = temp_dir("release-help-zh");
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

    let output = run_cli(&root, &["release", "--help"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("发布工作流（规划、执行、恢复）"));
    assert!(stdout.contains("生成发布计划并持久化发布状态"));
    assert!(stdout.contains("执行发布计划并持久化状态"));
    assert!(stdout.contains("恢复一次失败或未完成的发布流程"));
    assert!(stdout.contains("发布版本号"));
    assert!(stdout.contains("目标发布环境"));
    assert!(stdout.contains("显示帮助"));
    assert!(!stdout.contains("Generate a release plan"));
    assert!(!stdout.contains("Release version"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_help_is_localized_in_zh_e2e() {
    let root = temp_dir("generate-module-help-zh");
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
        &["generate", "module", "--help"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("生成 lania-g 模块与 main.go 注入产物"));
    assert!(stdout.contains("预览生成的模块文件、注入变更与清理动作"));
    assert!(stdout.contains("对比当前模块 manifest 与计划输出"));
    assert!(stdout.contains("模块配置文件路径"));
    assert!(stdout.contains("跳过 main.go 注入和 helper 生成"));
    assert!(!stdout.contains("Generate lania-g modules"));
    assert!(!stdout.contains("Preview generated module files"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_product_help_is_localized_in_zh_e2e() {
    let root = temp_dir("product-help-zh");
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

    let output = run_cli(&root, &["product", "--help"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("面向 product 的 CLI 工作流与分发命令"));
    assert!(stdout.contains("以开发模式运行本地 product"));
    assert!(stdout.contains("构建一个用于打包的最小 product 快照"));
    assert!(stdout.contains("运行 product doctor 诊断，包括兼容性检查"));
    assert!(stdout.contains("生成一个带脚手架的 CLI product 工作区"));
    assert!(!stdout.contains("Run a local product in development mode"));
    assert!(!stdout.contains("Generate a scaffolded CLI product workspace"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_get_locale_returns_raw_value_e2e() {
    let root = temp_dir("config-get-locale");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["config", "get", "locale"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(matches!(stdout.trim(), "en" | "zh"));
    assert!(!stdout.contains("\"kind\": \"config_value\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_get_locale_human_mode_e2e() {
    let root = temp_dir("config-get-locale-human");
    let home = isolated_home(&root);
    write_file(
        root.join("lan.config.json"),
        r#"{
  "ui": {
    "output": {
      "mode": "human"
    }
  }
}
"#,
    );

    let output = run_cli(
        &root,
        &["config", "get", "locale"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(matches!(stdout.trim(), "en" | "zh"));
    assert!(!stdout.contains("\"kind\": \"config_value\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_get_root_human_mode_e2e() {
    let root = temp_dir("config-root-human");
    let home = isolated_home(&root);
    write_file(
        root.join("lan.config.json"),
        r#"{
  "ui": {
    "output": {
      "mode": "human"
    }
  }
}
"#,
    );

    let output = run_cli(&root, &["config"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("Global CLI Config"));
    assert!(stdout.contains("\"locale\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_root_uses_global_stream_mode_without_project_ui_override_e2e() {
    let root = temp_dir("config-root-stream-global");
    let home = isolated_home(&root);
    write_file(
        Path::new(&home).join(".lania").join("preferences.json"),
        r#"{
  "locale": "zh",
  "outputMode": "stream",
  "logTimestamps": false
}
"#,
    );

    let output = run_cli(&root, &["config"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    let json: serde_json::Value = serde_json::from_str(line).expect("config stream output jsonl");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "result");
    assert_eq!(json["payload"]["kind"], "config");
    assert_eq!(json["payload"]["config"]["output"]["mode"], "stream");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_root_uses_global_human_mode_without_project_ui_override_e2e() {
    let root = temp_dir("config-root-human-global");
    let home = isolated_home(&root);
    write_file(
        Path::new(&home).join(".lania").join("preferences.json"),
        r#"{
  "locale": "en",
  "outputMode": "human",
  "logTimestamps": false
}
"#,
    );

    let output = run_cli(&root, &["config"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("Global CLI Config"));
    assert!(stdout.contains("\"output\":"));
    assert!(stdout.contains("\"mode\": \"human\""));
    assert!(!stdout.contains("\"kind\": \"config\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_set_output_mode_returns_raw_value_e2e() {
    let root = temp_dir("config-set-output-mode");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["config", "set", "output.mode", "stream"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout.trim(), "stream");
    let prefs = read_json_file(Path::new(&home).join(".lania").join("preferences.json"));
    assert_eq!(prefs["outputMode"], "stream");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_set_output_mode_human_returns_raw_value_e2e() {
    let root = temp_dir("config-set-output-mode-human");
    let home = isolated_home(&root);

    let output = run_cli(
        &root,
        &["config", "set", "output.mode", "human"],
        &[("HOME", home.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout.trim(), "human");
    let prefs = read_json_file(Path::new(&home).join(".lania").join("preferences.json"));
    assert_eq!(prefs["outputMode"], "human");

    let _ = fs::remove_dir_all(root);
}
