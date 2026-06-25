use anyhow::{anyhow, Result};
use lania_exec::ExecCommand;
use std::path::{Path, PathBuf};

use super::{resolve::choose_first_available, RunPlan};

pub(super) fn build_compiled_run_plan(
    path: &Path,
    cwd: &Path,
    candidates: &[(&str, Vec<String>)],
    label: &str,
    suffix: &str,
) -> Result<RunPlan> {
    let (compiler, mut compiler_args) = choose_first_available(path, cwd, candidates)?;
    let output = compiled_output_path(path, suffix)?;
    let output_text = output.to_str().map(ToOwned::to_owned).ok_or_else(|| {
        anyhow!(
            "compiled output path is not valid UTF-8: {}",
            output.display()
        )
    })?;
    let file_text = path
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))?;

    // 编译型语言会先生成临时可执行文件，再在第二步真正执行。
    match label {
        "rustc" => {
            compiler_args.push(file_text.clone());
            compiler_args.push("--crate-name".into());
            compiler_args.push(sanitize_rust_crate_name(path));
            compiler_args.push("-o".into());
            compiler_args.push(output_text.clone());
        }
        _ => {
            compiler_args.push(file_text.clone());
            compiler_args.push("-o".into());
            compiler_args.push(output_text.clone());
        }
    }

    let prepared_command = ExecCommand::new(compiler.clone())
        .with_args(compiler_args)
        .in_dir(cwd.display().to_string());

    Ok(RunPlan {
        runtime: label.to_string(),
        program: output_text,
        program_args: Vec::new(),
        prepared_summary: Some(format!("compile {} before execution", path.display())),
        prepared_command: Some(prepared_command),
    })
}

fn compiled_output_path(path: &Path, suffix: &str) -> Result<PathBuf> {
    let stem = path
        .file_stem()
        .and_then(|item| item.to_str())
        .unwrap_or("tool-run");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock works")
        .as_nanos();
    let extension = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    Ok(std::env::temp_dir().join(format!("lania-run-{stem}-{suffix}-{unique}{extension}")))
}

fn sanitize_rust_crate_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|item| item.to_str())
        .unwrap_or("tool_run");
    let mut sanitized = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        sanitized.push_str("tool_run");
    }
    if sanitized
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        sanitized.insert_str(0, "tool_");
    }
    sanitized
}
