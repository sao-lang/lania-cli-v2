//! TUI 渲染层：yazi 风格布局。
//!
//! 布局结构：
//! ```text
//! ╭─ 环境 ──────────────────────────────────────────────────────────╮
//! │ ╭─ 工具 ─────────────╮ ╭─ Node.js ──────────────────────────╮ │
//! │ │ ▾ node             │ │  当前版本    24.12.0               │ │
//! │ │ ❯ ● Node.js  24.12 │ │  当前来源    系统安装              │ │
//! │ │   ● npm       9.6  │ │  ...                               │ │
//! │ ╰────────────────────╯ ╰────────────────────────────────────╯ │
//! │  i 安装  u 切换  d 卸载  m 管理器  p 锁定  v 版本  D 体检      │
//! ╰─ NORMAL ─────────────────────────────── /path/to/project ──────╯
//! ```

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

use super::app::{App, InputKind, Mode, Row};
use super::theme::{IconSet, Theme};

pub struct Renderer {
    pub theme: Theme,
    pub icons: IconSet,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            theme: Theme::yazi(),
            icons: IconSet::detect(),
        }
    }

    pub fn draw(&self, frame: &mut Frame<'_>, app: &mut App) {
        let area = frame.area();
        if area.width < 60 || area.height < 14 {
            self.draw_too_small(frame, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // 标题栏
                Constraint::Min(3),    // 主体
                Constraint::Length(1), // 快捷键栏
                Constraint::Length(1), // 状态栏
            ])
            .split(area);

        self.draw_header(frame, chunks[0]);
        self.draw_body(frame, chunks[1], app);
        self.draw_keys(frame, chunks[2]);
        self.draw_status(frame, chunks[3], app);

        match app.mode {
            Mode::Input(kind) => self.draw_input(frame, area, app, kind),
            Mode::Confirm => self.draw_confirm(frame, area, app),
            Mode::Result => self.draw_result(frame, area, app),
            Mode::Browse => {}
        }
    }

    fn draw_too_small(&self, frame: &mut Frame<'_>, area: Rect) {
        let text = vec![
            Line::from(""),
            Line::from(Span::styled(
                "终端太小",
                Style::default()
                    .fg(self.theme.warn)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("请放大到至少 60x14（当前 {}x{}）", area.width, area.height),
                Style::default().fg(self.theme.muted),
            )),
        ];
        frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), area);
    }

    // ---------------- 标题栏 ----------------

    fn draw_header(&self, frame: &mut Frame<'_>, area: Rect) {
        let title = Span::styled(
            " lan env ",
            Style::default()
                .fg(self.theme.title)
                .add_modifier(Modifier::BOLD),
        );
        let dot = Span::styled("·", Style::default().fg(self.theme.muted));
        let name = Span::styled("toolchains", Style::default().fg(self.theme.text));
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                title,
                Span::raw(" "),
                dot,
                Span::raw(" "),
                name,
            ])),
            area,
        );
    }

    // ---------------- 主体双栏 ----------------

    fn draw_body(&self, frame: &mut Frame<'_>, area: Rect, app: &mut App) {
        // 左侧固定宽度更稳定：百分比在窄终端会让标题（含 CJK）与边框挤在一起。
        let left_width = ((area.width as u32 * 34 / 100) as u16)
            .clamp(28, 42)
            .min(area.width.saturating_sub(20));
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(left_width),
                Constraint::Length(1), // 面板间距
                Constraint::Fill(1),
            ])
            .split(area);

        // 记录可视高度并立即校正滚动窗口：
        // 终端尺寸变化后 `list_height` 会变，必须让光标重新落在可视区内。
        let height = columns[0].height.saturating_sub(2) as usize;
        if app.list_height != height {
            app.list_height = height;
            app.ensure_cursor_visible();
        }

        self.draw_list(frame, columns[0], app);
        self.draw_detail(frame, columns[2], app);
    }

    fn panel_block(&self, title: &str, focused: bool) -> Block<'_> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(self.theme.border_style(focused))
            .padding(ratatui::widgets::Padding::horizontal(1))
            .title(Span::styled(
                format!(" {title} "),
                self.theme.title_style(focused),
            ))
    }

    fn draw_list(&self, frame: &mut Frame<'_>, area: Rect, app: &App) {
        let block = self.panel_block("工具", true);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible = inner.height as usize;

        // 把「可显示的行」展开成扁平列表：分组标题行与其后的语言工具行合并为一行。
        let mut display_rows: Vec<Line<'_>> = Vec::new();
        let mut index = 0usize;
        while index < app.rows.len() {
            match &app.rows[index] {
                Row::Group(_) => {
                    if let Some(Row::Tool(tool_index)) = app.rows.get(index + 1) {
                        if let Some(tool) = app.tools.get(*tool_index) {
                            let selected = index + 1 == app.cursor;
                            display_rows.push(self.tool_line(app, tool, selected));
                            index += 2;
                            continue;
                        }
                    }
                    index += 1;
                }
                Row::Tool(tool_index) => {
                    if let Some(tool) = app.tools.get(*tool_index) {
                        let selected = index == app.cursor;
                        display_rows.push(self.tool_line(app, tool, selected));
                    }
                    index += 1;
                }
            }
        }

        let total = display_rows.len();
        let max_offset = total.saturating_sub(visible);
        let offset = app.list_offset.min(max_offset);
        let end = (offset + visible).min(total);
        let window: Vec<Line<'_>> = display_rows.drain(offset..end).collect();

        // 溢出时为滚动条预留最右 1 列，避免它盖住版本号文字。
        let overflow = total > visible;
        let body_area = if overflow {
            Rect {
                width: inner.width.saturating_sub(1),
                ..inner
            }
        } else {
            inner
        };

        frame.render_widget(Paragraph::new(window), body_area);

        if overflow {
            let mut state = ScrollbarState::new(max_offset).position(offset);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .style(Style::default().fg(self.theme.border)),
                Rect {
                    x: inner.x + inner.width.saturating_sub(1),
                    y: inner.y,
                    width: 1,
                    height: inner.height,
                },
                &mut state,
            );
        }
    }

    fn tool_line<'a>(&self, app: &App, tool: &lania_env::ToolStatus, selected: bool) -> Line<'a> {
        let is_language = tool.id == tool.language;
        // 统一的行格式：`<指针> <图标> <名称(定宽)> <版本>`。
        // 语言与附属工具只在图标/颜色上区分，缩进保持一致，
        // 避免此前 `·   npm` 那种多空格造成的参差感。
        let pointer = if selected {
            Span::styled(
                format!("{} ", self.icons.pointer()),
                Style::default()
                    .fg(self.theme.border_focus)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("  ")
        };

        let icon = if is_language {
            Span::styled(
                format!("{} ", self.icons.language(&tool.language)),
                Style::default().fg(self.theme.accent),
            )
        } else {
            Span::styled("· ", Style::default().fg(self.theme.muted))
        };

        let label = if is_language {
            language_label(&tool.language).to_string()
        } else {
            tool.id.clone()
        };
        let name_width = 10usize;
        let pad = name_width.saturating_sub(display_width(&label));
        let name = Span::styled(
            format!("{label}{}", " ".repeat(pad)),
            if is_language {
                Style::default().fg(self.theme.text)
            } else {
                Style::default().fg(self.theme.muted)
            },
        );

        let version = match (&tool.version, tool.installed) {
            (Some(version), _) => {
                Span::styled(format!(" {version}"), Style::default().fg(self.theme.ok))
            }
            // 懒加载：已安装但还没跑 `--version`，移动光标到该项时会补上。
            (None, true) => Span::styled(" …", Style::default().fg(self.theme.muted)),
            (None, false) => Span::styled(" 未安装", Style::default().fg(self.theme.err)),
        };

        let line = Line::from(vec![pointer, icon, name, version]);
        if selected {
            line.style(self.theme.selected_style())
        } else {
            line
        }
    }

    fn draw_detail<'a>(&self, frame: &mut Frame<'_>, area: Rect, app: &App) {
        let Some(tool) = app.current() else {
            let block = self.panel_block("详情", false);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "  没有可用工具",
                    Style::default().fg(self.theme.muted),
                )),
                inner,
            );
            return;
        };

        let title = language_label(&tool.language);
        let block = self.panel_block(title, false);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let label_width = 20usize;
        let mut lines: Vec<Line<'_>> = Vec::new();

        // 版本大标题
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                tool.version.clone().unwrap_or_else(|| "未安装".into()),
                Style::default()
                    .fg(if tool.installed {
                        self.theme.ok
                    } else {
                        self.theme.err
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", tool.name),
                Style::default().fg(self.theme.muted),
            ),
        ]));
        lines.push(Line::from(""));

        let mut kv = |lines: &mut Vec<Line<'a>>, key: &str, value: Span<'a>| {
            // 中文字符占 2 个单元格，必须按显示宽度补齐才能对齐。
            let key_width = display_width(key) + 2;
            let padding = label_width.saturating_sub(key_width);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {key}{}", " ".repeat(padding)),
                    Style::default().fg(self.theme.muted),
                ),
                value,
            ]));
        };

        kv(
            &mut lines,
            "可执行文件",
            Span::styled(
                tool.binary_path.clone().unwrap_or_else(|| "—".into()),
                Style::default().fg(self.theme.text),
            ),
        );
        kv(
            &mut lines,
            "当前来源",
            match app.effective_manager() {
                Some(manager) => {
                    Span::styled(manager.id().to_string(), Style::default().fg(self.theme.ok))
                }
                None if tool.installed => {
                    Span::styled("系统安装", Style::default().fg(self.theme.warn))
                }
                None => Span::styled("—", Style::default().fg(self.theme.muted)),
            },
        );
        kv(
            &mut lines,
            "安装将使用",
            match app.target_manager() {
                Some(manager) => Span::styled(
                    manager.id().to_string(),
                    Style::default().fg(self.theme.accent),
                ),
                None => Span::styled("无可用管理器", Style::default().fg(self.theme.err)),
            },
        );
        kv(
            &mut lines,
            "安装根目录",
            Span::styled(
                tool.install_root.clone().unwrap_or_else(|| "—".into()),
                Style::default().fg(self.theme.text),
            ),
        );
        kv(
            &mut lines,
            "项目版本约束",
            match &tool.project_version {
                Some(pin) => Span::styled(
                    format!("{}  ({})", pin.version, pin.file),
                    Style::default().fg(self.theme.accent),
                ),
                None => Span::styled("无", Style::default().fg(self.theme.muted)),
            },
        );

        // 已安装版本（`prime_versions` 在切换工具时已写入缓存，渲染时不再扫盘）
        let versions: &[(String, String)] = app.cached_versions();

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "  已安装版本",
                Style::default()
                    .fg(self.theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({})", versions.len()),
                Style::default().fg(self.theme.muted),
            ),
        ]));
        if versions.is_empty() {
            lines.push(Line::from(Span::styled(
                if tool.installed {
                    "    当前版本不受任何版本管理器管理"
                } else {
                    "    尚未安装"
                },
                Style::default().fg(self.theme.muted),
            )));
        } else {
            let indent = 4usize;
            let usable = (inner.width as usize).saturating_sub(indent);
            let gap = 1usize;
            let mut spans: Vec<Span<'_>> = Vec::new();
            let mut used = 0usize;
            for (manager, version) in versions {
                let is_current = tool.version.as_deref() == Some(version.as_str());
                let (fg, bg) = if is_current {
                    (self.theme.bg_chip(), self.theme.ok)
                } else {
                    (self.theme.text, self.theme.selection_bg)
                };
                // 宽度按「版本 chip + 管理器名 + 间距」估算，超出就换行。
                let width = version.len() + manager.len() + 3 + gap;
                if used > 0 && used + width > usable {
                    lines.push(Line::from(spans));
                    spans = Vec::new();
                    used = 0;
                }
                if spans.is_empty() {
                    spans.push(Span::raw(" ".repeat(indent)));
                    used = indent;
                }
                spans.push(Span::styled(
                    format!(" {version} "),
                    Style::default().fg(fg).bg(bg),
                ));
                spans.push(Span::styled(
                    format!("{manager} "),
                    Style::default().fg(self.theme.muted),
                ));
                used += width;
            }
            if !spans.is_empty() {
                lines.push(Line::from(spans));
            }
        }

        // 版本管理器可用性：按面板实际宽度换行，避免 `npm` 被截成 `n`。
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  版本管理器",
            Style::default()
                .fg(self.theme.muted)
                .add_modifier(Modifier::BOLD),
        )));

        let indent = 4usize;
        let usable = (inner.width as usize).saturating_sub(indent);
        let gap = 3usize;
        let mut spans: Vec<Span<'_>> = Vec::new();
        let mut used = 0usize;
        for probe in &tool.candidates {
            let (mark, color) = if probe.available {
                (self.icons.installed_mark(true), self.theme.ok)
            } else {
                (self.icons.installed_mark(false), self.theme.muted)
            };
            let text = format!("{mark} {}", probe.manager.id());
            let width = display_width(&text);
            if used > 0 && used + gap + width > usable {
                lines.push(Line::from(spans));
                spans = Vec::new();
                used = 0;
            }
            if spans.is_empty() {
                spans.push(Span::raw(" ".repeat(indent)));
                used = indent;
            } else {
                spans.push(Span::raw(" ".repeat(gap)));
                used += gap;
            }
            spans.push(Span::styled(text, Style::default().fg(color)));
            used += width;
        }
        if !spans.is_empty() {
            lines.push(Line::from(spans));
        }

        let content_height = lines.len();
        let overflow = content_height > inner.height as usize;

        // 内容溢出时给正文右侧留 1 列给滚动条，避免覆盖边框或文字。
        let body_area = if overflow {
            Rect {
                width: inner.width.saturating_sub(1),
                ..inner
            }
        } else {
            inner
        };

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0));
        frame.render_widget(paragraph, body_area);

        if overflow {
            let mut state =
                ScrollbarState::new(content_height.saturating_sub(inner.height as usize))
                    .position(app.scroll as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .style(Style::default().fg(self.theme.border)),
                Rect {
                    x: inner.x + inner.width.saturating_sub(1),
                    y: inner.y,
                    width: 1,
                    height: inner.height,
                },
                &mut state,
            );
        }
    }

    // ---------------- 底部栏 ----------------

    fn draw_keys(&self, frame: &mut Frame<'_>, area: Rect) {
        let key = |name: &'static str| {
            Span::styled(
                format!(" {name} "),
                Style::default()
                    .fg(chip_fg())
                    .bg(self.theme.muted)
                    .add_modifier(Modifier::BOLD),
            )
        };
        let label = |text: &'static str| Span::styled(text, Style::default().fg(self.theme.muted));

        let spans = vec![
            Span::raw(" "),
            key("i"),
            label(" install "),
            key("u"),
            label(" use "),
            key("d"),
            label(" remove "),
            key("m"),
            label(" manager "),
            key("p"),
            label(" pin "),
            key("v"),
            label(" versions "),
            key("D"),
            label(" doctor "),
            key("r"),
            label(" reload "),
            key("q"),
            label(" quit"),
        ];
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_status(&self, frame: &mut Frame<'_>, area: Rect, app: &App) {
        let mode_label = match app.mode {
            Mode::Browse => "NORMAL",
            Mode::Input(_) => "INPUT",
            Mode::Confirm => "CONFIRM",
            Mode::Result => "RESULT",
        };
        let mode_color = match app.mode {
            Mode::Browse => self.theme.border_focus,
            Mode::Input(_) => self.theme.accent,
            Mode::Confirm => self.theme.warn,
            Mode::Result => self.theme.ok,
        };

        let left = Span::styled(
            format!(" {mode_label} "),
            Style::default()
                .fg(self.theme.bg_chip())
                .bg(mode_color)
                .add_modifier(Modifier::BOLD),
        );
        let middle = Span::styled(
            format!(" {} ", app.status),
            Style::default().fg(self.theme.text),
        );
        let path = app.cwd.display().to_string();
        let pin = if app.project_pin {
            " [项目锁定]"
        } else {
            ""
        };
        let right_text = format!(" {path}{pin} ");
        let right = Span::styled(right_text, Style::default().fg(self.theme.muted));

        frame.render_widget(
            Paragraph::new(Line::from(vec![left, middle, right]))
                .style(Style::default().bg(self.theme.status_bg)),
            area,
        );
    }

    // ---------------- 弹窗 ----------------

    fn popup_area(&self, area: Rect, width: u16, height: u16) -> Rect {
        let width = width.min(area.width.saturating_sub(4));
        let height = height.min(area.height.saturating_sub(4));
        Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        }
    }

    fn draw_input(&self, frame: &mut Frame<'_>, area: Rect, app: &App, kind: InputKind) {
        let popup = self.popup_area(area, 60, 7);
        frame.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.accent))
            .title(Span::styled(
                format!(" {} 版本 ", kind.label()),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let tool_name = app
            .current()
            .map(|tool| tool.language.clone())
            .unwrap_or_default();
        let manager = app
            .target_manager()
            .map(|manager| manager.id().to_string())
            .unwrap_or_else(|| "无".into());
        let pin = if app.project_pin { "开" } else { "关" };

        let lines = vec![
            Line::from(Span::styled(
                format!("  {tool_name} · 管理器 {manager} · 项目锁定 {pin}"),
                Style::default().fg(self.theme.muted),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ❯ ", Style::default().fg(self.theme.border_focus)),
                Span::styled(app.input.clone(), Style::default().fg(self.theme.text)),
                Span::styled("█", Style::default().fg(self.theme.border_focus)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Enter 确认   Esc 取消",
                Style::default().fg(self.theme.muted),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_confirm(&self, frame: &mut Frame<'_>, area: Rect, app: &App) {
        let Some(plan) = app.pending_plan.as_ref() else {
            return;
        };
        let step_count = plan.steps.len() as u16;
        let height = 8 + step_count;
        let popup = self.popup_area(area, 76, height);
        frame.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.warn))
            .title(Span::styled(
                format!(" 确认{} {} ", action_label(plan.action), plan.tool),
                Style::default()
                    .fg(self.theme.warn)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let mut lines: Vec<Line<'_>> = Vec::new();
        lines.push(Line::from(""));

        if let Some(error) = plan.error.as_deref() {
            lines.push(Line::from(Span::styled(
                format!("  ✗ {error}"),
                Style::default().fg(self.theme.err),
            )));
        } else {
            for step in &plan.steps {
                lines.push(Line::from(vec![
                    Span::styled("  $ ", Style::default().fg(self.theme.ok)),
                    Span::styled(step.command.clone(), Style::default().fg(self.theme.text)),
                ]));
                lines.push(Line::from(Span::styled(
                    format!("     {}", step.label),
                    Style::default().fg(self.theme.muted),
                )));
            }
        }

        for note in &plan.notes {
            lines.push(Line::from(Span::styled(
                format!("  · {note}"),
                Style::default().fg(self.theme.warn),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "  Enter ",
                Style::default().fg(self.theme.bg_chip()).bg(self.theme.ok),
            ),
            Span::styled(" 执行    ", Style::default().fg(self.theme.muted)),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(self.theme.bg_chip())
                    .bg(self.theme.muted),
            ),
            Span::styled(" 取消", Style::default().fg(self.theme.muted)),
        ]));

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    fn draw_result(&self, frame: &mut Frame<'_>, area: Rect, app: &App) {
        let popup = self.popup_area(
            area,
            84,
            (app.log.len() as u16 + 6).min(area.height.saturating_sub(4)),
        );
        frame.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.ok))
            .title(Span::styled(
                " 输出 ",
                Style::default()
                    .fg(self.theme.ok)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let mut lines: Vec<Line<'_>> = app
            .log
            .iter()
            .map(|line| {
                let style = if line.trim_start().starts_with('✓') {
                    Style::default().fg(self.theme.ok)
                } else if line.trim_start().starts_with('✗') {
                    Style::default().fg(self.theme.err)
                } else if line.starts_with('›') || line.starts_with('$') {
                    Style::default().fg(self.theme.accent)
                } else {
                    Style::default().fg(self.theme.text)
                };
                Line::from(Span::styled(format!("  {line}"), style))
            })
            .collect();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Esc / Enter 关闭",
            Style::default().fg(self.theme.muted),
        )));

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

/// 语言 id → 展示名。
///
/// 直接委托给 `lania_env::Language`，避免这里再维护一份会与实际支持列表脱节的映射。
pub fn language_label(id: &str) -> &'static str {
    lania_env::Language::parse_id(id)
        .map(|language| language.display_name())
        .unwrap_or("Other")
}

/// 动作 → 中文标签。
pub fn action_label(action: lania_env::EnvAction) -> &'static str {
    use lania_env::EnvAction;
    match action {
        EnvAction::Install => "安装",
        EnvAction::Use => "切换",
        EnvAction::Remove | EnvAction::UninstallManagerVersion => "卸载",
    }
}

impl Theme {
    /// 深色背景上用于强调块前景色。
    fn bg_chip(&self) -> ratatui::style::Color {
        chip_fg()
    }
}

/// 强调块（键名、chip）上的深色前景，保证与背景对比度。
fn chip_fg() -> ratatui::style::Color {
    ratatui::style::Color::Rgb(16, 18, 22)
}

/// 估算字符串的终端显示宽度（CJK / 全角按 2 计）。
pub fn display_width(text: &str) -> usize {
    text.chars()
        .map(|c| {
            let code = c as u32;
            let wide = matches!(code,
                0x1100..=0x115F | 0x2E80..=0xA4CF | 0xA960..=0xA97F
                | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE10..=0xFE19
                | 0xFE30..=0xFE6F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6
                | 0x1F300..=0x1F64F | 0x1F900..=0x1F9FF | 0x20000..=0x3FFFD
            );
            if wide {
                2
            } else {
                1
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_width_counts_cjk_as_two() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("当前版本"), 8);
        // 中文 2 个（4 列）+ 空格 1 + 2 个 ASCII 字母
        assert_eq!(display_width("版本 A1"), 7);
    }

    #[test]
    fn language_label_covers_all_supported() {
        // 直接遍历 `Language::all()`，新增语言时自动纳入覆盖。
        for language in lania_env::Language::all() {
            let label = language_label(language.id());
            assert_ne!(label, "Other", "{} missing label", language.id());
            assert!(!label.is_empty(), "{} has empty label", language.id());
        }
        assert!(lania_env::Language::all().len() >= 40, "expected 40+ languages");
    }
}
