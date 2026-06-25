use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use super::{ExecCommand, ExecError, ExecErrorCode, ExecEvent, ExecRunOptions, ExecService};

#[test]
fn records_dry_run_commands() {
    let service = ExecService::dry_run();
    let result = service
        .run(ExecCommand::new("echo").with_args(["hello"]))
        .expect("dry-run succeeds");

    assert!(result.skipped);
    assert_eq!(service.history().len(), 1);
}

#[test]
fn supports_env_and_checked_run() {
    let service = ExecService::default();
    let result = service
        .run_checked(
            ExecCommand::new("python3")
                .with_args(["-c", "import os; print(os.environ['LANIA_EXEC_TEST'])"])
                .with_env("LANIA_EXEC_TEST", "ok"),
        )
        .expect("checked run succeeds");

    assert_eq!(result.stdout.trim(), "ok");
    assert!(!service.working_dir().expect("cwd available").is_empty());
}

#[tokio::test]
async fn streams_stdout_and_stderr_events() {
    let service = ExecService::default();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_sink = Arc::clone(&events);
    let options = ExecRunOptions {
        on_event: Some(Arc::new(move |event| {
            events_sink.lock().expect("events lock").push(event);
        })),
        ..ExecRunOptions::default()
    };

    let result = service
        .run_with_options_async(
            ExecCommand::new("python3").with_args([
                "-c",
                "import sys; print('out'); print('err', file=sys.stderr)",
            ]),
            options,
        )
        .await
        .expect("exec succeeds");

    let captured = events.lock().expect("events lock");
    assert!(captured
        .iter()
        .any(|event| matches!(event, ExecEvent::Stdout(line) if line == "out")));
    assert!(captured
        .iter()
        .any(|event| matches!(event, ExecEvent::Stderr(line) if line == "err")));
    assert_eq!(result.stdout, "out");
    assert_eq!(result.stderr, "err");
}

#[tokio::test]
async fn times_out_and_cleans_up_process() {
    let service = ExecService::default();
    let result = service
        .run_with_options_async(
            ExecCommand::new("python3")
                .with_args(["-c", "import time; print('start'); time.sleep(2)"]),
            ExecRunOptions {
                timeout: Some(Duration::from_millis(100)),
                kill_process_tree: true,
                ..ExecRunOptions::default()
            },
        )
        .await
        .expect("timeout result returned");

    assert!(result.timed_out);
    assert_eq!(result.exit_code, -1);
}

#[tokio::test]
async fn propagates_cancellation() {
    let service = ExecService::default();
    let cancellation = CancellationToken::new();
    let child_token = cancellation.clone();
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        child_token.cancel();
    });

    let result = service
        .run_with_options_async(
            ExecCommand::new("python3").with_args(["-c", "import time; time.sleep(2)"]),
            ExecRunOptions {
                cancellation: Some(cancellation),
                kill_process_tree: true,
                ..ExecRunOptions::default()
            },
        )
        .await
        .expect("cancel result returned");
    canceller.await.expect("canceller joins");

    assert!(result.cancelled);
    assert_eq!(result.exit_code, -2);
}

#[test]
fn requires_explicit_shell_mode() {
    let service = ExecService::dry_run();
    let result = service.run(ExecCommand::shell("echo 'hello from shell'"));
    assert!(result.is_ok());
}

#[test]
fn checked_run_returns_typed_command_failure() {
    let service = ExecService::default();
    let error = service
        .run_checked(ExecCommand::new("python3").with_args([
            "-c",
            "import sys; print('boom', file=sys.stderr); sys.exit(7)",
        ]))
        .expect_err("checked run should fail");
    let exec_error = error.downcast_ref::<ExecError>().expect("typed exec error");

    assert_eq!(exec_error.code, ExecErrorCode::CommandFailed);
    assert_eq!(exec_error.exit_code, Some(7));
    assert!(exec_error.stderr.contains("boom"));
}

#[test]
fn missing_binary_returns_typed_spawn_failure() {
    let service = ExecService::default();
    let error = service
        .run(ExecCommand::new("lania-command-that-should-not-exist"))
        .expect_err("missing binary should fail");
    let exec_error = error.downcast_ref::<ExecError>().expect("typed exec error");

    assert_eq!(exec_error.code, ExecErrorCode::BinaryMissing);
    assert!(exec_error.message.contains("failed to spawn"));
}
