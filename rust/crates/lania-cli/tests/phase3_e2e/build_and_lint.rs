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
fn lan_lint_uses_the_python_bridge_for_python_projects_e2e() {
    // B8 验收：Python 项目的 lint 走 `lan-py-bridge`（mypy 真 API）。
    // 没装 mypy 的环境下本用例会失败 —— 与项目其它「依赖真实工具」的用例同一前提。
    let root = temp_dir("lint-py-bridge");
    write_file(root.join("pyproject.toml"), "[project]\nname = \"demo\"\n");
    write_file(
        root.join("lan.config.js"),
        "export default { lintTools: ['mypy'] };\n",
    );
    write_file(
        root.join("app.py"),
        "def add(a: int, b: int) -> int:\n    return a + b\n\n\nresult: str = add(1, 2)\n",
    );

    let output = run_cli(&root, &["lint"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(
        json["result"]["bridge"], "lan-py-bridge",
        "python project must use the python bridge: {json}"
    );
    assert_eq!(
        json["result"]["resultsByAdaptor"]["mypy"]["implementation"],
        "runtime"
    );
    let diagnostics = json["result"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(!diagnostics.is_empty(), "mypy must report the type error");
    assert_eq!(diagnostics[0]["tool"], "mypy");
    assert_eq!(diagnostics[0]["id"], "assignment");
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 5);
    assert_eq!(output.status.code(), Some(1));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_lint_falls_back_when_the_python_bridge_is_unavailable_e2e() {
    // B8 验收：移除 bridge 后仍可用。
    // `LANIA_PYTHON` 指向不存在的解释器 ⇒ 登记失败 ⇒ 走 JS 簇 bridge 的 spawn 适配器。
    let root = temp_dir("lint-py-fallback");
    write_file(root.join("pyproject.toml"), "[project]\nname = \"demo\"\n");
    write_file(
        root.join("lan.config.js"),
        "export default { lintTools: ['mypy'] };\n",
    );
    write_file(root.join("app.py"), "value: int = 1\n");

    let output = run_cli(
        &root,
        &["lint"],
        &[("LANIA_PYTHON", "/definitely/not/a/python")],
    );
    let json = parse_stdout_json(&output);

    // 回落路径不写 `bridge` 字段（它来自 Python bridge 的聚合结果）。
    assert!(
        json["result"]["bridge"].is_null(),
        "fallback must not claim the python bridge: {json}"
    );
    assert!(json["result"]["resultsByAdaptor"].is_object());
    assert!(output.status.code().is_some());

    let _ = fs::remove_dir_all(root);
}

// ---- B10：Go bridge（`lan-go-bridge`）----

/// 一个能让 `go vet` 报错的极简 module（printf 参数类型不匹配）。
fn write_go_project(root: &Path, source: &str) {
    write_file(root.join("go.mod"), "module demo\n\ngo 1.21\n");
    write_file(root.join("main.go"), source);
}

#[test]
fn lan_lint_uses_the_go_bridge_for_go_projects_e2e() {
    // B10 验收：Go 项目的 lint 走 `lan-go-bridge`（`go vet` 由 bridge 内 exec）。
    // 与 Python 用例同一前提：需要真实 go 工具链（同时用来编译 bridge 本体）。
    let root = temp_dir("lint-go-bridge");
    write_go_project(
        &root,
        "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Printf(\"%d\", \"text\")\n}\n",
    );
    write_file(
        root.join("lan.config.js"),
        "export default { lintTools: ['go-vet'] };\n",
    );

    let output = run_cli(&root, &["lint"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(
        json["result"]["bridge"], "lan-go-bridge",
        "go project must use the go bridge: {json}"
    );
    assert_eq!(
        json["result"]["resultsByAdaptor"]["go-vet"]["implementation"],
        "runtime"
    );
    let diagnostics = json["result"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        !diagnostics.is_empty(),
        "go vet must report the printf issue"
    );
    assert_eq!(diagnostics[0]["tool"], "go-vet");
    assert_eq!(diagnostics[0]["severity"], "error");
    assert!(diagnostics[0]["file"]
        .as_str()
        .unwrap_or_default()
        .ends_with("main.go"));
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 6);
    assert_eq!(output.status.code(), Some(1));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_lint_falls_back_when_the_go_bridge_is_unavailable_e2e() {
    // B10 验收：移除 bridge 后仍可用。
    // `LANIA_GO` 指向不存在的工具链 ⇒ 无法编译 bridge ⇒ 回落 JS 簇的 spawn 适配器。
    let root = temp_dir("lint-go-fallback");
    write_go_project(
        &root,
        "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Printf(\"%d\", \"text\")\n}\n",
    );
    write_file(
        root.join("lan.config.js"),
        "export default { lintTools: ['go-vet'] };\n",
    );

    let bridge_home = isolated_home(&root);
    let output = run_cli(
        &root,
        &["lint"],
        &[
            ("LANIA_GO", "/definitely/not/a/go"),
            // 缓存目录也指到空目录：否则上一次用例编译好的 bridge 二进制会被命中，
            // 这条用例就测不到回落路径了。
            ("LANIA_BRIDGE_HOME", bridge_home.as_str()),
        ],
    );
    let json = parse_stdout_json(&output);

    // 回落路径不写 `bridge` 字段（它来自语言 bridge 的聚合结果）。
    assert!(
        json["result"]["bridge"].is_null(),
        "fallback must not claim the go bridge: {json}"
    );
    assert!(json["result"]["resultsByAdaptor"]["go-vet"].is_object());
    assert_eq!(output.status.code(), Some(1));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_lint_surfaces_go_tool_failures_instead_of_passing_e2e() {
    // 回归用例：`go vet` 退出码非 0、但输出里没有任何 `file:line:col` 时，
    // 不能被当成「0 errors」—— 否则 `lan lint` 会以 0 退出且什么都不显示，
    // 用户既看不到失败原因，也拿不到非零退出码。
    let root = temp_dir("lint-go-tool-failure");
    // 只有 go.mod、没有 .go 文件：`go vet ./...` 退出 1 并打印 "no packages to vet"。
    write_file(root.join("go.mod"), "module demo\n\ngo 1.21\n");

    let output = run_cli(&root, &["lint"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(json["result"]["bridge"], "lan-go-bridge", "{json}");
    let diagnostics = json["result"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        !diagnostics.is_empty(),
        "tool failure must be reported as a diagnostic: {json}"
    );
    assert_eq!(diagnostics[0]["tool"], "go-vet");
    assert_eq!(diagnostics[0]["severity"], "error");
    assert_eq!(output.status.code(), Some(1));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_build_uses_the_go_bridge_e2e() {
    // B10 验收：`lan build` 的进程链从 `Rust → node → worker → go build`
    // 降到 `Rust → go bridge → go build`，产物落在 `--output-dir`。
    let root = temp_dir("build-go-bridge");
    write_go_project(&root, "package main\n\nfunc main() {}\n");

    let output = run_cli(&root, &["build", "--output-dir", "bin/app"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["bridge"], "lan-go-bridge", "{json}");
    assert_eq!(json["result"]["runtime"], "go");
    assert_eq!(json["result"]["success"], true);
    assert_eq!(json["result"]["exitCode"], 0);
    assert!(
        root.join("bin/app").exists() || root.join("bin/app.exe").exists(),
        "go build must produce the binary"
    );

    let _ = fs::remove_dir_all(root);
}

// ---- B11：Rust bridge（`lan-rs-bridge`，由 `lan` 二进制自身承担）----

/// 一个能被 clippy 稳定报出 `needless_range_loop` 的最小 crate。
///
/// 刻意用数组而不是 `vec![…]`：否则 `clippy::useless_vec` 会先命中，
/// 断言就不稳（两个 lint 都在同一份输出里，排序不同会换首条）。
const RUST_RANGE_LOOP_SOURCE: &str = "fn main() {\n    let values = [1, 2, 3];\n    let mut total = 0;\n    for index in 0..values.len() {\n        total += values[index];\n    }\n    println!(\"{total}\");\n}\n";

fn write_rust_project(root: &Path, source: &str) {
    write_file(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    );
    write_file(root.join("src/main.rs"), source);
}

#[test]
fn lan_lint_uses_the_rust_bridge_for_rust_projects_e2e() {
    // B11 验收：Rust 项目的 lint 走 `lan-rs-bridge`（`lan` 二进制自身的 bridge 模式），
    // 由 bridge 内 exec `cargo clippy --message-format=json` 并归一化成 `Diagnostic`。
    let root = temp_dir("lint-rs-bridge");
    write_rust_project(&root, RUST_RANGE_LOOP_SOURCE);

    let output = run_cli(&root, &["lint"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(
        json["result"]["bridge"], "lan-rs-bridge",
        "rust project must use the rust bridge: {json}"
    );
    assert_eq!(
        json["result"]["resultsByAdaptor"]["clippy"]["implementation"],
        "runtime"
    );
    let diagnostics = json["result"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        !diagnostics.is_empty(),
        "clippy must report the range loop: {json}"
    );
    assert_eq!(diagnostics[0]["tool"], "clippy");
    assert_eq!(diagnostics[0]["severity"], "warning");
    assert_eq!(diagnostics[0]["id"], "clippy::needless_range_loop");
    assert!(diagnostics[0]["file"]
        .as_str()
        .unwrap_or_default()
        .ends_with("main.rs"));
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 4);
    // clippy 的 warning 不算失败（与 JS 簇路径一致：只有 error 才让 lint 失败）。
    assert_eq!(output.status.code(), Some(0));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_build_uses_the_rust_bridge_e2e() {
    // B11 验收：`lan build` 走 `lan-rs-bridge`，进程链 `Rust → rs bridge → cargo build`。
    let root = temp_dir("build-rs-bridge");
    write_rust_project(&root, "fn main() {\n    println!(\"hello\");\n}\n");

    let output = run_cli(&root, &["build"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["bridge"], "lan-rs-bridge", "{json}");
    assert_eq!(json["result"]["runtime"], "rust");
    assert_eq!(json["result"]["success"], true);
    assert_eq!(
        json["result"]["command"][0], "cargo",
        "command must come from toolchain.describe: {json}"
    );
    assert!(
        root.join("target/release").is_dir(),
        "cargo build must produce target/release"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_lint_falls_back_when_the_rust_bridge_is_unavailable_e2e() {
    // B11 验收：移除 bridge 后仍可用（`LANIA_RS_BRIDGE` 指向不存在的可执行文件）。
    let root = temp_dir("lint-rs-fallback");
    write_rust_project(&root, RUST_RANGE_LOOP_SOURCE);
    // JS 簇路径按 `lan.config.lintTools` 选工具（缺省是 eslint），这里显式要 clippy。
    write_file(
        root.join("lan.config.js"),
        "export default { lintTools: ['clippy'] };\n",
    );

    let output = run_cli(
        &root,
        &["lint"],
        &[("LANIA_RS_BRIDGE", "/definitely/not/a/bridge")],
    );
    let json = parse_stdout_json(&output);

    assert!(
        json["result"]["bridge"].is_null(),
        "fallback must not claim the rust bridge: {json}"
    );
    assert!(
        json["result"]["resultsByAdaptor"]["clippy"].is_object(),
        "the JS cluster path must still run clippy: {json}"
    );

    let _ = fs::remove_dir_all(root);
}

// ---- B9：Dart bridge（`lan-dart-bridge`）----

fn write_dart_project(root: &Path, source: &str) {
    write_file(
        root.join("pubspec.yaml"),
        "name: demo\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n",
    );
    write_file(root.join("bin/main.dart"), source);
}

#[test]
fn lan_lint_uses_the_dart_bridge_for_dart_projects_e2e() {
    // B9 验收：Dart 项目的 lint 走 `lan-dart-bridge`（`dart analyze --format machine`）。
    // 需要真实 dart SDK —— 与 Python / Go 用例同一前提。
    let root = temp_dir("lint-dart-bridge");
    write_dart_project(
        &root,
        "void main() {\n  int value = 'text';\n  print(value);\n}\n",
    );

    let output = run_cli(&root, &["lint"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(
        json["result"]["bridge"], "lan-dart-bridge",
        "dart project must use the dart bridge: {json}"
    );
    assert_eq!(
        json["result"]["resultsByAdaptor"]["dart-analyze"]["implementation"],
        "runtime"
    );
    let diagnostics = json["result"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        !diagnostics.is_empty(),
        "dart analyze must report the error"
    );
    assert_eq!(diagnostics[0]["tool"], "dart-analyze");
    assert_eq!(diagnostics[0]["id"], "INVALID_ASSIGNMENT");
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 2);
    assert_eq!(output.status.code(), Some(1));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_build_uses_the_dart_bridge_e2e() {
    // B9 验收：Dart 项目的 build 去掉 Node 中转，
    // 进程链变成 `Rust → dart bridge → dart compile`。
    let root = temp_dir("build-dart-bridge");
    write_dart_project(&root, "void main() {\n  print('hello');\n}\n");

    let output = run_cli(&root, &["build", "--output-dir", "bin/app"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0), "{json}");
    assert_eq!(json["result"]["bridge"], "lan-dart-bridge", "{json}");
    assert_eq!(json["result"]["runtime"], "dart");
    assert_eq!(json["result"]["success"], true);
    assert!(
        root.join("bin/app.exe").exists() || root.join("bin/app").exists(),
        "dart compile must produce the executable"
    );

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

#[test]
fn lan_dev_streams_app_logs_with_prefix_and_quiet_app_suppresses_e2e() {
    // §12 验收：`lan dev` 启动后，应用自身 stdout（服务端访问日志）实时出现且带 `[app]`，
    // `--quiet-app` 能关掉它。
    //
    // 用 Rust 项目走 JS 簇的 spawn 适配器（dev 仍留在 node bridge，见 §4.2）：
    // 应用每 200ms 打一行，因此「就绪（~250ms grace / 1s 超时）之后」的行必然走
    // `channel: "app"`，不会因为编译耗时把断言拖成 flaky。
    let root = temp_dir("dev-app-logs");
    write_rust_project(
        &root,
        "fn main() {\n    for _ in 0..30 {\n        println!(\"GET /api/user 200\");\n        std::thread::sleep(std::time::Duration::from_millis(200));\n    }\n}\n",
    );

    let output = run_cli(
        &root,
        &["dev"],
        &[
            ("LANIA_INTERRUPT_AFTER_MS", "6000"),
            ("LANIA_TIMEOUT_MS", "30000"),
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[app] GET /api/user 200"),
        "app logs must be streamed with the [app] prefix: {stderr}"
    );

    let quiet = run_cli(
        &root,
        &["dev", "--quiet-app"],
        &[
            ("LANIA_INTERRUPT_AFTER_MS", "6000"),
            ("LANIA_TIMEOUT_MS", "30000"),
        ],
    );
    let quiet_stderr = String::from_utf8_lossy(&quiet.stderr);
    assert!(
        !quiet_stderr.contains("[app] "),
        "--quiet-app must suppress app logs: {quiet_stderr}"
    );

    let _ = fs::remove_dir_all(root);
}
