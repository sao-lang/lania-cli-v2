//! `lan env ui` 的 TUI 实现（yazi 风格）。
//!
//! 模块划分：
//! - `theme` 配色与图标集
//! - `app`   应用状态与状态机（与渲染解耦）
//! - `ui`    渲染
//! - 本文件  终端生命周期与事件循环

pub mod app;
pub mod theme;
pub mod ui;

#[cfg(test)]
mod tests;

use std::io;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, InputKind, Mode};
use ui::Renderer;

/// 启动 TUI，阻塞直到用户退出。
pub fn run(cwd: PathBuf, locale: String) -> Result<()> {
    let renderer = Renderer::new(&locale);
    let mut app = App::new(cwd, locale);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, &mut app, &renderer);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// 后台扫描状态。
enum ScanState {
    Idle,
    Running(std::sync::mpsc::Receiver<app::ScanResult>),
    /// 安装状态已就绪，正在后台补齐各工具的版本号。
    Filling(std::sync::mpsc::Receiver<app::VersionBatch>),
    Done,
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    renderer: &Renderer,
) -> Result<()> {
    let mut scan = ScanState::Idle;

    loop {
        // 首帧立刻画出来，扫描放到后台线程，避免打开界面就卡住几秒。
        if matches!(scan, ScanState::Idle) {
            app.status = app.tr("Scanning toolchains…", "扫描环境中…").into();
            scan = ScanState::Running(app.spawn_scan());
        }

        terminal.draw(|frame| renderer.draw(frame, app))?;

        // 非阻塞地接收后台扫描结果。
        match &scan {
            ScanState::Running(rx) => match rx.try_recv() {
                Ok(result) => {
                    app.apply_scan(result);
                    // 安装状态就绪后，接着在后台补版本号。
                    app.status = app.tr("Reading versions…", "读取版本号…").into();
                    scan = ScanState::Filling(app.spawn_version_fill());
                    continue; // 立即重绘，让结果尽快上屏
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    app.status = app.tr("Reading versions…", "读取版本号…").into();
                    scan = ScanState::Filling(app.spawn_version_fill());
                }
            },
            ScanState::Filling(rx) => match rx.try_recv() {
                Ok(batch) => {
                    app.apply_versions(batch);
                    continue; // 每来一批就重绘，版本逐步出现
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // 通道还开着但暂时没数据；等下一轮。
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    app.status = format!("{} {}", app.tools.len(), app.tr("tools", "个工具"));
                    scan = ScanState::Done;
                }
            },
            _ => {}
        }

        // 100ms 轮询：既有按键手感，又能及时接住后台结果。
        if !event::poll(std::time::Duration::from_millis(100))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        // Windows 上会同时收到 Press/Release，只处理 Press。
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // 首轮扫描尚未完成时只允许退出，避免在没有数据时误触发操作；
        // 版本号补齐阶段允许正常操作（数据已经可用了）。
        if matches!(scan, ScanState::Running(_)) {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                app.should_quit = true;
            }
        } else {
            handle_key(app, key);
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

/// 非交互渲染：把当前画面画到一个离屏 buffer 并转换成纯文本。
///
/// 用于 `--snapshot`（CI / 调试），保证布局逻辑在没有 TTY 时也能被验证。
/// 可以先模拟按 `down_steps` 次方向键，用于验证滚动。
///
/// 如果 `wait_versions` 为 true，会像真实 TUI 那样等待后台补齐版本号，
/// 这样快照内容与用户实际看到的界面一致（只是阻塞到补齐完成）。
pub fn snapshot_with(
    cwd: PathBuf,
    cols: u16,
    rows: u16,
    down_steps: usize,
    wait_versions: bool,
    locale: &str,
) -> Result<String> {
    use ratatui::{backend::TestBackend, Terminal};

    let renderer = Renderer::new(locale);
    let mut app = App::new(cwd, locale);
    if wait_versions {
        // 复用交互路径的两段式流程：先拿安装状态，再补版本号。
        app.status = app.tr("Scanning toolchains…", "扫描环境中…").into();
        let scan = app.spawn_scan();
        if let Ok(result) = scan.recv() {
            app.apply_scan(result);
        }
        let fill = app.spawn_version_fill();
        for batch in fill.iter() {
            app.apply_versions(batch);
        }
        app.loading = false;
        app.status = format!("{} {}", app.tools.len(), app.tr("tools", "个工具"));
        app.prime_versions();
    } else {
        // 只验证布局：连版本号都不读，保证快速返回。
        app.load_versions = false;
        app.refresh();
    }

    let backend = TestBackend::new(cols, rows);
    let mut terminal = Terminal::new(backend)?;

    // 先渲染一次，让 App 拿到真实的 list_height（滚动窗口计算依赖它），
    // 再模拟按键，否则 `ensure_cursor_visible` 会用旧的默认高度。
    terminal.draw(|frame| renderer.draw(frame, &mut app))?;
    for _ in 0..down_steps {
        app.move_cursor(1);
    }
    terminal.draw(|frame| renderer.draw(frame, &mut app))?;

    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..rows {
        let mut line = String::new();
        let mut x = 0u16;
        while x < cols {
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            line.push_str(symbol);
            // 宽字符（CJK / emoji）占 2 个单元格，第 2 格是空占位，需要跳过，
            // 否则文本化输出会在中文字符之间多出空格。
            let width = unicode_width(symbol);
            x += width.max(1);
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    Ok(out)
}

/// 粗略估算字符显示宽度（CJK 与全角符号按 2 计）。
fn unicode_width(symbol: &str) -> u16 {
    symbol
        .chars()
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

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    match app.mode {
        Mode::Browse => handle_browse(app, key),
        Mode::Input(_) => handle_input(app, key),
        Mode::Confirm => handle_confirm(app, key),
        Mode::Result => handle_result(app, key),
    }
}

fn handle_browse(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => app.move_cursor(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_cursor(1),
        KeyCode::PageUp => app.move_cursor(-10),
        KeyCode::PageDown => app.move_cursor(10),
        KeyCode::Home => app.move_cursor(-(app.rows.len() as isize)),
        KeyCode::End => app.move_cursor(app.rows.len() as isize),
        // 右侧详情滚动
        KeyCode::Char('J') => app.scroll_detail(1),
        KeyCode::Char('K') => app.scroll_detail(-1),
        KeyCode::Char('r') => {
            app.scroll = 0;
            app.refresh();
        }
        KeyCode::Char('v') => app.reload_versions(),
        KeyCode::Char('D') => app.doctor(),
        KeyCode::Char('m') => app.cycle_manager(),
        KeyCode::Char('p') => {
            app.project_pin = !app.project_pin;
            app.status = if app.project_pin {
                app.tr("Project pin: on", "项目锁定：开").into()
            } else {
                app.tr("Project pin: off", "项目锁定：关").into()
            };
        }
        KeyCode::Char('i') => app.begin_input(InputKind::Install),
        KeyCode::Char('u') => app.begin_input(InputKind::Use),
        KeyCode::Char('d') => app.begin_input(InputKind::Remove),
        _ => {}
    }
}

fn handle_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Browse;
            app.input.clear();
        }
        KeyCode::Enter => {
            app.submit_input();
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(c) => {
            // 版本号只接受白名单字符，与 CLI 的校验保持一致。
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '@' | '/') {
                app.input.push(c);
            }
        }
        _ => {}
    }
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.execute_pending(),
        KeyCode::Esc => {
            app.pending_plan = None;
            app.mode = Mode::Browse;
            app.status = app.tr("Cancelled", "已取消").into();
        }
        _ => {}
    }
}

fn handle_result(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
            app.mode = Mode::Browse;
            app.log.clear();
            if app.pending_refresh {
                app.pending_refresh = false;
                app.refresh();
            }
        }
        _ => {}
    }
}
