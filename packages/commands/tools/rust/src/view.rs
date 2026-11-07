//! `tools view` 的文本预览、目录渲染与媒体打开逻辑。

mod directory;
mod file_kind;
mod matcher;
mod open;
mod render;

#[cfg(test)]
mod tests;

use anyhow::{bail, Context, Result};
use lania_command::{ArgSpec, CommandSpec, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext},
};
use serde_json::json;
use std::fs;

use self::{
    directory::{render_directory_listing, DirectoryListingOptions},
    file_kind::{inspect_view_file_kind, view_kind_name},
    matcher::{build_directory_sort, build_entry_filter, build_view_matcher},
    open::open_with_system_command,
    render::{render_hex_preview, render_with_line_numbers},
};
use crate::{
    shared::{bool_option, numeric_option, required_arg, resolve_from_cwd},
    VIEW_HANDLER_ID,
};

const TEXT_LINE_LIMIT: usize = 2000;
const BINARY_SNIFF_LIMIT: usize = 4096;
const HEX_PREVIEW_BYTES: usize = 256;

pub(super) fn spec() -> CommandSpec {
    CommandSpec::new(
        "view",
        "Show file contents or open media with the system app",
        VIEW_HANDLER_ID,
    )
    .with_args(vec![ArgSpec {
        name: "path".into(),
        required: true,
        multiple: false,
        help: "Path to the file to inspect".into(),
    }])
    .with_options(vec![
        OptionSpec {
            long: "lines".into(),
            short: Some('n'),
            help: "Limit the number of displayed text lines".into(),
            value_kind: ValueKind::Number,
            default_value: Some(TEXT_LINE_LIMIT.to_string()),
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "start".into(),
            short: Some('s'),
            help: "Start viewing from a specific 1-based line number".into(),
            value_kind: ValueKind::Number,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "end".into(),
            short: Some('e'),
            help: "End viewing at a specific 1-based line number".into(),
            value_kind: ValueKind::Number,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "tail".into(),
            short: Some('t'),
            help: "Show the last N lines of a text file".into(),
            value_kind: ValueKind::Number,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "head".into(),
            short: Some('H'),
            help: "Show the first N lines of a text file".into(),
            value_kind: ValueKind::Number,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "grep".into(),
            short: Some('g'),
            help: "Filter visible text or directory entries by substring".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "regex".into(),
            short: Some('r'),
            help: "Filter visible text or directory entries by regular expression".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "ignore-case".into(),
            short: Some('i'),
            help: "Enable case-insensitive matching for grep or regex".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "tree".into(),
            short: None,
            help: "Render directories as a recursive tree".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "max-depth".into(),
            short: None,
            help: "Limit recursive directory traversal depth".into(),
            value_kind: ValueKind::Number,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "sort".into(),
            short: None,
            help: "Sort directory entries by name, size, or time".into(),
            value_kind: ValueKind::String,
            default_value: Some("name".into()),
            choices: vec!["name".into(), "size".into(), "time".into(), "ext".into()],
            negatable: false,
        },
        OptionSpec {
            long: "reverse".into(),
            short: None,
            help: "Reverse directory sort order".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "hidden".into(),
            short: None,
            help: "Include hidden files and directories".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "files-only".into(),
            short: None,
            help: "Only include file entries when viewing directories".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "dirs-only".into(),
            short: None,
            help: "Only include directory entries when viewing directories".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "hex-bytes".into(),
            short: None,
            help: "Limit binary hex preview bytes".into(),
            value_kind: ValueKind::Number,
            default_value: Some(HEX_PREVIEW_BYTES.to_string()),
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "meta-only".into(),
            short: None,
            help: "Only show file metadata without opening external viewers".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
    ])
}

pub(super) fn execute(ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
    ctx.require_capability(CapabilityName::Exec)?;
    ctx.require_capability(CapabilityName::Fs)?;

    let path = required_arg(ctx.command(), "path")?;
    let resolved = resolve_from_cwd(&ctx.command().cwd, &path);
    let metadata = fs::metadata(&resolved)
        .with_context(|| format!("failed to read metadata for {}", resolved.display()))?;
    let size_bytes = metadata.len();
    let meta_only = bool_option(ctx.command(), "meta-only");
    let line_limit = numeric_option(ctx.command(), "lines").unwrap_or(TEXT_LINE_LIMIT);
    let start = numeric_option(ctx.command(), "start").unwrap_or(1);
    let end = numeric_option(ctx.command(), "end");
    let tail = numeric_option(ctx.command(), "tail");
    let head = numeric_option(ctx.command(), "head");
    let tree = bool_option(ctx.command(), "tree");
    let max_depth = numeric_option(ctx.command(), "max-depth");
    let sort = build_directory_sort(ctx.command())?;
    let reverse = bool_option(ctx.command(), "reverse");
    let show_hidden = bool_option(ctx.command(), "hidden");
    let files_only = bool_option(ctx.command(), "files-only");
    let dirs_only = bool_option(ctx.command(), "dirs-only");
    let entry_filter = build_entry_filter(files_only, dirs_only)?;
    let matcher = build_view_matcher(ctx.command())?;
    let matcher_ref = matcher.as_ref();
    let hex_bytes = numeric_option(ctx.command(), "hex-bytes").unwrap_or(HEX_PREVIEW_BYTES);

    if metadata.is_dir() {
        let directory = render_directory_listing(
            &resolved,
            DirectoryListingOptions {
                line_limit,
                start,
                end,
                tail,
                head,
                matcher: matcher_ref,
                tree,
                max_depth,
                sort,
                reverse,
                show_hidden,
                entry_filter,
            },
        )?;
        let output = json!({
            "kind": "tool_view_result",
            "path": resolved.display().to_string(),
            "mode": "directory",
            "mediaType": "directory",
            "sizeBytes": size_bytes,
            "lineCount": directory.total_count,
            "startLine": directory.start_line,
            "endLine": directory.end_line,
            "displayedLines": directory.displayed_lines,
            "truncated": directory.truncated,
            "content": directory.text,
        });
        return Ok(ctx.complete_template_info(output, 0));
    }

    if !metadata.is_file() {
        bail!(
            "{} is neither a regular file nor a directory",
            resolved.display()
        );
    }

    let file_kind = inspect_view_file_kind(&resolved)?;
    if !matches!(file_kind, ViewFileKind::Text) {
        return handle_non_text_file(ctx, &resolved, size_bytes, file_kind, meta_only, hex_bytes);
    }

    let content = fs::read_to_string(&resolved)
        .with_context(|| format!("failed to read text file {}", resolved.display()))?;
    let rendered =
        render_with_line_numbers(&content, line_limit, start, end, tail, head, matcher_ref);
    let total_lines = content.lines().count();
    let output = json!({
        "kind": "tool_view_result",
        "path": resolved.display().to_string(),
        "mode": "text",
        "mediaType": "text",
        "sizeBytes": size_bytes,
        "lineCount": total_lines,
        "startLine": rendered.start_line,
        "endLine": rendered.end_line,
        "displayedLines": rendered.displayed_lines,
        "truncated": rendered.truncated,
        "content": rendered.text,
    });
    Ok(ctx.complete_template_info(output, 0))
}

fn handle_non_text_file(
    ctx: &CommandExecutionContext<'_>,
    path: &std::path::Path,
    size_bytes: u64,
    file_kind: ViewFileKind,
    meta_only: bool,
    hex_bytes: usize,
) -> Result<CommandExecution> {
    let media_type = view_kind_name(&file_kind);
    if matches!(file_kind, ViewFileKind::Binary) {
        let preview = render_hex_preview(path, hex_bytes)?;
        let output = json!({
            "kind": "tool_view_result",
            "path": path.display().to_string(),
            "mode": "hex",
            "mediaType": media_type,
            "sizeBytes": size_bytes,
            "lineCount": preview.line_count,
            "startLine": 0,
            "endLine": 0,
            "displayedLines": preview.line_count,
            "truncated": preview.truncated,
            "content": preview.text,
        });
        return Ok(ctx.complete_template_info(output, 0));
    }

    if meta_only {
        let output = json!({
            "kind": "tool_open_result",
            "path": path.display().to_string(),
            "mode": "metadata",
            "mediaType": media_type,
            "sizeBytes": size_bytes,
            "opened": false,
            "viewer": "system_default",
        });
        return Ok(ctx.complete_template_info(output, 0));
    }

    let command = open_with_system_command(path, &ctx.command().cwd)?;
    let exec_result = ctx.exec().run(command.clone())?;
    let output = json!({
        "kind": "tool_open_result",
        "path": path.display().to_string(),
        "mode": "external",
        "mediaType": media_type,
        "sizeBytes": size_bytes,
        "program": command.program,
        "args": command.args,
        "exitCode": exec_result.exit_code,
        "viewer": "system_default",
        "opened": true,
    });
    Ok(ctx.complete_template_info(output, exec_result.exit_code))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedText {
    text: String,
    displayed_lines: usize,
    truncated: bool,
    start_line: usize,
    end_line: usize,
    total_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HexPreview {
    text: String,
    line_count: usize,
    truncated: bool,
}

#[derive(Debug, Clone)]
enum ViewMatcher {
    Substring { needle: String, ignore_case: bool },
    Regex(regex::Regex),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryEntryFilter {
    Any,
    FilesOnly,
    DirsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectorySort {
    Name,
    Size,
    Time,
    Extension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewFileKind {
    Text,
    Image,
    Video,
    Audio,
    Pdf,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeEntry {
    line: String,
    kind: DirectoryEntryKind,
    children: Vec<TreeEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryEntryKind {
    File,
    Dir,
    Other,
}
