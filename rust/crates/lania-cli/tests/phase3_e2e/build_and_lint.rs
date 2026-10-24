//! dev/build/lint 编译链路 e2e。
use super::common::*;
use super::*;

#[test]
fn lan_dev_with_vite_e2e() {
    let root = temp_dir("dev-vite");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );

    let output = run_cli(
        &root,
        &["dev", "--port", "3101"],
        &[("LANIA_INTERRUPT_AFTER_MS", "20")],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(130));
    assert_eq!(json["kind"], "result");
    assert_eq!(json["command"]["name"], "dev");
    assert_eq!(json["exitCode"], 130);
    assert!(json["cwd"].is_string());
    assert_eq!(json["result"]["tool"], "vite");
    assert_eq!(json["result"]["port"], 3101);
    assert_eq!(json["result"]["workerMode"], "isolated_worker");
    assert_eq!(json["result"]["eventSchema"], "lania.compiler.events.v1");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_dev_with_vite_version_mismatch_warns() {
    let root = temp_dir("dev-vite-mismatch");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );
    write_file(
        root.join("package.json"),
        "{\"name\":\"demo\",\"devDependencies\":{\"vite\":\"^99.0.0\"}}\n",
    );
    write_fake_package(
        &root,
        "vite",
        "module.exports = { version: '1.0.0', createServer: async () => ({ listen: async () => {}, resolvedUrls: { local: ['http://127.0.0.1:3000/'] }, close: async () => {} }) };\n",
    );

    let output = run_cli(
        &root,
        &["dev", "--port", "3300"],
        &[("LANIA_INTERRUPT_AFTER_MS", "20")],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(130));
    assert_eq!(json["kind"], "result");
    assert_eq!(json["exitCode"], 130);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_dev_with_webpack_e2e() {
    let root = temp_dir("dev-webpack");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'webpack' };\n",
    );

    let output = run_cli(
        &root,
        &["dev", "--port", "3201"],
        &[("LANIA_INTERRUPT_AFTER_MS", "20")],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(130));
    assert_eq!(json["result"]["tool"], "webpack");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_build_with_vite_e2e() {
    let root = temp_dir("build-vite");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );

    let output = run_cli(&root, &["build", "--mode", "production"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["result"]["tool"], "vite");
    assert_eq!(json["result"]["mode"], "production");
    assert_eq!(json["result"]["workerMode"], "isolated_worker");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_build_with_webpack_e2e() {
    let root = temp_dir("build-webpack");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'webpack' };\n",
    );

    let output = run_cli(
        &root,
        &["build", "--watch"],
        &[("LANIA_INTERRUPT_AFTER_MS", "20")],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(130));
    assert_eq!(json["result"]["tool"], "webpack");
    assert_eq!(json["result"]["watch"], true);
    assert_eq!(json["result"]["workerMode"], "isolated_worker");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_dev_with_rollup_e2e() {
    let root = temp_dir("dev-rollup");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'rollup' };\n",
    );
    write_fake_package(
        &root,
        "rollup",
        "const listeners = new Map();\n\
function on(name, fn) { const arr = listeners.get(name) ?? []; arr.push(fn); listeners.set(name, arr); }\n\
function off(name, fn) { const arr = listeners.get(name) ?? []; listeners.set(name, arr.filter((f) => f !== fn)); }\n\
function emit(name, ev) { for (const fn of (listeners.get(name) ?? [])) fn(ev); }\n\
module.exports = {\n\
  version: '1.0.0',\n\
  watch() {\n\
    return {\n\
      on(name, fn) { on(name, fn); setTimeout(() => { emit('event', { code: 'START' }); emit('event', { code: 'BUNDLE_END' }); emit('event', { code: 'END' }); }, 10); },\n\
      off,\n\
      close() {}\n\
    };\n\
  },\n\
  rollup: async () => ({ write: async () => ({ output: [{ fileName: 'index.js', code: 'console.log(1);' }] }) })\n\
};\n",
    );

    let output = run_cli(
        &root,
        &["dev", "--port", "3301"],
        &[("LANIA_INTERRUPT_AFTER_MS", "20")],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(130));
    assert_eq!(json["result"]["tool"], "rollup");
    assert_eq!(json["result"]["longRunning"], true);
    assert_eq!(json["result"]["workerMode"], "isolated_worker");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_build_with_rollup_e2e() {
    let root = temp_dir("build-rollup");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'rollup' };\n",
    );
    write_fake_package(
        &root,
        "rollup",
        "module.exports = {\n\
  version: '1.0.0',\n\
  rollup: async () => ({\n\
    write: async () => ({ output: [{ fileName: 'index.js', code: 'console.log(1);' }] })\n\
  })\n\
};\n",
    );

    let output = run_cli(&root, &["build", "--mode", "production"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["result"]["tool"], "rollup");
    assert_eq!(json["result"]["workerMode"], "isolated_worker");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_lint_fix_e2e() {
    let root = temp_dir("lint-fix");
    write_file(
        root.join("lan.config.js"),
        "export default { lintTools: ['eslint', 'prettier'] };\n",
    );
    write_file(root.join("src/index.js"), "const answer = 42\n");

    let output = run_cli(&root, &["lint", "fix"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["result"]["mode"], "fix");
    assert_eq!(json["result"]["fix"], true);
    assert!(json["result"]["summaryText"]
        .as_str()
        .unwrap_or_default()
        .contains("lint fix"));
    assert!(json["result"]["resultsByAdaptor"]["eslint"].is_object());
    assert!(json["result"]["resultsByAdaptor"]["prettier"].is_object());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_lint_with_stylelint_and_textlint_e2e() {
    let root = temp_dir("lint-style-text");
    write_file(
        root.join("lan.config.js"),
        "export default { lintTools: ['stylelint', 'textlint'] };\n",
    );
    write_fake_package(
        &root,
        "stylelint",
        "module.exports = { lint: async () => ({ results: [{ source: 'src/app.css', warnings: [{ severity: 'warning' }] }] }) };\n",
    );
    write_fake_package(
        &root,
        "textlint",
        "module.exports = { TextLintEngine: class { async lintFiles() { return [{ filePath: 'README.md', messages: [{ severity: 2 }] }]; } } };\n",
    );
    write_file(root.join("src/app.css"), "body { color: red; }\n");
    write_file(root.join("README.md"), "hello world\n");

    let output = run_cli(&root, &["lint"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(json["result"]["formatter"], "lania.lint.formatter.v1");
    assert_eq!(json["result"]["mode"], "check");
    assert_eq!(json["result"]["exitCode"], 1);
    assert!(json["result"]["summaryText"]
        .as_str()
        .unwrap_or_default()
        .contains("lint check"));
    assert!(json["result"]["resultsByAdaptor"]["stylelint"].is_object());
    assert!(json["result"]["resultsByAdaptor"]["textlint"].is_object());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_lint_linters_filter_and_grouped_output_e2e() {
    let root = temp_dir("lint-grouped-filter");
    write_file(
        root.join("lan.config.js"),
        "export default { lintTools: ['stylelint', 'textlint'] };\n",
    );
    write_fake_package(
        &root,
        "stylelint",
        "module.exports = { lint: async () => ({ results: [{ source: 'src/app.css', warnings: [{ severity: 'warning' }] }] }) };\n",
    );
    write_fake_package(
        &root,
        "textlint",
        "module.exports = { TextLintEngine: class { async lintFiles() { return [{ filePath: 'README.md', messages: [{ severity: 2 }] }]; } } };\n",
    );
    write_file(root.join("src/app.css"), "body { color: red; }\n");
    write_file(root.join("README.md"), "hello world\n");

    let output = run_cli(
        &root,
        &["lint", "--linters", "stylelint", "--grouped-output"],
        &[],
    );
    let json = parse_stdout_json(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["result"]["groupedOutput"], true);
    assert!(json["result"]["resultsByAdaptor"]["stylelint"].is_object());
    assert!(json["result"]["resultsByAdaptor"]["textlint"].is_null());
    assert!(stderr.contains("lint grouped output:"));
    assert!(stderr.contains("[stylelint] 0 errors, 1 warnings, 1 files"));
    assert!(stderr.contains("src/app.css: 0 errors, 1 warnings"));

    let _ = fs::remove_dir_all(root);
}
