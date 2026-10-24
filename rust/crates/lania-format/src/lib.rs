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
            "py" => {
                if !allow_external {
                    return Ok(skipped(path, Some("ruff".into()), "outside root_dir"));
                }
                // 优先 ruff format；不可用时回退 black。
                let result =
                    self.format_with_external(exec, path, content, ExternalFormatter::Ruff)?;
                if result.status == FormatStatus::Skipped
                    && result
                        .message
                        .as_deref()
                        .is_some_and(|message| message.contains("unavailable"))
                {
                    return self.format_with_external(
                        exec,
                        path,
                        content,
                        ExternalFormatter::Black,
                    );
                }
                Ok(result)
            }
            "dart" => {
                if !allow_external {
                    return Ok(skipped(path, Some("dart".into()), "outside root_dir"));
                }
                self.format_with_external(exec, path, content, ExternalFormatter::Dart)
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
            ExternalFormatter::Ruff => {
                ExecCommand::new("ruff").with_args(vec!["format".to_string(), temp_path_string])
            }
            ExternalFormatter::Black => {
                ExecCommand::new("black").with_args(vec!["--quiet".to_string(), temp_path_string])
            }
            ExternalFormatter::Dart => {
                ExecCommand::new("dart").with_args(vec!["format".to_string(), temp_path_string])
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
    Ruff,
    Black,
    Dart,
}

impl ExternalFormatter {
    fn name(&self) -> &'static str {
        match self {
            Self::Prettier => "prettier",
            Self::Gofmt => "gofmt",
            Self::Rustfmt => "rustfmt",
            Self::Ruff => "ruff",
            Self::Black => "black",
            Self::Dart => "dart",
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
    use super::{
        file_ext, format_json, ok, skipped, FormatFileResult, FormatMode, FormatOptions,
        FormatReport, FormatService, FormatStatus,
    };
    use lania_exec::ExecService;
    use lania_fs::PlannedFile;
    use std::path::Path;

    // ── FormatReport ──────────────────────────────────────────────

    #[test]
    fn format_report_counts_formatted() {
        let report = FormatReport {
            results: vec![
                FormatFileResult {
                    path: "a.json".into(),
                    status: FormatStatus::Formatted,
                    tool: None,
                    message: None,
                },
                FormatFileResult {
                    path: "b.json".into(),
                    status: FormatStatus::Skipped,
                    tool: None,
                    message: None,
                },
                FormatFileResult {
                    path: "c.json".into(),
                    status: FormatStatus::Formatted,
                    tool: None,
                    message: None,
                },
            ],
        };
        assert_eq!(report.formatted_count(), 2);
        assert_eq!(report.skipped_count(), 1);
        assert_eq!(report.failed_count(), 0);
    }

    // ── FormatOptions::default ────────────────────────────────────

    #[test]
    fn format_options_default_is_enabled_best_effort() {
        let opts = FormatOptions::default();
        assert_eq!(opts.enabled, true);
        assert_eq!(opts.mode, FormatMode::BestEffort);
        assert_eq!(opts.root_dir, None);
    }

    // ── format_json ───────────────────────────────────────────────

    #[test]
    fn format_json_sorts_keys_and_indents() {
        let result = format_json("{\"b\":1,\"a\":2}");
        assert_eq!(result, Some("{\n  \"a\": 2,\n  \"b\": 1\n}\n".into()));
    }

    #[test]
    fn format_json_handles_empty_object() {
        // serde_json keeps empty objects compact
        let result = format_json("{}");
        assert_eq!(result, Some("{}\n".into()));
    }

    #[test]
    fn format_json_handles_nested_json() {
        let input = "{\"outer\":{\"inner\":1}}";
        let result = format_json(input);
        assert!(result.is_some());
        let out = result.unwrap();
        assert!(out.contains("\"outer\""));
        assert!(out.contains("\"inner\""));
    }

    #[test]
    fn format_json_returns_none_for_invalid_input() {
        assert_eq!(format_json("not json"), None);
        assert_eq!(format_json(""), None);
    }

    #[test]
    fn format_json_preserves_already_pretty_json() {
        let pretty = "{\n  \"a\": 1\n}\n";
        let result = format_json(pretty);
        // Should still produce valid pretty output
        assert!(result.is_some());
    }

    // ── file_ext ──────────────────────────────────────────────────

    #[test]
    fn file_ext_returns_extension_lowercase() {
        assert_eq!(file_ext(Path::new("test.JSON")), Some("json".into()));
        assert_eq!(file_ext(Path::new("test.ts")), Some("ts".into()));
    }

    #[test]
    fn file_ext_returns_none_when_no_extension() {
        assert_eq!(file_ext(Path::new("Makefile")), None);
    }

    // ── ok / skipped helpers ──────────────────────────────────────

    #[test]
    fn ok_creates_formatted_result() {
        let r = ok(Path::new("f.json"), Some("prettier".into()));
        assert_eq!(r.status, FormatStatus::Formatted);
        assert_eq!(r.tool, Some("prettier".into()));
    }

    #[test]
    fn skipped_creates_skipped_result_with_message() {
        let r = skipped(Path::new("f.json"), None, "unsupported");
        assert_eq!(r.status, FormatStatus::Skipped);
        assert_eq!(r.message, Some("unsupported".into()));
    }

    // ── FormatService integration ─────────────────────────────────

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
        assert_eq!(report.failed_count(), 0);
        assert_eq!(files[0].content, "{\n  \"a\": 2,\n  \"b\": 1\n}\n");
    }

    #[test]
    fn format_disabled_returns_empty_report() {
        let exec = ExecService::dry_run();
        let service = FormatService;
        let mut files = vec![PlannedFile {
            path: "demo.json".into(),
            content: "{}".into(),
        }];
        let report = service
            .format_planned_files(
                &exec,
                &mut files,
                &FormatOptions {
                    enabled: false,
                    mode: FormatMode::BestEffort,
                    root_dir: None,
                },
            )
            .expect("format disabled");
        assert_eq!(report.formatted_count(), 0);
        assert_eq!(report.skipped_count(), 0);
        // Content unchanged when disabled
        assert_eq!(files[0].content, "{}");
    }

    #[test]
    fn format_skips_unsupported_file_types() {
        let exec = ExecService::dry_run();
        let service = FormatService;
        let mut files = vec![PlannedFile {
            path: "readme.txt".into(),
            content: "Hello".into(),
        }];
        let report = service
            .format_planned_files(
                &exec,
                &mut files,
                &FormatOptions::default(),
            )
            .expect("format skips unsupported");
        assert_eq!(report.formatted_count(), 0);
        assert_eq!(report.skipped_count(), 1);
        assert!(report.results[0].message.as_deref().unwrap_or("").contains("unsupported file type"));
    }

    #[test]
    fn format_best_effort_does_not_fail_on_missing_formatter() {
        let exec = ExecService::dry_run();
        let service = FormatService;
        let mut files = vec![PlannedFile {
            path: "script.ts".into(),
            content: "let x=1".into(),
        }];
        // BestEffort mode: missing external formatter should skip, not fail
        let report = service
            .format_planned_files(
                &exec,
                &mut files,
                &FormatOptions {
                    enabled: true,
                    mode: FormatMode::BestEffort,
                    root_dir: None,
                },
            )
            .expect("best effort should not fail");
        assert_eq!(report.failed_count(), 0);
    }

    #[test]
    fn python_formatting_is_best_effort_with_or_without_ruff() {
        let exec = ExecService::new(false);
        let service = FormatService;
        let mut files = vec![PlannedFile {
            path: "main.py".into(),
            content: "x=1".into(),
        }];
        let report = service
            .format_planned_files(
                &exec,
                &mut files,
                &FormatOptions {
                    enabled: true,
                    mode: FormatMode::BestEffort,
                    root_dir: None,
                },
            )
            .expect("python formatting should not hard fail");
        assert_eq!(report.failed_count(), 0);
    }

    #[test]
    fn dart_formatting_is_best_effort_with_or_without_sdk() {
        let exec = ExecService::new(false);
        let service = FormatService;
        let mut files = vec![PlannedFile {
            path: "bin/main.dart".into(),
            content: "void main(){print('x');}".into(),
        }];
        let report = service
            .format_planned_files(
                &exec,
                &mut files,
                &FormatOptions {
                    enabled: true,
                    mode: FormatMode::BestEffort,
                    root_dir: None,
                },
            )
            .expect("dart formatting should not hard fail");
        assert_eq!(report.failed_count(), 0);
    }
}
