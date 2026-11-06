use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use lania_exec::{resolve_executable, ExecService};

use crate::{parse_shebang_line, RunRequest, RuntimeService};

#[test]
fn detects_supported_run_file_kinds() {
    let service = RuntimeService::new();
    let detect = |name: &str| service.detect(Path::new(name)).expect("supported");

    assert_eq!(detect("/tmp/demo.ts").language, "typescript");
    assert_eq!(detect("/tmp/demo.py").language, "python");
    assert_eq!(detect("/tmp/demo.go").language, "go");
    assert_eq!(detect("/tmp/Demo.java").language, "java");
    assert_eq!(detect("/tmp/demo.rs").language, "rust");
    assert_eq!(detect("/tmp/demo.sh").language, "sh");
    assert_eq!(detect("/tmp/demo.zsh").language, "zsh");
    assert!(service.detect(Path::new("/tmp/demo.txt")).is_err());
}

#[test]
fn parses_env_style_shebang() {
    let plan =
        parse_shebang_line("/usr/bin/env -S node --no-warnings").expect("shebang should parse");
    assert_eq!(plan.language, "shebang");
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
    let root = unique_temp_dir("runtime-local-runtime");
    let script = root.join("src/demo.ts");
    let runtime_name = if cfg!(windows) { "tsx.cmd" } else { "tsx" };
    let local_runtime = root.join("node_modules/.bin").join(runtime_name);
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
        if cfg!(windows) {
            "@echo off\r\n"
        } else {
            "#!/bin/sh
"
        },
    )
    .expect("runtime written");

    let service = RuntimeService::new();
    let plan = service.plan(&script, &root).expect("run plan resolves");
    assert_eq!(plan.language, "typescript");
    assert_eq!(plan.runtime, "tsx");
    let resolved = Path::new(&plan.program);
    assert!(resolved.is_file());
    assert_eq!(
        resolved
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase(),
        runtime_name.to_ascii_lowercase()
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn builds_compiled_plan_for_rust_source() {
    let root = unique_temp_dir("runtime-rust-plan");
    let script = root.join("src/demo.rs");
    fs::create_dir_all(script.parent().expect("script parent")).expect("script dir created");
    fs::write(
        &script,
        "fn main() {}
",
    )
    .expect("script written");

    let service = RuntimeService::new();
    let plan = service.plan(&script, &root).expect("run plan resolves");
    assert_eq!(plan.runtime, "rustc");
    assert_eq!(plan.language, "rust");
    assert!(plan.prepared_command.is_some());
    assert!(plan.prepared_summary.is_some());
    assert!(plan.cleanup_path.is_some());
    assert!(plan.program.contains("lania-run-demo-rust-source"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runs_rust_file_and_cleans_temp_output() {
    if resolve_executable("rustc").is_none() {
        return;
    }
    let root = unique_temp_dir("runtime-rust-run");
    let script = root.join("demo.rs");
    fs::create_dir_all(&root).expect("root created");
    fs::write(&script, "fn main() { println!(\"rust-runtime-ok\"); }\n")
        .expect("rust file written");

    let service = RuntimeService::new();
    let result = service
        .run(
            &ExecService::new(false),
            &RunRequest {
                path: script.clone(),
                cwd: root.clone(),
                args: Vec::new(),
            },
        )
        .expect("rust run succeeds");

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("rust-runtime-ok"));
    assert_eq!(result.language, "rust");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn java_run_command_passes_source_file_once() {
    if resolve_executable("java").is_none() {
        return;
    }
    let root = unique_temp_dir("runtime-java");
    let script = root.join("Demo.java");
    fs::create_dir_all(&root).expect("root created");
    fs::write(
        &script,
        "class Demo {}
",
    )
    .expect("java file written");

    let service = RuntimeService::new();
    let plan = service.plan(&script, &root).expect("java plan resolves");
    let command = service
        .build_command(&script, &plan, &[], &root)
        .expect("java command builds");

    assert!(command.program.ends_with("java"));
    assert_eq!(command.args, vec![script.display().to_string()]);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runs_python_file_when_python_is_available() {
    if resolve_executable("python3").is_none() && resolve_executable("python").is_none() {
        return;
    }
    let root = unique_temp_dir("runtime-python");
    let script = root.join("demo.py");
    fs::create_dir_all(&root).expect("root created");
    fs::write(&script, "print('runtime-ok')\n").expect("python file written");

    let service = RuntimeService::new();
    let result = service
        .run(
            &ExecService::new(false),
            &RunRequest {
                path: script.clone(),
                cwd: root.clone(),
                args: Vec::new(),
            },
        )
        .expect("python run succeeds");

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("runtime-ok"));
    assert_eq!(result.language, "python");

    let _ = fs::remove_dir_all(root);
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock works")
        .as_nanos();
    std::env::temp_dir().join(format!("lania-{name}-{unique}"))
}
