//! 多语言脚本运行时的识别、解析与执行。
//!
//! 该 crate 是 `lan tools run`、`host.runtime.*` 与 `ctx.tools.runtime` 的共享实现：
//! - 只描述“某个文件应该由哪个运行时执行”
//! - 不直接承担项目级工具链探测（那是 `runex-core` 的职责）
//! - 编译型语言会先生成临时可执行文件，执行结束后由 [`RuntimeService::run`] 清理

mod registry;
mod resolve;
mod shebang;

#[cfg(test)]
mod tests;

pub use shebang::parse_shebang_line;

use anyhow::{anyhow, Result};
use lania_exec::{ExecCommand, ExecResult, ExecService};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use resolve::choose_first_available;

/// 运行时候选程序及其固定参数。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCandidate {
    pub program: &'static str,
    pub args: &'static [&'static str],
}

/// 编译型语言的预处理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepareKind {
    Rustc,
    Cc,
}

/// 单个运行时的静态描述。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub extensions: &'static [&'static str],
    pub candidates: &'static [RuntimeCandidate],
    pub prepare: Option<PrepareKind>,
    pub package_managers: &'static [&'static str],
}

/// 文件类型识别结果（不校验运行时是否安装）。
#[derive(Debug, Clone)]
pub struct DetectedRuntime {
    pub file: String,
    pub language: String,
    pub runtime: String,
}

/// 一次运行的命令计划。
#[derive(Debug, Clone)]
pub struct RunPlan {
    pub language: String,
    pub runtime: String,
    pub program: String,
    pub program_args: Vec<String>,
    pub prepared_command: Option<ExecCommand>,
    pub prepared_summary: Option<String>,
    pub cleanup_path: Option<PathBuf>,
}

/// 运行请求。
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub path: PathBuf,
    pub cwd: PathBuf,
    pub args: Vec<String>,
}

/// 可序列化的命令快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

impl RunCommand {
    fn from_exec(command: &ExecCommand) -> Self {
        Self {
            program: command.program.clone(),
            args: command.args.clone(),
            cwd: command.cwd.clone(),
        }
    }
}

/// 运行结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRunResult {
    pub file: String,
    pub runtime: String,
    pub language: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub prepared_command: Option<RunCommand>,
    pub prepared_summary: Option<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub skipped: bool,
    pub timed_out: bool,
    pub cancelled: bool,
}

/// 无状态运行时服务。
#[derive(Debug, Clone, Default)]
pub struct RuntimeService;

impl RuntimeService {
    pub fn new() -> Self {
        Self
    }

    pub fn list(&self) -> &'static [RuntimeSpec] {
        registry::runtimes()
    }

    pub fn spec(&self, language: &str) -> Option<&'static RuntimeSpec> {
        registry::find_by_language(language)
    }

    /// 识别文件对应的语言与首选运行时，不检查可执行文件是否存在。
    pub fn detect(&self, path: &Path) -> Result<DetectedRuntime> {
        if path.is_file() {
            if let Some(plan) = shebang::read_shebang(path)? {
                return Ok(DetectedRuntime {
                    file: path.display().to_string(),
                    language: plan.language,
                    runtime: plan.runtime,
                });
            }
        }

        let spec = detect_spec(path)?;
        let runtime = spec
            .candidates
            .first()
            .map(|candidate| candidate.program.to_string())
            .unwrap_or_default();
        Ok(DetectedRuntime {
            file: path.display().to_string(),
            language: spec.id.to_string(),
            runtime,
        })
    }

    /// 构造运行计划；shebang 优先，其次按扩展名。
    pub fn plan(&self, path: &Path, cwd: &Path) -> Result<RunPlan> {
        if let Some(plan) = shebang::parse_shebang(path, cwd)? {
            return Ok(plan);
        }

        let spec = detect_spec(path)?;
        match spec.prepare {
            Some(kind) => build_compiled_run_plan(path, cwd, spec, kind),
            None => build_simple_run_plan(path, cwd, spec),
        }
    }

    /// 把运行计划展开成最终执行的命令。
    pub fn build_command(
        &self,
        path: &Path,
        plan: &RunPlan,
        extra_args: &[String],
        cwd: &Path,
    ) -> Result<ExecCommand> {
        let mut args = plan.program_args.clone();
        if plan.prepared_command.is_none() {
            let file = path
                .to_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))?;
            args.push(file);
        }
        args.extend(extra_args.iter().cloned());

        Ok(ExecCommand::new(plan.program.clone())
            .with_args(args)
            .in_dir(cwd.display().to_string()))
    }

    /// 执行脚本；编译型语言的 prepare 与临时产物清理都在这里完成。
    pub fn run(&self, exec: &ExecService, request: &RunRequest) -> Result<RuntimeRunResult> {
        let plan = self.plan(&request.path, &request.cwd)?;
        let command = self.build_command(&request.path, &plan, &request.args, &request.cwd)?;
        let prepared_command = plan.prepared_command.as_ref().map(RunCommand::from_exec);
        let prepared_summary = plan.prepared_summary.clone();
        let prepare = plan.prepared_command.clone();

        let outcome = (|| -> Result<ExecResult> {
            if let Some(prepare) = prepare {
                exec.run_checked(prepare)?;
            }
            exec.run(command.clone())
        })();

        if let Some(cleanup_path) = plan.cleanup_path.as_deref() {
            let _ = fs::remove_file(cleanup_path);
            // rustc 在 Windows 上会在可执行文件旁生成 `.pdb`，一并清理。
            let _ = fs::remove_file(cleanup_path.with_extension("pdb"));
        }

        let result = outcome?;
        Ok(RuntimeRunResult {
            file: request.path.display().to_string(),
            runtime: plan.runtime,
            language: plan.language,
            program: command.program,
            args: command.args,
            cwd: command
                .cwd
                .unwrap_or_else(|| request.cwd.display().to_string()),
            prepared_command,
            prepared_summary,
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            skipped: result.skipped,
            timed_out: result.timed_out,
            cancelled: result.cancelled,
        })
    }
}

fn detect_spec(path: &Path) -> Result<&'static RuntimeSpec> {
    let extension = path
        .extension()
        .and_then(|item| item.to_str())
        .map(|item| item.to_ascii_lowercase());
    let Some(extension) = extension else {
        return Err(unsupported_runtime_error(path));
    };
    registry::find_by_extension(&extension).ok_or_else(|| unsupported_runtime_error(path))
}

fn unsupported_runtime_error(path: &Path) -> anyhow::Error {
    anyhow!(
        "unsupported runnable file type for {} (supported: js/mjs/cjs/ts/tsx/jsx/py/sh/bash/zsh/rb/php/go/java/rs/c/lua/dart/nim/zig/kt/kts/swift)",
        path.display()
    )
}

fn build_simple_run_plan(path: &Path, cwd: &Path, spec: &'static RuntimeSpec) -> Result<RunPlan> {
    let candidates = spec
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.program,
                candidate
                    .args
                    .iter()
                    .map(|item| (*item).to_string())
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    let (program, args) = choose_first_available(path, cwd, &candidates)?;
    Ok(RunPlan {
        language: spec.id.to_string(),
        runtime: display_runtime_name(&program),
        program,
        program_args: args,
        prepared_command: None,
        prepared_summary: None,
        cleanup_path: None,
    })
}

fn build_compiled_run_plan(
    path: &Path,
    cwd: &Path,
    spec: &'static RuntimeSpec,
    prepare_kind: PrepareKind,
) -> Result<RunPlan> {
    let candidates = spec
        .candidates
        .iter()
        .map(|candidate| (candidate.program, Vec::new()))
        .collect::<Vec<_>>();
    let (compiler, mut compiler_args) = choose_first_available(path, cwd, &candidates)?;
    let (label, suffix) = match prepare_kind {
        PrepareKind::Rustc => ("rustc", "rust-source"),
        PrepareKind::Cc => ("cc", "c-source"),
    };
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

    match prepare_kind {
        PrepareKind::Rustc => {
            compiler_args.push(file_text);
            compiler_args.push("--crate-name".into());
            compiler_args.push(sanitize_rust_crate_name(path));
            compiler_args.push("-o".into());
            compiler_args.push(output_text.clone());
        }
        PrepareKind::Cc => {
            compiler_args.push(file_text);
            compiler_args.push("-o".into());
            compiler_args.push(output_text.clone());
        }
    }

    let prepared_command = ExecCommand::new(compiler)
        .with_args(compiler_args)
        .in_dir(cwd.display().to_string());

    Ok(RunPlan {
        language: spec.id.to_string(),
        runtime: label.to_string(),
        program: output_text,
        program_args: Vec::new(),
        prepared_summary: Some(format!("compile {} before execution", path.display())),
        prepared_command: Some(prepared_command),
        cleanup_path: Some(output),
    })
}

fn compiled_output_path(path: &Path, suffix: &str) -> Result<PathBuf> {
    let stem = path
        .file_stem()
        .and_then(|item| item.to_str())
        .unwrap_or("tool-run");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
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

pub(crate) fn display_runtime_name(program: &str) -> String {
    let name = PathBuf::from(program)
        .file_name()
        .and_then(|item| item.to_str())
        .unwrap_or(program)
        .to_string();
    #[cfg(windows)]
    {
        // Windows 可执行文件通常带 `.exe` / `.cmd`，展示时不保留这些后缀。
        let path = PathBuf::from(&name);
        if let Some(extension) = path.extension().and_then(|item| item.to_str()) {
            if matches!(
                extension.to_ascii_lowercase().as_str(),
                "exe" | "cmd" | "bat" | "com"
            ) {
                if let Some(stem) = path.file_stem().and_then(|item| item.to_str()) {
                    return stem.to_string();
                }
            }
        }
    }
    name
}
