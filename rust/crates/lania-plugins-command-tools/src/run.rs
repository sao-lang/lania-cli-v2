//! `tools run`：把脚本交给 `lania-runtime` 识别并执行。

use anyhow::Result;
use lania_command::{ArgSpec, CommandSpec};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext},
};
use lania_runtime::RunRequest;
use serde_json::json;
use std::path::PathBuf;

use crate::{
    shared::{json_value_to_args, required_arg},
    RUN_HANDLER_ID,
};

pub(super) fn spec() -> CommandSpec {
    CommandSpec::new(
        "run",
        "Run a code file with a detected runtime",
        RUN_HANDLER_ID,
    )
    .with_args(vec![
        ArgSpec {
            name: "file".into(),
            required: true,
            multiple: false,
            help: "Path to the code file to execute".into(),
        },
        ArgSpec {
            name: "args".into(),
            required: false,
            multiple: true,
            help: "Additional arguments passed to the executed file".into(),
        },
    ])
}

pub(super) fn execute(ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
    ctx.require_capability(CapabilityName::Exec)?;

    let file = required_arg(ctx.command(), "file")?;
    let resolved = crate::shared::resolve_from_cwd(&ctx.command().cwd, &file);
    let args = ctx
        .command()
        .argv
        .args
        .get("args")
        .map(json_value_to_args)
        .unwrap_or_default();

    let result = ctx.runtime().run(
        ctx.exec(),
        &RunRequest {
            path: resolved,
            cwd: PathBuf::from(&ctx.command().cwd),
            args,
        },
    )?;

    let output = json!({
        "kind": "tool_run_result",
        "file": result.file,
        "language": result.language,
        "runtime": result.runtime,
        "program": result.program,
        "args": result.args,
        "cwd": result.cwd,
        "preparedCommand": result.prepared_command,
        "preparedSummary": result.prepared_summary,
        "exitCode": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "skipped": result.skipped,
        "timedOut": result.timed_out,
        "cancelled": result.cancelled,
    });
    Ok(ctx.complete_template_info(output, result.exit_code))
}
