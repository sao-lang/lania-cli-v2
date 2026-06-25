//! Code formatting helpers for generated outputs.
//!
//! Goal: best-effort formatting that can be applied to `PlannedFile` before writing.
//! - JSON: formatted in-process (stable and fast).
//! - Others: optional external formatters (prettier/gofmt/rustfmt) via `lania-exec`.
//!
//! This crate avoids requiring stdin piping support from `ExecService` by formatting via temp files.
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use lania_exec::{ExecCommand, ExecService};
use lania_fs::PlannedFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMode {
    /// Never fail the caller; keep original content on any error.
    BestEffort,
    /// Fail the caller when a formatter for a supported file type fails.
    Strict,
}

#[derive(Debug, Clone)]
pub struct FormatOptions {
    pub enabled: bool,
    pub mode: FormatMode,
    /// If set, only files under this directory are considered for external formatting.
    /// (Useful to avoid formatting paths outside workspace accidentally.)
    pub root_dir: Option<PathBuf>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: FormatMode::BestEffort,
            root_dir: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatStatus {
    Formatted,
    Skipped,
    Failed,
}

#[derive(Debug, Clone)]
pub struct FormatFileResult {
    pub path: PathBuf,
    pub status: FormatStatus,
    pub tool: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FormatReport {
    pub results: Vec<FormatFileResult>,
}

impl FormatReport {
    pub fn formatted_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status == FormatStatus::Formatted)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status == FormatStatus::Failed)
            .count()
    }

    pub fn skipped_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status == FormatStatus::Skipped)
            .count()
    }
}

#[derive(Debug, Clone, Default)]
pub struct FormatService;

impl FormatService {
    pub fn format_planned_files(
        &self,
        exec: &ExecService,
        files: &mut [PlannedFile],
        options: &FormatOptions,
    ) -> Result<FormatReport> {
        if !options.enabled {
            return Ok(FormatReport::default());
        }

        let mut report = FormatReport::default();
        for file in files.iter_mut() {
            let result = self.format_one(exec, &file.path, &mut file.content, options);
            match result {
                Ok(entry) => report.results.push(entry),
                Err(error) => {
                    // Strict mode: surface error. Best-effort: keep content.
                    if matches!(options.mode, FormatMode::Strict) {
                        return Err(error);
                    }
                    report.results.push(FormatFileResult {
                        path: file.path.clone(),
                        status: FormatStatus::Failed,
                        tool: None,
                        message: Some(error.to_string()),
                    });
                }
            }
        }
        Ok(report)
    }

    fn format_one(
        &self,
        exec: &ExecService,
        path: &Path,
        content: &mut String,
        options: &FormatOptions,
    ) -> Result<FormatFileResult> {
        // Root guard for external formatting only.
        let allow_external = options
            .root_dir
            .as_ref()
            .map(|root| path.starts_with(root))
            .unwrap_or(true);

        let ext = file_ext(path).unwrap_or_default();
        match ext.as_str() {
            "json" => {
                if let Some(formatted) = format_json(content) {
                    if &formatted != content {
                        *content = formatted;
                        return Ok(ok(path, Some("json".into())));
                    }
                }
                Ok(skipped(
                    path,
                    Some("json".into()),
                    "not valid json or no changes",
                ))
            }
            // Use prettier for common web formats if available.
            "js" | "jsx" | "ts" | "tsx" | "css" | "scss" | "md" | "yaml" | "yml" => {
                if !allow_external {
                    return Ok(skipped(path, Some("prettier".into()), "outside root_dir"));
                }
                self.format_with_external(exec, path, content, ExternalFormatter::Prettier)
            }
            "go" => {
                if !allow_external {
                    return Ok(skipped(path, Some("gofmt".into()), "outside root_dir"));
                }
                self.format_with_external(exec, path, content, ExternalFormatter::Gofmt)
            }
            "rs" => {
                if !allow_external {
                    return Ok(skipped(path, Some("rustfmt".into()), "outside root_dir"));
                }
                self.format_with_external(exec, path, content, ExternalFormatter::Rustfmt)
            }
            _ => Ok(skipped(path, None, "unsupported file type")),
        }
    }

    fn format_with_external(
        &self,
        exec: &ExecService,
        path: &Path,
        content: &mut String,
        formatter: ExternalFormatter,
    ) -> Result<FormatFileResult> {
        // Work on a temp file to avoid needing stdin piping support.
        let ext = file_ext(path).unwrap_or_default();
        let temp = temp_path("lania-format", &ext);
        if let Some(parent) = temp.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create temp dir {}", parent.display()))?;
        }
        fs::write(&temp, content.as_bytes())
            .with_context(|| format!("failed to write temp file {}", temp.display()))?;

        let temp_path_string = temp.display().to_string();
        let command = match formatter {
            ExternalFormatter::Prettier => ExecCommand::new("prettier")
                .with_args(vec!["--write".to_string(), temp_path_string]),
            ExternalFormatter::Gofmt => {
                ExecCommand::new("gofmt").with_args(vec!["-w".to_string(), temp_path_string])
            }
            ExternalFormatter::Rustfmt => {
                ExecCommand::new("rustfmt").with_args(vec![temp_path_string])
            }
        };

        // Best-effort: if command cannot spawn, treat as "skipped".
        let run = exec.run(command);
        match run {
            Ok(result) => {
                if !result.skipped && result.exit_code != 0 {
                    // formatter ran but failed
                    anyhow::bail!(
                        "{} failed with exit code {}: {}",
                        formatter.name(),
                        result.exit_code,
                        result.stderr
                    );
                }
            }
            Err(error) => {
                // If the binary is missing or spawn fails, do not hard fail in best-effort mode.
                // Keep content unchanged.
                return Ok(skipped(
                    path,
                    Some(formatter.name().into()),
                    &format!("formatter unavailable: {error}"),
                ));
            }
        }

        let formatted = fs::read_to_string(&temp)
            .with_context(|| format!("failed to read formatted temp file {}", temp.display()))?;
        let _ = fs::remove_file(&temp);

        if &formatted != content {
            *content = formatted;
            Ok(ok(path, Some(formatter.name().into())))
        } else {
            Ok(skipped(path, Some(formatter.name().into()), "no changes"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalFormatter {
    Prettier,
    Gofmt,
    Rustfmt,
}

impl ExternalFormatter {
    fn name(&self) -> &'static str {
        match self {
            Self::Prettier => "prettier",
            Self::Gofmt => "gofmt",
            Self::Rustfmt => "rustfmt",
        }
    }
}

fn ok(path: &Path, tool: Option<String>) -> FormatFileResult {
    FormatFileResult {
        path: path.to_path_buf(),
        status: FormatStatus::Formatted,
        tool,
        message: None,
    }
}

fn skipped(path: &Path, tool: Option<String>, message: &str) -> FormatFileResult {
    FormatFileResult {
        path: path.to_path_buf(),
        status: FormatStatus::Skipped,
        tool,
        message: Some(message.to_string()),
    }
}

fn file_ext(path: &Path) -> Option<String> {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|s| s.to_ascii_lowercase())
}

fn temp_path(prefix: &str, ext: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    let file_name = if ext.is_empty() {
        format!("{prefix}-{unique}")
    } else {
        format!("{prefix}-{unique}.{ext}")
    };
    std::env::temp_dir().join(file_name)
}

fn format_json(input: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(input).ok()?;
    let mut out = serde_json::to_string_pretty(&value).ok()?;
    out.push('\n');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{FormatMode, FormatOptions, FormatService};
    use lania_exec::ExecService;
    use lania_fs::PlannedFile;

    #[test]
    fn formats_json_in_process() {
        let exec = ExecService::dry_run();
        let service = FormatService;
        let mut files = vec![PlannedFile {
            path: "demo.json".into(),
            content: "{\"b\":1,\"a\":2}".into(),
        }];
        let report = service
            .format_planned_files(
                &exec,
                &mut files,
                &FormatOptions {
                    enabled: true,
                    mode: FormatMode::Strict,
                    root_dir: None,
                },
            )
            .expect("format succeeds");
        assert_eq!(report.formatted_count(), 1);
        assert_eq!(files[0].content, "{\n  \"a\": 2,\n  \"b\": 1\n}\n");
    }
}
