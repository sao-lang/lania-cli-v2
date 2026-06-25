use anyhow::{anyhow, Result};
use lania_command::CommandContext;
use lania_exec::ExecCommand;
use std::path::Path;

use crate::shared::json_value_to_args;

use super::{
    compile::build_compiled_run_plan, detect::detect_run_file_kind, display_runtime_name,
    resolve::choose_first_available, shebang::parse_shebang, RunFileKind, RunPlan,
};

pub(super) fn build_run_plan(path: &Path, cwd: &Path) -> Result<RunPlan> {
    if let Some(plan) = parse_shebang(path, cwd)? {
        return Ok(plan);
    }

    match detect_run_file_kind(path)? {
        RunFileKind::JavaScript => {
            build_simple_run_plan(path, cwd, &[("node", vec![]), ("bun", vec![])])
        }
        RunFileKind::TypeScript => build_simple_run_plan(
            path,
            cwd,
            &[("tsx", vec![]), ("bun", vec![]), ("ts-node", vec![])],
        ),
        RunFileKind::Python => {
            build_simple_run_plan(path, cwd, &[("python3", vec![]), ("python", vec![])])
        }
        RunFileKind::Shell(preferred) => build_simple_run_plan(
            path,
            cwd,
            &[(preferred, vec![]), ("bash", vec![]), ("sh", vec![])],
        ),
        RunFileKind::Ruby => build_simple_run_plan(path, cwd, &[("ruby", vec![])]),
        RunFileKind::Php => build_simple_run_plan(path, cwd, &[("php", vec![])]),
        RunFileKind::Go => build_simple_run_plan(path, cwd, &[("go", vec!["run".into()])]),
        RunFileKind::Java => build_simple_run_plan(path, cwd, &[("java", vec![])]),
        RunFileKind::Lua => build_simple_run_plan(path, cwd, &[("lua", vec![])]),
        RunFileKind::Dart => build_simple_run_plan(path, cwd, &[("dart", vec!["run".into()])]),
        RunFileKind::Nim => build_simple_run_plan(path, cwd, &[("nim", vec!["r".into()])]),
        RunFileKind::Zig => build_simple_run_plan(path, cwd, &[("zig", vec!["run".into()])]),
        RunFileKind::Kotlin => build_simple_run_plan(
            path,
            cwd,
            &[("kotlin", vec![]), ("kotlinc", vec!["-script".into()])],
        ),
        RunFileKind::Swift => build_simple_run_plan(path, cwd, &[("swift", vec![])]),
        RunFileKind::Rust => {
            build_compiled_run_plan(path, cwd, &[("rustc", vec![])], "rustc", "rust-source")
        }
        RunFileKind::C => build_compiled_run_plan(
            path,
            cwd,
            &[("cc", vec![]), ("clang", vec![]), ("gcc", vec![])],
            "cc",
            "c-source",
        ),
    }
}

pub(super) fn build_run_command(
    path: &Path,
    plan: &RunPlan,
    context: &CommandContext,
) -> Result<ExecCommand> {
    let extra_args = context
        .argv
        .args
        .get("args")
        .map(json_value_to_args)
        .unwrap_or_default();

    let mut args = plan.program_args.clone();
    if plan.prepared_command.is_none() {
        let file = path
            .to_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))?;
        args.push(file);
    }
    args.extend(extra_args);

    Ok(ExecCommand::new(plan.program.clone())
        .with_args(args)
        .in_dir(context.cwd.clone()))
}

fn build_simple_run_plan(
    path: &Path,
    cwd: &Path,
    candidates: &[(&str, Vec<String>)],
) -> Result<RunPlan> {
    let (program, args) = choose_first_available(path, cwd, candidates)?;
    Ok(RunPlan {
        runtime: display_runtime_name(&program),
        program,
        program_args: args,
        prepared_command: None,
        prepared_summary: None,
    })
}
