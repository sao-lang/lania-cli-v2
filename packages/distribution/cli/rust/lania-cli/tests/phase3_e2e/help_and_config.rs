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

#[cfg(unix)]
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
fn lan_config_root_normalizes_legacy_stream_mode_without_project_ui_override_e2e() {
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
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "result");
    assert_eq!(json["command"]["name"], "config");
    assert_eq!(json["result"]["kind"], "config");
    assert_eq!(json["result"]["config"]["output"]["mode"], "json");

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
    assert_eq!(stdout.trim(), "json");
    let prefs = read_json_file(Path::new(&home).join(".lania").join("preferences.json"));
    assert_eq!(prefs["outputMode"], "json");

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

// ---- B13：`lan config init / doctor / set / sync`（项目 `lan.config.ts`）----

fn write_go_project(root: &Path) {
    write_file(root.join("go.mod"), "module demo\n\ngo 1.21\n");
    write_file(root.join("main.go"), "package main\n\nfunc main() {}\n");
}

#[test]
fn lan_config_init_creates_lan_config_ts_e2e() {
    let root = temp_dir("config-init");
    let home = isolated_home(&root);
    write_go_project(&root);

    let output = run_cli(&root, &["config", "init"], &[("HOME", home.as_str())]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["kind"], "config_init");
    assert_eq!(json["result"]["written"], true);
    assert_eq!(json["result"]["scope"], "project");
    // 新项目没有 `lania` 包 ⇒ 用普通对象模板，避免 config 一生成就加载失败。
    assert_eq!(json["result"]["mode"], "plain");

    let config = fs::read_to_string(root.join("lan.config.ts")).expect("lan.config.ts written");
    assert!(config.contains("export default {"), "{config}");
    assert!(config.contains("\"runtime\": \"go\""), "{config}");
    assert!(config.contains("\"lintTools\""), "{config}");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_init_refuses_to_overwrite_without_force_e2e() {
    let root = temp_dir("config-init-exists");
    let home = isolated_home(&root);
    write_go_project(&root);
    write_file(
        root.join("lan.config.js"),
        "module.exports = { runtime: 'go' };\n",
    );

    let output = run_cli(&root, &["config", "init"], &[("HOME", home.as_str())]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["written"], false);
    assert_eq!(json["result"]["reason"], "exists");
    assert!(json["result"]["suggestion"]
        .as_str()
        .unwrap_or_default()
        .contains("export default"));
    assert!(!root.join("lan.config.ts").exists(), "must not write");
    assert_eq!(
        fs::read_to_string(root.join("lan.config.js")).expect("old config kept"),
        "module.exports = { runtime: 'go' };\n"
    );

    // `--force` 会写 `.ts`，但不会删掉旧的 `.js`（用户自己决定留哪个）。
    let output = run_cli(
        &root,
        &["config", "init", "--force"],
        &[("HOME", home.as_str())],
    );
    let json = parse_stdout_json(&output);
    assert_eq!(json["result"]["written"], true, "{json}");
    assert!(root.join("lan.config.ts").exists());
    assert!(root.join("lan.config.js").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_set_project_key_writes_pure_data_e2e() {
    let root = temp_dir("config-set-project");
    let home = isolated_home(&root);
    write_go_project(&root);
    write_file(
        root.join("lan.config.ts"),
        "export default { lintTools: [] };\n",
    );

    let output = run_cli(
        &root,
        &["config", "set", "runtime", "go"],
        &[("HOME", home.as_str())],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["kind"], "config_set");
    assert_eq!(json["result"]["written"], true);
    assert_eq!(json["result"]["scope"], "project");

    let config = fs::read_to_string(root.join("lan.config.ts")).expect("config rewritten");
    assert!(config.contains("\"runtime\": \"go\""), "{config}");
    assert!(
        config.contains("lintTools"),
        "existing keys must be kept: {config}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_set_refuses_function_configs_e2e() {
    let root = temp_dir("config-set-fn");
    let home = isolated_home(&root);
    write_go_project(&root);
    let original = "export default () => ({ runtime: 'node' });\n";
    write_file(root.join("lan.config.ts"), original);

    let output = run_cli(
        &root,
        &["config", "set", "runtime", "go"],
        &[("HOME", home.as_str())],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["written"], false);
    assert_eq!(json["result"]["pureData"], false);
    assert!(json["result"]["suggestion"]
        .as_str()
        .unwrap_or_default()
        .contains("\"runtime\": \"go\""));
    // 硬约束：函数配置一个字都不能改。
    assert_eq!(
        fs::read_to_string(root.join("lan.config.ts")).expect("config kept"),
        original
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_doctor_reports_runtime_mismatch_e2e() {
    let root = temp_dir("config-doctor");
    let home = isolated_home(&root);
    write_go_project(&root);
    write_file(
        root.join("lan.config.ts"),
        "export default { runtime: 'python', lintTools: ['mypy'] };\n",
    );

    let output = run_cli(&root, &["config", "doctor"], &[("HOME", home.as_str())]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["kind"], "config_doctor");
    assert_eq!(json["result"]["exists"], true);
    assert_eq!(json["result"]["runtime"]["declared"], "python");
    assert_eq!(json["result"]["runtime"]["detected"], "go");
    assert_eq!(json["result"]["runtime"]["effective"], "python");
    assert_eq!(json["result"]["runtime"]["source"], "config");
    let codes: Vec<&str> = json["result"]["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .filter_map(|issue| issue["code"].as_str())
        .collect();
    assert!(codes.contains(&"runtime_mismatch"), "{codes:?}");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_config_sync_suggests_then_writes_when_asked_e2e() {
    let root = temp_dir("config-sync");
    let home = isolated_home(&root);
    write_file(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write_file(root.join("src/main.rs"), "fn main() {}\n");

    // 默认只给建议，不写盘。
    let output = run_cli(&root, &["config", "sync"], &[("HOME", home.as_str())]);
    let json = parse_stdout_json(&output);
    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["kind"], "config_sync");
    assert_eq!(json["result"]["written"], false);
    assert_eq!(json["result"]["writable"], true);
    let keys: Vec<&str> = json["result"]["suggestions"]
        .as_array()
        .expect("suggestions array")
        .iter()
        .filter_map(|suggestion| suggestion["key"].as_str())
        .collect();
    assert_eq!(keys, vec!["runtime", "lintTools"]);
    assert!(
        !root.join("lan.config.ts").exists(),
        "sync must not write by default"
    );

    // `--write` 才落盘（纯数据配置）。
    let output = run_cli(
        &root,
        &["config", "sync", "--write"],
        &[("HOME", home.as_str())],
    );
    let json = parse_stdout_json(&output);
    assert_eq!(json["result"]["written"], true, "{json}");
    let config = fs::read_to_string(root.join("lan.config.ts")).expect("config written");
    assert!(config.contains("\"runtime\": \"rust\""), "{config}");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn broken_lan_config_ts_surfaces_a_warning_e2e() {
    // §6.5.9 验收：`.ts` 配置加载失败（语法错误）时不能静默吞掉 ——
    // `validation_errors` 里要有条目，CLI 以 warning 显示，但**不阻断**命令。
    let root = temp_dir("config-broken");
    let home = isolated_home(&root);
    write_go_project(&root);
    // 故意少一个右花括号。
    write_file(
        root.join("lan.config.ts"),
        "export default {\n  runtime: 'go',\n",
    );

    let output = run_cli(&root, &["config", "doctor"], &[("HOME", home.as_str())]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(0), "must not block: {stderr}");
    assert!(
        stderr.contains("lan.config"),
        "the load failure must be visible as a warning: {stderr}"
    );

    let json = parse_stdout_json(&output);
    let errors = json["result"]["validationErrors"]
        .as_array()
        .expect("validationErrors array");
    assert!(
        !errors.is_empty(),
        "load failure must be recorded in validation_errors: {json}"
    );

    let _ = fs::remove_dir_all(root);
}
