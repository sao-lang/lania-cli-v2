use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    detect::detect_run_file_kind,
    planner::{build_run_command, build_run_plan},
    shebang::parse_shebang_line,
    RunFileKind,
};
use crate::RUN_HANDLER_ID;

fn runtime_name(kind: &RunFileKind) -> &'static str {
    match kind {
        RunFileKind::JavaScript => "node",
        RunFileKind::TypeScript => "tsx",
        RunFileKind::Python => "python3",
        RunFileKind::Shell(preferred) => preferred,
        RunFileKind::Ruby => "ruby",
        RunFileKind::Php => "php",
        RunFileKind::Go => "go",
        RunFileKind::Java => "java",
        RunFileKind::Rust => "rustc",
        RunFileKind::C => "cc",
        RunFileKind::Lua => "lua",
        RunFileKind::Dart => "dart",
        RunFileKind::Nim => "nim",
        RunFileKind::Zig => "zig",
        RunFileKind::Kotlin => "kotlin",
        RunFileKind::Swift => "swift",
    }
}

#[test]
fn detects_supported_run_file_kinds() {
    let ts = std::path::PathBuf::from("/tmp/demo.ts");
    let py = std::path::PathBuf::from("/tmp/demo.py");
    let go = std::path::PathBuf::from("/tmp/demo.go");
    let java = std::path::PathBuf::from("/tmp/Demo.java");
    let rust = std::path::PathBuf::from("/tmp/demo.rs");
    let txt = std::path::PathBuf::from("/tmp/demo.txt");
    assert_eq!(
        detect_run_file_kind(ts.as_path()).expect("ts is supported"),
        RunFileKind::TypeScript
    );
    assert_eq!(
        detect_run_file_kind(py.as_path()).expect("py is supported"),
        RunFileKind::Python
    );
    assert_eq!(
        detect_run_file_kind(go.as_path()).expect("go is supported"),
        RunFileKind::Go
    );
    assert_eq!(
        detect_run_file_kind(java.as_path()).expect("java is supported"),
        RunFileKind::Java
    );
    assert_eq!(
        detect_run_file_kind(rust.as_path()).expect("rust is supported"),
        RunFileKind::Rust
    );
    assert_eq!(runtime_name(&RunFileKind::Shell("bash")), "bash");
    assert!(detect_run_file_kind(txt.as_path()).is_err());
}

#[test]
fn parses_env_style_shebang() {
    let plan =
        parse_shebang_line("/usr/bin/env -S node --no-warnings").expect("shebang should parse");
    assert_eq!(plan.runtime, "node");
    assert_eq!(plan.program, "node");
    assert_eq!(plan.program_args, vec!["--no-warnings".to_string()]);
}

#[test]
fn parses_env_shebang_with_assignments() {
    let plan = parse_shebang_line("/usr/bin/env FOO=bar python3 -u")
        .expect("assignment shebang should parse");
    assert_eq!(plan.runtime, "python3");
    assert_eq!(plan.program, "python3");
    assert_eq!(plan.program_args, vec!["-u".to_string()]);
}

#[test]
fn prefers_project_local_runtime_for_typescript() {
    let root = unique_temp_dir("tools-run-local-runtime");
    let script = root.join("src/demo.ts");
    let local_runtime = root.join("node_modules/.bin/tsx");
    fs::create_dir_all(script.parent().expect("script parent")).expect("script dir created");
    fs::create_dir_all(local_runtime.parent().expect("runtime parent"))
        .expect("runtime dir created");
    fs::write(
        &script,
        "console.log('demo')
",
    )
    .expect("script written");
    fs::write(
        &local_runtime,
        "#!/bin/sh
",
    )
    .expect("runtime written");

    let plan = build_run_plan(&script, &root).expect("run plan resolves");
    assert_eq!(plan.runtime, "tsx");
    assert_eq!(plan.program, local_runtime.display().to_string());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn builds_compiled_plan_for_rust_source() {
    let root = unique_temp_dir("tools-run-rust-plan");
    let script = root.join("src/demo.rs");
    fs::create_dir_all(script.parent().expect("script parent")).expect("script dir created");
    fs::write(
        &script,
        "fn main() {}
",
    )
    .expect("script written");

    let plan = build_run_plan(&script, &root).expect("run plan resolves");
    assert_eq!(plan.runtime, "rustc");
    assert!(plan.prepared_command.is_some());
    assert!(plan.prepared_summary.is_some());
    assert!(plan.program.contains("lania-run-demo-rust-source"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn java_run_command_passes_source_file_once() {
    let root = unique_temp_dir("tools-run-java");
    let script = root.join("Demo.java");
    fs::create_dir_all(&root).expect("root created");
    fs::write(
        &script,
        "class Demo {}
",
    )
    .expect("java file written");

    let plan = build_run_plan(&script, &root).expect("java plan resolves");
    let context = lania_command::CommandContext {
        cwd: root.display().to_string(),
        argv: lania_command::ParsedArgv {
            args: Default::default(),
            options: Default::default(),
        },
        handler_id: RUN_HANDLER_ID.into(),
        trace_id: "trace-java".into(),
    };
    let command = build_run_command(&script, &plan, &context).expect("java command builds");

    assert!(command.program.ends_with("java"));
    assert_eq!(command.args, vec![script.display().to_string()]);

    let _ = fs::remove_dir_all(root);
}

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock works")
        .as_nanos();
    std::env::temp_dir().join(format!("lania-{name}-{unique}"))
}
