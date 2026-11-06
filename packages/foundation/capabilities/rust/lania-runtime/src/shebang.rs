use anyhow::{anyhow, Context, Result};
use std::{fs, path::Path};

use crate::{display_runtime_name, resolve::resolve_command, RunPlan};

/// 读取文件首行的 shebang 并解析为未解析可执行文件的运行计划。
pub(crate) fn read_shebang(path: &Path) -> Result<Option<RunPlan>> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read file {}", path.display()))?;
    let Some(first_line) = bytes.split(|byte| *byte == b'\n').next() else {
        return Ok(None);
    };
    if !first_line.starts_with(b"#!") {
        return Ok(None);
    }
    let line = String::from_utf8_lossy(&first_line[2..]).trim().to_string();
    Ok(parse_shebang_line(&line))
}

/// 读取 shebang，并把解释器解析成当前环境可执行的路径。
pub(crate) fn parse_shebang(path: &Path, cwd: &Path) -> Result<Option<RunPlan>> {
    let Some(plan) = read_shebang(path)? else {
        return Ok(None);
    };
    Ok(Some(resolve_run_plan_program(plan, path, cwd)?))
}

pub fn parse_shebang_line(line: &str) -> Option<RunPlan> {
    let tokens = shell_words::split(line).ok()?;
    if tokens.is_empty() {
        return None;
    }

    let (program, args) = if is_env_program(&tokens[0]) {
        parse_env_shebang(&tokens[1..])?
    } else {
        (
            tokens[0].to_string(),
            tokens[1..].iter().map(|item| (*item).to_string()).collect(),
        )
    };

    Some(RunPlan {
        language: "shebang".into(),
        runtime: display_runtime_name(&program),
        program,
        program_args: args,
        prepared_command: None,
        prepared_summary: None,
        cleanup_path: None,
    })
}

fn parse_env_shebang(tokens: &[String]) -> Option<(String, Vec<String>)> {
    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens.get(index)?;
        if token == "-S" {
            let joined = tokens[index + 1..].join(" ");
            let split = shell_words::split(&joined).ok()?;
            let program = split.first()?.to_string();
            let args = split.iter().skip(1).cloned().collect();
            return Some((program, args));
        }
        if token.starts_with('-') || is_env_assignment(token) {
            index += 1;
            continue;
        }
        let program = token.to_string();
        let args = tokens.iter().skip(index + 1).cloned().collect();
        return Some((program, args));
    }
    None
}

fn is_env_program(token: &str) -> bool {
    token == "env" || token.ends_with("/env")
}

fn is_env_assignment(token: &str) -> bool {
    let Some((key, _)) = token.split_once('=') else {
        return false;
    };
    !key.is_empty() && !key.starts_with('-')
}

fn resolve_run_plan_program(plan: RunPlan, script_path: &Path, cwd: &Path) -> Result<RunPlan> {
    let resolved = resolve_command(&plan.program, script_path, cwd)
        .ok_or_else(|| anyhow!("shebang runtime `{}` is not available", plan.program))?;
    Ok(RunPlan {
        runtime: display_runtime_name(&resolved),
        program: resolved,
        ..plan
    })
}
