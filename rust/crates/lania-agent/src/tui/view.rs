//! 渲染：把 [`App`] 画成一帧。只依赖 ratatui，不读终端、不做状态迁移。
//!
//! 手工按显示宽度折行（`unicode-width`），而不是交给 `Paragraph::wrap`：
//! 这样「滚动 N 行」是精确的，中文/emoji 也不会按 1 个 cell 错算。

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Row, ToolState};
use crate::runtime::NoticeLevel;

const ACCENT: Color = Color::Cyan;
const OK_COLOR: Color = Color::Green;
const FAIL_COLOR: Color = Color::Red;
const WARN_COLOR: Color = Color::Yellow;
const MUTED: Color = Color::DarkGray;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < 40 || area.height < 10 {
        let message = Paragraph::new(format!(
            "terminal too small ({}×{}), need at least 40×10",
            area.width, area.height
        ))
        .style(Style::default().fg(WARN_COLOR))
        .alignment(Alignment::Center);
        frame.render_widget(message, area);
        return;
    }

    let composer_lines = app.composer.text.lines().count().clamp(1, 5) as u16;
    let chunks = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(composer_lines + 2),
        Constraint::Length(1),
    ])
    .split(area);

    draw_transcript(frame, app, chunks[0]);
    draw_composer(frame, app, chunks[1]);
    draw_status(frame, app, chunks[2]);

    if let Some(dialog) = app.dialog.as_ref() {
        draw_dialog(frame, app, &dialog.request, area);
    }
}

fn draw_transcript(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let width = area.width.saturating_sub(2) as usize;
    let lines = transcript_lines(app, width);
    let height = area.height.saturating_sub(2) as usize;
    // 跟随末尾时 offset = 内容高度 - 视口高度；向上滚动时减掉 scroll。
    let base = lines.len().saturating_sub(height);
    let offset = base.saturating_sub(app.scroll);

    // 内容不足一屏时贴着底部显示：终端里「最新输出在下面」比「顶部对齐」更符合直觉。
    let mut lines = lines;
    if offset == 0 && lines.len() < height {
        let mut padding: Vec<Line<'static>> = vec![Line::from(""); height - lines.len()];
        padding.append(&mut lines);
        lines = padding;
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(MUTED)),
        )
        .scroll((offset as u16, 0));
    frame.render_widget(paragraph, area);
}

/// 把缓冲区内容导成纯文本。
///
/// 必须按显示宽度跳过「宽字符占用的第二格」，否则中文在 dump 里会被拆成
/// `把  pi` 这种带空格的样子（真实终端没有这个问题，只有文本导出才有）。
pub fn buffer_to_text(buffer: &ratatui::buffer::Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            let width = UnicodeWidthStr::width(symbol).max(1) as u16;
            out.push_str(symbol);
            x += width;
        }
        out.push('\n');
    }
    out
}

/// 把行模型折成终端行。`pub(crate)` 方便 snapshot 测试直接比对。
pub fn transcript_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let width = width.max(20);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for row in &app.rows {
        match row {
            Row::User(text) => {
                push_wrapped(
                    &mut lines,
                    text,
                    width,
                    "› ",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                );
            }
            Row::Assistant(text) => {
                push_wrapped(&mut lines, text, width, "  ", Style::default());
            }
            Row::Thinking(text) => {
                push_wrapped(
                    &mut lines,
                    text,
                    width,
                    "✻ ",
                    Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                );
            }
            Row::Tool(tool) => {
                let (glyph, color) = match tool.state {
                    ToolState::Running => ("⏺", ACCENT),
                    ToolState::Ok => ("✔", OK_COLOR),
                    ToolState::Failed => ("✘", FAIL_COLOR),
                };
                let mut spans = vec![
                    Span::styled(format!("{glyph} "), Style::default().fg(color)),
                    Span::styled(tool.name.clone(), Style::default().fg(color)),
                ];
                if !tool.argument.is_empty() {
                    spans.push(Span::styled(
                        format!(" {}", tool.argument),
                        Style::default().fg(MUTED),
                    ));
                }
                lines.push(Line::from(spans));
                if let Some(detail) = tool.detail.as_ref() {
                    for line in wrap_plain(detail, width.saturating_sub(4)) {
                        lines.push(Line::from(Span::styled(
                            format!("    {line}"),
                            Style::default().fg(MUTED),
                        )));
                    }
                }
            }
            Row::Notice(level, text) => {
                let (prefix, color) = match level {
                    NoticeLevel::Info => ("· ", MUTED),
                    NoticeLevel::Warn => ("! ", WARN_COLOR),
                };
                push_wrapped(&mut lines, text, width, prefix, Style::default().fg(color));
            }
            Row::Error(text) => {
                push_wrapped(
                    &mut lines,
                    text,
                    width,
                    "✘ ",
                    Style::default().fg(FAIL_COLOR),
                );
            }
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ask something · /help for commands",
            Style::default().fg(MUTED),
        )));
    }
    lines
}

fn push_wrapped(
    lines: &mut Vec<Line<'static>>,
    text: &str,
    width: usize,
    prefix: &str,
    style: Style,
) {
    let available = width.saturating_sub(display_width(prefix)).max(10);
    let wrapped = wrap_plain(text, available);
    for (index, chunk) in wrapped.iter().enumerate() {
        let lead = if index == 0 {
            prefix.to_string()
        } else {
            " ".repeat(display_width(prefix))
        };
        lines.push(Line::from(Span::styled(format!("{lead}{chunk}"), style)));
    }
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// 按显示宽度折行，保留原有换行。
fn wrap_plain(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out = Vec::new();
    for raw in text.split('\n') {
        if raw.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut used = 0usize;
        for ch in raw.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + w > width && !current.is_empty() {
                out.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push(ch);
            used += w;
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn draw_composer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let title = if app.busy {
        format!(" {} working… (esc to interrupt) ", app.spinner_glyph())
    } else {
        " input ".to_string()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MUTED))
        .title(title);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if app.composer.text.is_empty() {
        lines.push(Line::from(Span::styled(
            if app.busy {
                "queued prompts run after the current turn"
            } else {
                "type a prompt · ctrl-j newline · /help"
            },
            Style::default().fg(MUTED),
        )));
    } else {
        for line in app.composer.text.split('\n') {
            lines.push(Line::from(Span::styled(line.to_string(), Style::default())));
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
    place_cursor(frame, app, area);
}

/// 光标位置：按 `cursor` 的 char 索引 + 已有换行推算行列。
fn place_cursor(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let text = &app.composer.text;
    let chars: Vec<char> = text.chars().collect();
    let cursor = app.composer.cursor.min(chars.len());
    let before: String = chars[..cursor].iter().collect();
    let row = before.matches('\n').count();
    let line_start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
    let column: usize = before[line_start..]
        .chars()
        .map(|ch| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1))
        .sum();

    let inner_x = area.x.saturating_add(1);
    let inner_y = area.y.saturating_add(1);
    let max_y = area.bottom().saturating_sub(2);
    let max_x = area.right().saturating_sub(1);
    let x = (inner_x + column as u16).min(max_x);
    let y = (inner_y + row as u16).min(max_y);
    frame.set_cursor_position((x, y));
}

fn draw_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let left = if let Some(hint) = app.hint.as_ref() {
        hint.clone()
    } else {
        app.status_line()
    };
    let right = if app.busy {
        format!("{} ", app.spinner_glyph())
    } else {
        String::new()
    };
    let line = Line::from(vec![
        Span::styled(format!(" {left}"), Style::default().fg(MUTED)),
        Span::styled(right, Style::default().fg(ACCENT)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_dialog(
    frame: &mut Frame<'_>,
    app: &App,
    request: &crate::approval::ApprovalRequest,
    area: Rect,
) {
    let height = 9.min(area.height.saturating_sub(2));
    let width = area.width.saturating_sub(8).min(90);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("tier ", Style::default().fg(MUTED)),
            Span::styled(request.tier.clone(), Style::default().fg(WARN_COLOR)),
            Span::styled("  tool ", Style::default().fg(MUTED)),
            Span::styled(request.tool.clone(), Style::default().fg(ACCENT)),
        ]),
        Line::from(Span::styled(
            request.target.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "[a] allow once   [s] always allow   [d] / esc deny",
            Style::default().fg(MUTED),
        )),
    ];
    if app.busy {
        lines.push(Line::from(Span::styled(
            "the agent is waiting for this decision",
            Style::default().fg(MUTED),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(WARN_COLOR))
        .title(" approval required ");
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer_to_text(&buffer)
    }

    #[test]
    fn wraps_by_display_width() {
        // 每个汉字占 2 列，宽度 10 时一行放 5 个字。
        let wrapped = wrap_plain("一二三四五六七", 10);
        assert_eq!(wrapped, vec!["一二三四五".to_string(), "六七".to_string()]);
    }

    #[test]
    fn renders_empty_state_and_status() {
        let app = App::new("anthropic/claude", "s-1");
        let screen = render(&app, 80, 20);
        assert!(screen.contains("ask something"), "{screen}");
        assert!(screen.contains("input"), "{screen}");
        assert!(screen.contains("anthropic/claude"), "{screen}");
    }

    #[test]
    fn too_small_terminal_shows_hint() {
        let app = App::new("m", "s");
        let screen = render(&app, 30, 8);
        assert!(screen.contains("terminal too small"), "{screen}");
    }

    #[test]
    fn transcript_shows_tool_and_assistant_rows() {
        let mut app = App::new("m", "s");
        app.push_user("read a.ts");
        app.on_event(&crate::event_helper::text_delta("done."));
        app.on_event(&crate::event_helper::tool_start("read", "a.ts"));
        app.on_event(&crate::event_helper::tool_end(true));
        let screen = render(&app, 80, 24);
        assert!(screen.contains("read a.ts"), "{screen}");
        assert!(screen.contains("done."), "{screen}");
        assert!(screen.contains("✔ read a.ts"), "{screen}");
    }

    #[test]
    fn dialog_is_drawn_above_transcript() {
        let mut app = App::new("m", "s");
        app.push_user("please edit");
        let (tx, _rx) = std::sync::mpsc::channel();
        app.dialog = Some(super::super::app::Dialog {
            request: crate::approval::ApprovalRequest::new("write", "edit", "/repo/src/a.ts"),
            reply: tx,
        });
        let screen = render(&app, 90, 24);
        assert!(screen.contains("approval required"), "{screen}");
        assert!(screen.contains("always allow"), "{screen}");
        assert!(screen.contains("/repo/src/a.ts"), "{screen}");
    }
}
