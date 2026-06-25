//! `tools run` 的文件类型识别、运行时解析与执行逻辑。

mod compile;
mod detect;
mod planner;
mod resolve;
mod shebang;

#[cfg(test)]
mod tests;

use anyhow::Result;
use lania_command::{ArgSpec, CommandSpec};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext},
};
use serde_json::json;
use std::{fs, path::PathBuf};

use self::planner::{build_run_command, build_run_plan};
use crate::{shared::required_arg, RUN_HANDLER_ID};

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
    let run_plan = build_run_plan(&resolved, std::path::Path::new(&ctx.command().cwd))?;
    if let Some(prepare) = run_plan.prepared_command.clone() {
        ctx.exec().run_checked(prepare)?;
    }

    let command = build_run_command(&resolved, &run_plan, ctx.command())?;
    let result = ctx.exec().run(command.clone());
    if run_plan.prepared_command.is_some() {
        let _ = fs::remove_file(&command.program);
    }
    let result = result?;

    let output = json!({
        "kind": "tool_run_result",
        "file": resolved.display().to_string(),
        "runtime": run_plan.runtime,
        "program": command.program,
        "args": command.args,
        "cwd": command.cwd,
        "preparedCommand": run_plan.prepared_command.as_ref().map(|cmd| json!({
            "program": cmd.program,
            "args": cmd.args,
            "cwd": cmd.cwd,
        })),
        "preparedSummary": run_plan.prepared_summary,
        "exitCode": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "skipped": result.skipped,
        "timedOut": result.timed_out,
        "cancelled": result.cancelled,
    });
    Ok(ctx.complete_template_info(output, result.exit_code))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RunFileKind {
    JavaScript,
    TypeScript,
    Python,
    Shell(&'static str),
    Ruby,
    Php,
    Go,
    Java,
    Rust,
    C,
    Lua,
    Dart,
    Nim,
    Zig,
    Kotlin,
    Swift,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunPlan {
    runtime: String,
    program: String,
    program_args: Vec<String>,
    prepared_command: Option<lania_exec::ExecCommand>,
    prepared_summary: Option<String>,
}

fn display_runtime_name(program: &str) -> String {
    PathBuf::from(program)
        .file_name()
        .and_then(|item| item.to_str())
        .unwrap_or(program)
        .to_string()
}
