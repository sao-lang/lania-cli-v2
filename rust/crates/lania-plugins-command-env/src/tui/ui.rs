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
    layout::{Alignment, Constraint, Direction, Layout, Rect},
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
    pub locale: String,
}

impl Renderer {
    pub fn new(locale: &str) -> Self {
        Self {
            theme: Theme::yazi(),
            icons: IconSet::detect(),
            locale: locale.to_string(),
        }
    }

    fn zh(&self) -> bool {
        self.locale.starts_with("zh")
    }

    fn tr(&self, en: &'static str, zh: &'static str) -> &'static str {
        if self.zh() {
            zh
        } else {
            en
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
                Constraint::Length(3), // 标题 + 工作目录 / git / 系统
                Constraint::Min(3),    // 主体
                Constraint::Length(1), // 快捷键栏
                Constraint::Length(1), // 状态栏
            ])
            .split(area);

        self.draw_header(frame, chunks[0], app);
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
                self.tr("Terminal too small", "终端太小"),
                Style::default()
                    .fg(self.theme.warn)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                if self.zh() {
                    format!("请放大到至少 60x14（当前 {}x{}）", area.width, area.height)
                } else {
                    format!(
                        "Resize to at least 60x14 (current {}x{})",
                        area.width, area.height
                    )
                },
                Style::default().fg(self.theme.muted),
            )),
        ];
        frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), area);
    }

    // ---------------- 标题栏 ----------------

    fn draw_header(&self, frame: &mut Frame<'_>, area: Rect, app: &App) {
        let bold = Modifier::BOLD;
        let title = Span::styled(
            " lan env ",
            Style::default().fg(self.theme.title).add_modifier(bold),
        );
        let dot = Span::styled("·", Style::default().fg(self.theme.border_focus));
        let name = Span::styled(
            "toolchains",
            Style::default().fg(self.theme.text).add_modifier(bold),
        );

        // 第一行：标题 + 设备系统信息（强调色，不使用灰色）。
        let os_label = Span::styled(
            "os ",
            Style::default().fg(self.theme.warn).add_modifier(bold),
        );
        let os_value = Span::styled(
            app.system.clone(),
            Style::default().fg(self.theme.text).add_modifier(bold),
        );

        // 第二行：当前工作目录。
        let cwd = fit_label(
            &app.cwd.display().to_string(),
            area.width.saturating_sub(7) as usize,
        );
        let cwd_line = Line::from(vec![
            Span::styled(
                " cwd ",
                Style::default().fg(self.theme.title).add_modifier(bold),
            ),
            Span::styled(cwd, Style::default().fg(self.theme.text).add_modifier(bold)),
        ]);

        // 第三行：git 仓库与分支。
        let git_line = match (&app.repo, &app.git_branch) {
            (Some(repo), Some(branch)) => Line::from(vec![
                Span::styled(
                    " git ",
                    Style::default().fg(self.theme.accent).add_modifier(bold),
                ),
                Span::styled(
                    repo.clone(),
                    Style::default().fg(self.theme.text).add_modifier(bold),
                ),
                Span::styled(
                    " @ ",
                    Style::default()
                        .fg(self.theme.border_focus)
                        .add_modifier(bold),
                ),
                Span::styled(
                    branch.clone(),
                    Style::default().fg(self.theme.ok).add_modifier(bold),
                ),
            ]),
            (Some(repo), None) => Line::from(vec![
                Span::styled(
                    " git ",
                    Style::default().fg(self.theme.accent).add_modifier(bold),
                ),
                Span::styled(
                    repo.clone(),
                    Style::default().fg(self.theme.text).add_modifier(bold),
                ),
            ]),
            _ => Line::from(vec![
                Span::styled(
                    " git ",
                    Style::default().fg(self.theme.accent).add_modifier(bold),
                ),
                Span::styled(
                    self.tr("no repository", "无仓库"),
                    Style::default().fg(self.theme.warn).add_modifier(bold),
                ),
            ]),
        };

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    title,
                    Span::raw(" "),
                    dot,
                    Span::raw(" "),
                    name,
                    Span::styled("  ·  ", Style::default().fg(self.theme.border_focus)),
                    os_label,
                    os_value,
                ]),
                cwd_line,
                git_line,
            ]),
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
        let block = self.panel_block(self.tr("Tools", "工具"), true);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible = inner.height as usize;
        let name_width = self.name_column_width(app);

        // 把「可显示的行」展开成扁平列表：分组标题行与其后的语言工具行合并为一行。
        let mut display_rows: Vec<Line<'_>> = Vec::new();
        let mut index = 0usize;
        while index < app.rows.len() {
            match &app.rows[index] {
                Row::Group(_) => {
                    if let Some(Row::Tool(tool_index)) = app.rows.get(index + 1) {
                        if let Some(tool) = app.tools.get(*tool_index) {
                            let selected = index + 1 == app.cursor;
                            display_rows.push(self.tool_line(tool, selected, name_width));
                            index += 2;
                            continue;
                        }
                    }
                    index += 1;
                }
                Row::Tool(tool_index) => {
                    if let Some(tool) = app.tools.get(*tool_index) {
                        let selected = index == app.cursor;
                        display_rows.push(self.tool_line(tool, selected, name_width));
                    }
                    index += 1;
                }
            }
        }

        if display_rows.is_empty() {
            // 首帧就画出骨架，避免边框出现后中间空白一段可感知的时间。
            let lines: Vec<Line<'_>> = (0..visible)
                .map(|row| {
                    let text = if app.loading {
                        if row % 2 == 0 {
                            "  ░░░░░░░░░░░░  ░░░░░"
                        } else {
                            "  ░░░░░░  ░░░░░░░"
                        }
                    } else if row == 0 {
                        self.tr("  No toolchains found", "  没有找到工具链")
                    } else {
                        ""
                    };
                    Line::from(Span::styled(text, Style::default().fg(self.theme.border)))
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), inner);
            return;
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

    /// 详情面板的一句话简介：语言用内置 summary，附属工具标注为所属语言的包管理器。
    fn tool_summary(&self, tool: &lania_runtime_manager::ToolStatus) -> String {
        if tool.id != tool.language {
            return format!(
                "{} {}",
                language_label(&tool.language),
                self.tr("package manager", "包管理器")
            );
        }
        lania_runtime_manager::Language::parse_id(&tool.language)
            .map(|language| language.summary(self.zh()).to_string())
            .unwrap_or_default()
    }

    /// 所有行共用的名称列宽：按最长标签计算并夹在 10..=14，保证版本列对齐。
    fn name_column_width(&self, app: &App) -> usize {
        app.tools
            .iter()
            .map(|tool| display_width(&tool_label(tool)))
            .max()
            .unwrap_or(10)
            .clamp(10, 14)
    }

    fn tool_line<'a>(
        &self,
        tool: &lania_runtime_manager::ToolStatus,
        selected: bool,
        name_width: usize,
    ) -> Line<'a> {
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

        let label = fit_label(&tool_label(tool), name_width);
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
            (None, false) => Span::styled(
                self.tr(" not installed", " 未安装"),
                Style::default().fg(self.theme.err),
            ),
        };

        let line = Line::from(vec![pointer, icon, name, version]);
        if selected {
            line.style(self.theme.selected_style())
        } else {
            line
        }
    }

    fn draw_detail(&self, frame: &mut Frame<'_>, area: Rect, app: &App) {
        let Some(tool) = app.current() else {
            let block = self.panel_block(self.tr("Details", "详情"), false);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let text = if app.loading {
                self.tr(
                    "  Scanning toolchains…\n\n  ░░░░░░░░░░░░░░░░\n  ░░░░░░░░░░\n  ░░░░░░░░░░░░░░",
                    "  正在扫描工具链…\n\n  ░░░░░░░░░░░░░░░░\n  ░░░░░░░░░░\n  ░░░░░░░░░░░░░░",
                )
            } else {
                self.tr("  No tools available", "  没有可用工具")
            };
            frame.render_widget(
                Paragraph::new(Span::styled(text, Style::default().fg(self.theme.muted))),
                inner,
            );
            return;
        };

        let title = language_label(&tool.language);
        let block = self.panel_block(title, false);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines: Vec<Line<'_>> = Vec::new();

        // 版本大标题 + 简介
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                tool.version
                    .clone()
                    .unwrap_or_else(|| self.tr("not installed", "未安装").into()),
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
        lines.push(Line::from(Span::styled(
            format!("  {}", self.tool_summary(tool)),
            Style::default().fg(self.theme.muted),
        )));
        lines.push(Line::from(""));

        fn kv<'a>(lines: &mut Vec<Line<'a>>, key: &str, value: Span<'a>, theme: &Theme) {
            // 中文字符占 2 个单元格，必须按显示宽度补齐才能对齐。
            let padding = label_padding(key, 22);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {key}{}", " ".repeat(padding)),
                    Style::default().fg(theme.muted),
                ),
                value,
            ]));
        }

        kv(
            &mut lines,
            self.tr("Executable", "可执行文件"),
            Span::styled(
                tool.binary_path.clone().unwrap_or_else(|| "—".into()),
                Style::default().fg(self.theme.text),
            ),
            &self.theme,
        );
        kv(
            &mut lines,
            self.tr("Current source", "当前来源"),
            match app.current_source() {
                Some("lania") => Span::styled(
                    self.tr("managed by lania", "lania 自管"),
                    Style::default().fg(self.theme.ok),
                ),
                Some(value) => {
                    Span::styled(value.to_string(), Style::default().fg(self.theme.warn))
                }
                None => Span::styled("—", Style::default().fg(self.theme.muted)),
            },
            &self.theme,
        );
        kv(
            &mut lines,
            self.tr("Self-managed", "自管状态"),
            if app.is_managed() {
                Span::styled(
                    tool.upstream.clone().unwrap_or_else(|| "✓".into()),
                    Style::default().fg(self.theme.accent),
                )
            } else {
                Span::styled(
                    self.tr("not yet supported", "暂不支持自管"),
                    Style::default().fg(self.theme.muted),
                )
            },
            &self.theme,
        );
        kv(
            &mut lines,
            self.tr("Managed versions", "自管版本数"),
            Span::styled(
                tool.installed_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "—".into()),
                Style::default().fg(self.theme.text),
            ),
            &self.theme,
        );
        kv(
            &mut lines,
            self.tr("Project constraint", "项目版本约束"),
            match &tool.project_version {
                Some(pin) => Span::styled(
                    format!("{}  ({})", pin.version, pin.file),
                    Style::default().fg(self.theme.accent),
                ),
                None => Span::styled(self.tr("none", "无"), Style::default().fg(self.theme.muted)),
            },
            &self.theme,
        );

        // 已安装版本（`prime_versions` 在切换工具时已写入缓存，渲染时不再扫盘）
        let versions: &[(String, String)] = app.cached_versions();

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                self.tr("  Installed versions", "  已安装版本"),
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
                    self.tr(
                        "    No versions installed through lania yet",
                        "    尚未通过 lania 安装任何版本",
                    )
                } else {
                    self.tr("    Not installed yet", "    尚未安装")
                },
                Style::default().fg(self.theme.muted),
            )));
        } else {
            let indent = 4usize;
            let usable = (inner.width as usize).saturating_sub(indent);
            let gap = 1usize;
            let mut spans: Vec<Span<'_>> = Vec::new();
            let mut used = 0usize;
            for (source, version) in versions {
                let is_current = tool.version.as_deref() == Some(version.as_str());
                let (fg, bg) = if is_current {
                    (self.theme.bg_chip(), self.theme.ok)
                } else {
                    (self.theme.text, self.theme.selection_bg)
                };
                // 宽度按「版本 chip + 来源名 + 间距」估算，超出就换行。
                let width = version.len() + source.len() + 3 + gap;
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
                    format!("{source} "),
                    Style::default().fg(self.theme.muted),
                ));
                used += width;
            }
            if !spans.is_empty() {
                lines.push(Line::from(spans));
            }
        }

        // 自管入口：`use` / `install` 之后 shim 会暴露这些命令。
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            self.tr("  Entrypoints", "  入口命令"),
            Style::default()
                .fg(self.theme.muted)
                .add_modifier(Modifier::BOLD),
        )));
        let entrypoints: Vec<&str> = lania_runtime_manager::Language::parse_id(&tool.language)
            .map(|language| language.probe_binaries())
            .unwrap_or_default();
        lines.push(Line::from(Span::styled(
            format!("    {}", entrypoints.join("  ")),
            Style::default().fg(self.theme.text),
        )));
        if !tool.managed {
            lines.push(Line::from(Span::styled(
                format!(
                    "    {}",
                    self.tr(
                        "self-managed install is not available for this language yet",
                        "该语言的自管安装尚未支持",
                    )
                ),
                Style::default().fg(self.theme.muted),
            )));
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
            label(self.tr(" install ", " 安装 ")),
            key("u"),
            label(self.tr(" use ", " 切换 ")),
            key("d"),
            label(self.tr(" remove ", " 卸载 ")),
            key("p"),
            label(self.tr(" pin ", " 锁定 ")),
            key("v"),
            label(self.tr(" versions ", " 版本 ")),
            key("D"),
            label(self.tr(" doctor ", " 体检 ")),
            key("r"),
            label(self.tr(" reload ", " 刷新 ")),
            key("q"),
            label(self.tr(" quit", " 退出")),
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
        let pin = if app.project_pin {
            self.tr(" [project pin] ", " [项目锁定] ")
        } else {
            ""
        };
        let right = Span::styled(pin, Style::default().fg(self.theme.muted));

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
                format!(" {} {} ", kind.label(self.zh()), self.tr("version", "版本")),
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
        let pin = if app.project_pin {
            self.tr("on", "开")
        } else {
            self.tr("off", "关")
        };

        let lines = vec![
            Line::from(Span::styled(
                format!(
                    "  {tool_name} · {} {pin}",
                    self.tr("project pin", "项目锁定")
                ),
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
                self.tr("  Enter confirm   Esc cancel", "  Enter 确认   Esc 取消"),
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
                format!(
                    " {} {} {} ",
                    self.tr("Confirm", "确认"),
                    action_label(plan.action, self.zh()),
                    plan.tool
                ),
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
            Span::styled(
                self.tr(" Execute    ", " 执行    "),
                Style::default().fg(self.theme.muted),
            ),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(self.theme.bg_chip())
                    .bg(self.theme.muted),
            ),
            Span::styled(
                self.tr(" Cancel", " 取消"),
                Style::default().fg(self.theme.muted),
            ),
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
                self.tr(" Output ", " 输出 "),
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
            self.tr("  Esc / Enter to close", "  Esc / Enter 关闭"),
            Style::default().fg(self.theme.muted),
        )));

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

/// 语言 id → 展示名。
///
/// 直接委托给 `lania_runtime_manager::Language`，避免这里再维护一份会与实际支持列表脱节的映射。
pub fn language_label(id: &str) -> &'static str {
    lania_runtime_manager::Language::parse_id(id)
        .map(|language| language.display_name())
        .unwrap_or("Other")
}

/// 左侧列表显示名：语言用展示名，附属工具用可执行文件名。
fn tool_label(tool: &lania_runtime_manager::ToolStatus) -> String {
    if tool.id == tool.language {
        language_label(&tool.language).to_string()
    } else {
        tool.id.clone()
    }
}

/// 截断到指定显示宽度，超出部分用 `…` 表示，保证版本列起点一致。
fn fit_label(label: &str, width: usize) -> String {
    if display_width(label) <= width {
        return label.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in label.chars() {
        let ch_width = display_width(&ch.to_string());
        if used + ch_width + 1 > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

/// 动作 → 本地化标签。
pub fn action_label(action: lania_runtime_manager::EnvAction, zh: bool) -> &'static str {
    use lania_runtime_manager::EnvAction;
    match (action, zh) {
        (EnvAction::Install, true) => "安装",
        (EnvAction::Use, true) => "切换",
        (EnvAction::Remove, true) => "卸载",
        (EnvAction::Install, false) => "install",
        (EnvAction::Use, false) => "use",
        (EnvAction::Remove, false) => "remove",
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

/// 详情面板 KV 的左侧留白：保证所有 value 从同一列开始。
fn label_padding(key: &str, label_width: usize) -> usize {
    label_width.saturating_sub(display_width(key) + 2)
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
        for language in lania_runtime_manager::Language::all() {
            let label = language_label(language.id());
            assert_ne!(label, "Other", "{} missing label", language.id());
            assert!(!label.is_empty(), "{} has empty label", language.id());
        }
        assert!(
            lania_runtime_manager::Language::all().len() >= 40,
            "expected 40+ languages"
        );
    }

    #[test]
    fn detail_values_share_one_column() {
        let label_width = 22;
        for key in [
            "Executable",
            "Current source",
            "Will install with",
            "Install root",
            "Project constraint",
            "可执行文件",
            "安装将使用",
        ] {
            // kv 前缀是 2 个空格 + key + padding，结果必须等于 label_width。
            assert_eq!(
                2 + display_width(key) + label_padding(key, label_width),
                label_width,
                "{key} is misaligned"
            );
        }
    }

    #[test]
    fn fit_label_truncates_to_column_width() {
        assert_eq!(fit_label("Node.js", 14), "Node.js");
        assert_eq!(fit_label("kotlinc-native", 14), "kotlinc-native");
        let fitted = fit_label("typescript-language-server", 14);
        assert!(display_width(&fitted) <= 14, "{fitted:?}");
        assert!(fitted.ends_with('…'), "{fitted:?}");
    }

    fn tool(id: &str, language: &str) -> lania_runtime_manager::ToolStatus {
        lania_runtime_manager::ToolStatus {
            id: id.into(),
            name: id.into(),
            language: language.into(),
            installed: true,
            version: None,
            binary_path: None,
            source: Some("system".into()),
            managed: false,
            upstream: None,
            installed_count: None,
            project_version: None,
        }
    }

    #[test]
    fn name_column_is_shared_and_bounded() {
        let renderer = Renderer::new("en");
        let mut app = App::new(std::path::PathBuf::from("."), "en");
        app.set_tools_for_test(vec![tool("node", "node")]);
        assert_eq!(renderer.name_column_width(&app), 10);

        app.set_tools_for_test(vec![tool("node", "node"), tool("kotlinc-native", "kotlin")]);
        assert_eq!(renderer.name_column_width(&app), 14);
    }

    #[test]
    fn detail_summaries_cover_languages_and_package_managers() {
        let renderer = Renderer::new("en");
        let language = tool("node", "node");
        assert!(renderer.tool_summary(&language).contains("JavaScript"));

        let companion = tool("npm", "node");
        assert!(renderer
            .tool_summary(&companion)
            .contains("package manager"));
    }

    #[test]
    fn loading_frame_renders_skeleton_instead_of_blank() {
        use ratatui::{backend::TestBackend, Terminal};

        let renderer = Renderer::new("en");
        let mut app = App::new(std::path::PathBuf::from("."), "en");
        app.loading = true;
        app.tools.clear();
        app.rows.clear();

        let mut terminal = Terminal::new(TestBackend::new(80, 16)).expect("terminal");
        terminal
            .draw(|frame| renderer.draw(frame, &mut app))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let text: String = (0..16)
            .map(|y| {
                (0..80)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Scanning toolchains"), "{text}");
        assert!(text.contains('░'), "{text}");
    }
}
