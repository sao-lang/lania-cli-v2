use anyhow::{bail, Context, Result};
use lania_command::CommandContext;
use regex::RegexBuilder;

use crate::shared::{bool_option, string_option};

use super::{DirectoryEntryFilter, DirectoryEntryKind, DirectorySort, ViewMatcher};

pub(super) fn build_view_matcher(context: &CommandContext) -> Result<Option<ViewMatcher>> {
    let ignore_case = bool_option(context, "ignore-case");
    if let Some(pattern) = string_option(context, "regex") {
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(ignore_case)
            .build()
            .with_context(|| format!("invalid regex pattern: {pattern}"))?;
        return Ok(Some(ViewMatcher::Regex(regex)));
    }
    if let Some(needle) = string_option(context, "grep") {
        return Ok(Some(ViewMatcher::Substring {
            needle,
            ignore_case,
        }));
    }
    Ok(None)
}

pub(super) fn build_entry_filter(
    files_only: bool,
    dirs_only: bool,
) -> Result<DirectoryEntryFilter> {
    match (files_only, dirs_only) {
        (true, true) => bail!("`--files-only` and `--dirs-only` cannot be used together"),
        (true, false) => Ok(DirectoryEntryFilter::FilesOnly),
        (false, true) => Ok(DirectoryEntryFilter::DirsOnly),
        (false, false) => Ok(DirectoryEntryFilter::Any),
    }
}

pub(super) fn build_directory_sort(context: &CommandContext) -> Result<DirectorySort> {
    match string_option(context, "sort").as_deref().unwrap_or("name") {
        "name" => Ok(DirectorySort::Name),
        "size" => Ok(DirectorySort::Size),
        "time" => Ok(DirectorySort::Time),
        "ext" => Ok(DirectorySort::Extension),
        other => bail!("unsupported sort mode `{other}` (expected: name, size, time, ext)"),
    }
}

impl ViewMatcher {
    pub(super) fn matches(&self, text: &str) -> bool {
        match self {
            ViewMatcher::Substring {
                needle,
                ignore_case,
            } => {
                if *ignore_case {
                    text.to_lowercase().contains(&needle.to_lowercase())
                } else {
                    text.contains(needle)
                }
            }
            ViewMatcher::Regex(regex) => regex.is_match(text),
        }
    }
}

impl DirectoryEntryFilter {
    pub(super) fn matches(self, kind: DirectoryEntryKind) -> bool {
        match self {
            DirectoryEntryFilter::Any => true,
            DirectoryEntryFilter::FilesOnly => matches!(kind, DirectoryEntryKind::File),
            DirectoryEntryFilter::DirsOnly => matches!(kind, DirectoryEntryKind::Dir),
        }
    }
}
