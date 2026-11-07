use anyhow::{Context, Result};
use std::{fs, path::Path};

use super::{HexPreview, RenderedText, ViewMatcher};

pub(super) fn render_with_line_numbers(
    content: &str,
    line_limit: usize,
    start: usize,
    end: Option<usize>,
    tail: Option<usize>,
    head: Option<usize>,
    matcher: Option<&ViewMatcher>,
) -> RenderedText {
    let lines = content.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
    render_lines_with_numbers(lines, line_limit, start, end, tail, head, matcher)
}

pub(super) fn render_lines_with_numbers(
    lines: Vec<String>,
    line_limit: usize,
    start: usize,
    end: Option<usize>,
    tail: Option<usize>,
    head: Option<usize>,
    matcher: Option<&ViewMatcher>,
) -> RenderedText {
    let filtered = lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| (index + 1, line))
        .filter(|(_, line)| match matcher {
            Some(pattern) => pattern.matches(line),
            None => true,
        })
        .collect::<Vec<_>>();
    let total_lines = filtered.len();

    let (effective_start, effective_end) = if let Some(tail_count) = tail {
        let start_line = total_lines
            .saturating_sub(tail_count)
            .saturating_add(1)
            .max(1);
        (start_line, total_lines.max(1))
    } else if let Some(head_count) = head {
        (1, head_count.min(total_lines.max(1)))
    } else {
        let start_line = start.max(1);
        let end_line = end.unwrap_or(total_lines).max(start_line);
        (start_line, end_line.min(total_lines.max(1)))
    };

    let mut rendered = Vec::new();
    let mut displayed = 0usize;
    for (position, (line_number, line)) in filtered.iter().enumerate() {
        let visible_index = position + 1;
        let rendered_number = if matcher.is_some() {
            *line_number
        } else {
            visible_index
        };
        if visible_index < effective_start || visible_index > effective_end {
            continue;
        }
        if displayed >= line_limit {
            break;
        }
        rendered.push(format!("{:>6} | {}", rendered_number, line));
        displayed += 1;
    }

    let selectable_count = if total_lines == 0 || effective_end < effective_start {
        0
    } else {
        effective_end - effective_start + 1
    };
    RenderedText {
        text: rendered.join(
            "
",
        ),
        displayed_lines: displayed,
        truncated: selectable_count > displayed,
        start_line: if displayed == 0 { 0 } else { effective_start },
        end_line: if displayed == 0 {
            0
        } else {
            effective_start + displayed - 1
        },
        total_count: total_lines,
    }
}

pub(super) fn render_hex_preview(path: &Path, byte_limit: usize) -> Result<HexPreview> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read file {}", path.display()))?;
    let shown = &bytes[..bytes.len().min(byte_limit)];
    let mut lines = Vec::new();
    for (offset, chunk) in shown.chunks(16).enumerate() {
        let absolute = offset * 16;
        let hex = chunk
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii = chunk
            .iter()
            .map(|byte| {
                let ch = *byte as char;
                if ch.is_ascii_graphic() || ch == ' ' {
                    ch
                } else {
                    '.'
                }
            })
            .collect::<String>();
        lines.push(format!("{absolute:08x}  {hex:<47}  {ascii}"));
    }
    Ok(HexPreview {
        text: lines.join(
            "
",
        ),
        line_count: lines.len(),
        truncated: bytes.len() > shown.len(),
    })
}
