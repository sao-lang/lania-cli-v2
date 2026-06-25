//! 目录浏览与 tree 渲染逻辑。
//!
//! 这不是简单的 `read_dir` 包装，而是把目录内容整理成适合 CLI 输出的文本：
//! - 普通列表模式：逐项展示类型、名称和大小
//! - tree 模式：递归展开目录结构，并在过滤后尽量保留层级上下文
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    render::render_lines_with_numbers, DirectoryEntryFilter, DirectoryEntryKind, DirectorySort,
    RenderedText, TreeEntry, ViewMatcher,
};

#[derive(Debug)]
struct DirectoryListingEntry {
    entry: fs::DirEntry,
    metadata: fs::Metadata,
}

pub(super) struct DirectoryListingOptions<'a> {
    pub(super) line_limit: usize,
    pub(super) start: usize,
    pub(super) end: Option<usize>,
    pub(super) tail: Option<usize>,
    pub(super) head: Option<usize>,
    pub(super) matcher: Option<&'a ViewMatcher>,
    pub(super) tree: bool,
    pub(super) max_depth: Option<usize>,
    pub(super) sort: DirectorySort,
    pub(super) reverse: bool,
    pub(super) show_hidden: bool,
    pub(super) entry_filter: DirectoryEntryFilter,
}

pub(super) fn render_directory_listing(
    path: &Path,
    options: DirectoryListingOptions<'_>,
) -> Result<RenderedText> {
    // 普通列表和 tree 模式共用同一套最终分页/裁剪渲染，
    // 区别只在于前面如何收集并组织 `lines`。
    let mut lines = Vec::new();
    let render_matcher = if options.tree {
        let entries = collect_tree_entries(
            path,
            path,
            options.max_depth,
            options.sort,
            options.reverse,
            options.show_hidden,
        )?;
        flatten_tree_entries(&entries, options.matcher, options.entry_filter, &mut lines);
        None
    } else {
        let entries = read_sorted_directory_entries(
            path,
            options.sort,
            options.reverse,
            options.show_hidden,
        )?;
        for entry in entries {
            let kind = classify_directory_entry(&entry.metadata);
            if !options.entry_filter.matches(kind) {
                continue;
            }
            let name = entry.entry.file_name().to_string_lossy().into_owned();
            lines.push(render_listing_line(&name, &entry.metadata, kind));
        }
        options.matcher
    };

    Ok(render_lines_with_numbers(
        lines,
        options.line_limit,
        options.start,
        options.end,
        options.tail,
        options.head,
        render_matcher,
    ))
}

fn render_listing_line(name: &str, metadata: &fs::Metadata, kind: DirectoryEntryKind) -> String {
    let type_name = directory_entry_kind_name(kind);
    let suffix = if metadata.is_dir() { "/" } else { "" };
    format!("[{type_name}] {name}{suffix} ({})", metadata.len())
}

fn read_sorted_directory_entries(
    current: &Path,
    sort: DirectorySort,
    reverse: bool,
    show_hidden: bool,
) -> Result<Vec<DirectoryListingEntry>> {
    // 目录项先完整枚举再排序，保证 name/size/time/extension 四种排序行为一致。
    let mut entries = fs::read_dir(current)
        .with_context(|| format!("failed to read directory {}", current.display()))?
        .map(|entry| {
            let entry = entry?;
            let metadata = entry.metadata()?;
            Ok(DirectoryListingEntry { entry, metadata })
        })
        .collect::<std::result::Result<Vec<_>, std::io::Error>>()
        .with_context(|| format!("failed to enumerate directory {}", current.display()))?;

    if !show_hidden {
        entries.retain(|entry| !entry.entry.file_name().to_string_lossy().starts_with('.'));
    }

    entries.sort_by(|left, right| compare_directory_entries(left, right, sort));
    if reverse {
        entries.reverse();
    }
    Ok(entries)
}

fn collect_tree_entries(
    root: &Path,
    current: &Path,
    max_depth: Option<usize>,
    sort: DirectorySort,
    reverse: bool,
    show_hidden: bool,
) -> Result<Vec<TreeEntry>> {
    collect_tree_entries_inner(
        root,
        current,
        Vec::new(),
        max_depth,
        sort,
        reverse,
        show_hidden,
    )
}

fn collect_tree_entries_inner(
    root: &Path,
    current: &Path,
    lineage: Vec<bool>,
    max_depth: Option<usize>,
    sort: DirectorySort,
    reverse: bool,
    show_hidden: bool,
) -> Result<Vec<TreeEntry>> {
    // tree 收集时记录从 root 到当前节点的 lineage，
    // 用它决定每一层该渲染空格、竖线还是末尾分支符号。
    let entries = read_sorted_directory_entries(current, sort, reverse, show_hidden)?;
    let entry_count = entries.len();
    let mut nodes = Vec::new();

    for (index, entry) in entries.into_iter().enumerate() {
        let entry_path = entry.entry.path();
        let metadata = entry.metadata;
        let relative = entry_path.strip_prefix(root).unwrap_or(&entry_path);
        let name = relative.display().to_string();
        let is_last = index + 1 == entry_count;
        let kind = classify_directory_entry(&metadata);
        let children = if metadata.is_dir() && max_depth.map(|limit| limit > 0).unwrap_or(true) {
            let mut child_lineage = lineage.clone();
            child_lineage.push(is_last);
            collect_tree_entries_inner(
                root,
                &entry_path,
                child_lineage,
                max_depth.map(|limit| limit.saturating_sub(1)),
                sort,
                reverse,
                show_hidden,
            )?
        } else {
            Vec::new()
        };

        nodes.push(TreeEntry {
            line: format!(
                "{}{}",
                tree_prefix(&lineage, is_last),
                render_listing_line(&name, &metadata, kind),
            ),
            kind,
            children,
        });
    }
    Ok(nodes)
}

fn tree_prefix(lineage: &[bool], is_last: bool) -> String {
    let mut prefix = String::new();
    for ancestor_is_last in lineage {
        if *ancestor_is_last {
            prefix.push_str("    ");
        } else {
            prefix.push_str("│   ");
        }
    }
    prefix.push_str(if is_last { "└── " } else { "├── " });
    prefix
}

fn compare_directory_entries(
    left: &DirectoryListingEntry,
    right: &DirectoryListingEntry,
    sort: DirectorySort,
) -> std::cmp::Ordering {
    let left_name = left.entry.file_name().to_string_lossy().into_owned();
    let right_name = right.entry.file_name().to_string_lossy().into_owned();
    match sort {
        DirectorySort::Name => left_name.cmp(&right_name),
        DirectorySort::Size => right
            .metadata
            .len()
            .cmp(&left.metadata.len())
            .then_with(|| left_name.cmp(&right_name)),
        DirectorySort::Time => right
            .metadata
            .modified()
            .ok()
            .cmp(&left.metadata.modified().ok())
            .then_with(|| left_name.cmp(&right_name)),
        DirectorySort::Extension => extension_key(&left_name)
            .cmp(&extension_key(&right_name))
            .then_with(|| left_name.cmp(&right_name)),
    }
}

fn extension_key(name: &str) -> (String, String) {
    let path = PathBuf::from(name);
    let ext = path
        .extension()
        .and_then(|item| item.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    (ext, name.to_ascii_lowercase())
}

fn flatten_tree_entries(
    entries: &[TreeEntry],
    matcher: Option<&ViewMatcher>,
    entry_filter: DirectoryEntryFilter,
    lines: &mut Vec<String>,
) -> bool {
    // 过滤 tree 输出时，如果子节点命中而父节点本身没命中，
    // 仍尽量保留祖先目录，避免结果失去层级定位信息。
    let mut kept_any = false;
    for entry in entries {
        let mut child_lines = Vec::new();
        let child_kept =
            flatten_tree_entries(&entry.children, matcher, entry_filter, &mut child_lines);
        let self_matches_text = matcher
            .map(|pattern| pattern.matches(&entry.line))
            .unwrap_or(true);
        let self_matches_type = entry_filter.matches(entry.kind);
        let include_self = self_matches_text && self_matches_type;

        if include_self {
            lines.push(entry.line.clone());
            lines.extend(child_lines);
            kept_any = true;
        } else if child_kept {
            // 过滤树形输出时保留祖先目录，保证仍能看出命中的层级位置。
            if matches!(entry_filter, DirectoryEntryFilter::FilesOnly) {
                lines.extend(child_lines);
            } else {
                lines.push(entry.line.clone());
                lines.extend(child_lines);
            }
            kept_any = true;
        }
    }
    kept_any
}

fn classify_directory_entry(metadata: &fs::Metadata) -> DirectoryEntryKind {
    if metadata.is_dir() {
        DirectoryEntryKind::Dir
    } else if metadata.is_file() {
        DirectoryEntryKind::File
    } else {
        DirectoryEntryKind::Other
    }
}

fn directory_entry_kind_name(kind: DirectoryEntryKind) -> &'static str {
    match kind {
        DirectoryEntryKind::File => "file",
        DirectoryEntryKind::Dir => "dir",
        DirectoryEntryKind::Other => "other",
    }
}
