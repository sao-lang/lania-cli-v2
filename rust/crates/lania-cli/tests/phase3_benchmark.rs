use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lania-cli-bench-{name}-{unique}"));
    fs::create_dir_all(&root).expect("temp dir created");
    root
}

fn write_file(path: impl AsRef<Path>, content: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent dir created");
    }
    fs::write(path, content).expect("file written");
}

fn run_cli(cwd: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lania-cli"));
    command.args(args).current_dir(cwd);
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.output().expect("cli runs")
}

#[test]
#[ignore = "manual benchmark smoke for async runtime command timings"]
fn async_runtime_smoke_benchmark_reports_command_timings() {
    let root = temp_dir("async-runtime");
    write_file(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite', lintTools: ['eslint'] };\n",
    );
    write_file(root.join("src/index.js"), "const answer = 42\n");

    let scenarios = [
        ("build", vec!["build"]),
        ("lint", vec!["lint"]),
        ("dev", vec!["dev", "--port", "3401"]),
    ];

    for (name, args) in scenarios {
        let started = Instant::now();
        let output = if name == "dev" {
            run_cli(&root, &args, &[("LANIA_INTERRUPT_AFTER_MS", "20")])
        } else {
            run_cli(&root, &args, &[])
        };
        let elapsed = started.elapsed();
        assert!(
            output.status.success() || output.status.code() == Some(130),
            "{name} should complete or exit on interrupt"
        );
        eprintln!("benchmark {name}: {}ms", elapsed.as_millis());
    }

    let _ = fs::remove_dir_all(root);
}
