//! 全屏 TUI 入口：终端生命周期 + 输入线程 + 事件合流。
//!
//! 事件来源有三路，用 `tokio::select!` 合流：
//! 1. 键盘：独立线程里 `crossterm::event::poll` + `read`，通过 mpsc 送入；
//!    （不在主循环里阻塞 `event::read()`，否则定时 tick 和 agent 事件都会被卡住）
//! 2. 运行时：actor 的 `UiUpdate`；
//! 3. 审批：宿主 broker 转来的弹窗请求。
//!
//! 退出清理用 [`TerminalGuard`] 保证——即使中途 `?` 提前返回，也一定会
//! 关掉 raw mode 与备用屏，不会把用户终端留在坏状态。

pub mod actor;
pub mod app;
pub mod view;

use std::{
    io::{stdout, Stdout, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lania_agent_core::{AgentSessionClient, ApprovalMode, ThinkingLevel};

use crate::approval::{ApprovalDecision, ApprovalPrompt, ApprovalRequest};
use lania_agent_core::SessionInterrupt;
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::runtime::PiRuntime;

use self::{
    actor::{Actor, UiCommand, UiUpdate},
    app::{App, Dialog},
};

/// TUI 的运行参数。
pub struct TuiConfig<'a> {
    /// 当前会话 id 的共享槽位（中断与状态栏都读它）。
    pub session_cell: Arc<Mutex<String>>,
    /// 中断句柄（由传输层提供；`None` 表示不支持中断）。
    pub interruptor: Option<Arc<dyn SessionInterrupt>>,
    /// 审批请求来源（由调用方通过 [`crate::ApprovalSetup`] 创建，宿主侧把 broker 接到同一个 bus 上）。
    pub approvals: Option<&'a mut tokio::sync::mpsc::UnboundedReceiver<ApprovalPrompt>>,
}

/// 跑起来直到用户退出。
pub async fn run<C>(runtime: PiRuntime<C>, config: TuiConfig<'_>) -> Result<()>
where
    C: AgentSessionClient + Send + 'static,
{
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel::<UiCommand>();
    let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel::<UiUpdate>();

    let mut app = App::new(
        runtime.info().model.clone(),
        runtime.session_id().to_string(),
    );
    app.thinking = runtime.spec().thinking.to_string();
    app.approval = runtime.spec().approval_mode.to_string();
    app.context_window = runtime.info().context_window;

    tokio::spawn(Actor::new(runtime, command_rx, update_tx.clone()).run());

    let mut approvals = config.approvals;
    let mut guard = TerminalGuard::new()?;
    let mut key_rx = spawn_input_thread();

    loop {
        guard.terminal.draw(|frame| view::draw(frame, &app))?;

        tokio::select! {
            maybe_key = key_rx.recv() => {
                let Some(key) = maybe_key else { break };
                match handle_key(key, &mut app, &command_tx) {
                    Some(Action::Command(command)) => {
                        let _ = command_tx.send(command);
                    }
                    Some(Action::Quit) => {
                        let _ = command_tx.send(UiCommand::Quit);
                    }
                    Some(Action::Interrupt) => {
                        let session_id = config
                            .session_cell
                            .lock()
                            .map(|guard| guard.clone())
                            .unwrap_or_default();
                        request_interrupt(config.interruptor.clone(), session_id);
                        app.push_notice(
                            crate::runtime::NoticeLevel::Warn,
                            "interrupt requested",
                        );
                    }
                    None => {}
                }
            }
            maybe_update = update_rx.recv() => {
                match maybe_update {
                    Some(update) => apply_update(&mut app, update),
                    None => break,
                }
            }
            maybe_prompt = next_prompt(&mut approvals) => {
                match maybe_prompt {
                    Some(prompt) => {
                        app.hint = None;
                        app.dialog = Some(Dialog {
                            request: prompt.request,
                            reply: prompt.reply,
                        });
                    }
                    // 审批通道被关掉：不再等待，继续跑（没有弹窗的 TUI 没有意义）。
                    None => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(120)) => {
                if app.busy {
                    app.spinner = app.spinner.wrapping_add(1);
                }
            }
        }

        if app.quit {
            break;
        }
    }

    let _ = command_tx.send(UiCommand::Quit);
    // 给 actor 一点时间关掉 Node 会话；超时就算了，进程马上退出。
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        while update_rx.recv().await.is_some() {}
    })
    .await;

    drop(guard);
    Ok(())
}

enum Action {
    Command(UiCommand),
    Quit,
    Interrupt,
}

fn resolve_slash(input: &str) -> Option<Action> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed[1..].splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default().to_ascii_lowercase();
    let argument = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    Some(match (name.as_str(), argument) {
        ("quit" | "exit" | "q", _) => Action::Quit,
        ("help" | "h" | "?", _) => Action::Command(UiCommand::Help),
        ("status" | "st", _) => Action::Command(UiCommand::Status),
        ("sessions", _) => Action::Command(UiCommand::Sessions),
        ("compact", _) => Action::Command(UiCommand::Compact),
        ("rewind" | "undo", Some(raw)) => {
            Action::Command(UiCommand::Rewind(raw.trim().parse::<usize>().unwrap_or(1)))
        }
        ("rewind" | "undo", None) => Action::Command(UiCommand::Rewind(1)),
        ("new", _) => Action::Command(UiCommand::New),
        ("fork", _) => Action::Command(UiCommand::Fork),
        ("model", Some(model)) => Action::Command(UiCommand::SetModel(model.to_string())),
        ("model", None) => Action::Command(UiCommand::Status),
        ("thinking" | "think", Some(level)) => ThinkingLevel::parse(level)
            .map(|level| Action::Command(UiCommand::SetThinking(level)))
            .unwrap_or(Action::Command(UiCommand::Help)),
        ("thinking" | "think", None) => Action::Command(UiCommand::CycleThinking),
        ("approve" | "approval", Some(mode)) => ApprovalMode::parse(mode)
            .map(|mode| Action::Command(UiCommand::SetApproval(mode)))
            .unwrap_or(Action::Command(UiCommand::Help)),
        ("approve" | "approval", None) => Action::Command(UiCommand::Status),
        _ => Action::Command(UiCommand::Help),
    })
}

fn handle_key(
    key: KeyEvent,
    app: &mut App,
    commands: &tokio::sync::mpsc::UnboundedSender<UiCommand>,
) -> Option<Action> {
    // 弹窗优先级最高：此时所有按键都在回答审批。
    if let Some(dialog) = app.dialog.as_mut() {
        let decision = match key.code {
            KeyCode::Char('a') | KeyCode::Char('y') => {
                Some(ApprovalDecision::AllowOnce)
            }
            KeyCode::Char('s') => Some(ApprovalDecision::AllowAlways),
            KeyCode::Char('d') | KeyCode::Char('n') | KeyCode::Esc => {
                Some(ApprovalDecision::Deny)
            }
            _ => None,
        };
        if let Some(decision) = decision {
            dialog.answer(decision);
            app.dialog = None;
        }
        return None;
    }

    // Ctrl-C / Esc：生成中则中断，空闲则退出（Ctrl-C 需连按两次的体验留给后续版本）。
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(if app.busy {
            Action::Interrupt
        } else {
            Action::Quit
        });
    }
    if key.code == KeyCode::Esc {
        if app.busy {
            return Some(Action::Interrupt);
        }
        app.composer.text.clear();
        app.composer.cursor = 0;
        return None;
    }

    match key.code {
        KeyCode::Enter => {
            let text = app.composer.take();
            if text.trim().is_empty() {
                return None;
            }
            if let Some(action) = resolve_slash(&text) {
                return Some(action);
            }
            app.push_user(text.trim().to_string());
            app.busy = true;
            let _ = commands.send(UiCommand::Prompt(text));
            None
        }
        // Ctrl-J 插入换行（终端里 Shift+Enter 不可靠，用 Ctrl-J 是通用约定）。
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.composer.insert('\n');
            None
        }
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.composer.insert(ch);
            None
        }
        KeyCode::Backspace => {
            app.composer.backspace();
            None
        }
        KeyCode::Delete => {
            app.composer.delete();
            None
        }
        KeyCode::Left => {
            app.composer.move_left();
            None
        }
        KeyCode::Right => {
            app.composer.move_right();
            None
        }
        KeyCode::Home => {
            app.composer.move_home();
            None
        }
        KeyCode::End => {
            app.composer.move_end();
            None
        }
        KeyCode::Up => {
            // 空输入框时上翻历史；有内容时不动，避免抢走多行编辑的方向键。
            if app.composer.is_empty() {
                app.composer.history_prev();
            }
            None
        }
        KeyCode::Down => {
            if app.composer.is_empty() || app.composer.history_index.is_some() {
                app.composer.history_next();
            }
            None
        }
        KeyCode::PageUp => {
            app.scroll_up(5);
            None
        }
        KeyCode::PageDown => {
            app.scroll_down(5);
            None
        }
        _ => None,
    }
}

fn apply_update(app: &mut App, update: UiUpdate) {
    match update {
        UiUpdate::Event(event) => app.on_event(&event),
        UiUpdate::Notice(level, text) => app.push_notice(level, text),
        UiUpdate::TurnFinished {
            usage,
            context_tokens,
            context_window,
        } => app.set_turn_usage(usage, context_tokens, context_window),
        UiUpdate::Reply(text) => {
            for line in text.lines() {
                app.push_notice(crate::runtime::NoticeLevel::Info, line.to_string());
            }
        }
        UiUpdate::Busy(busy) => app.busy = busy,
        UiUpdate::Metadata {
            model,
            thinking,
            approval,
            session_id,
            context_window,
        } => {
            app.model = model;
            app.thinking = thinking;
            app.approval = approval;
            app.session_id = session_id;
            if context_window.is_some() {
                app.context_window = context_window;
            }
        }
        UiUpdate::Error(text) => app.push_error(text),
        UiUpdate::Exited => app.quit = true,
    }
}

fn request_interrupt(interruptor: Option<Arc<dyn SessionInterrupt>>, session_id: String) {
    let Some(interruptor) = interruptor else {
        eprintln!("[lan pi] this transport does not support interrupt");
        return;
    };
    if session_id.is_empty() {
        return;
    }
    tokio::spawn(async move {
        let _ = interruptor.interrupt(&session_id).await;
    });
}

/// 有审批通道时等它，没有时永不完成（等价于关掉这个 `select!` 分支）。
async fn next_prompt(
    slot: &mut Option<&mut tokio::sync::mpsc::UnboundedReceiver<ApprovalPrompt>>,
) -> Option<ApprovalPrompt> {
    match slot {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}

/// 独立线程读键盘：`poll` 只在没有事件时让出 CPU，不影响主循环的 `select!`。
fn spawn_input_thread() -> tokio::sync::mpsc::UnboundedReceiver<KeyEvent> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || loop {
        match event::poll(Duration::from_millis(80)) {
            Ok(true) => match event::read() {
                Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                    if tx.send(key).is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(_) => return,
            },
            Ok(false) => {
                if tx.is_closed() {
                    return;
                }
            }
            Err(_) => return,
        }
    });
    rx
}

/// 终端状态守卫：drop 时一定会恢复 raw mode 与主屏。
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, cursor::Hide)?;
        // 清理顺序与进入相反，避免 Windows 上残留备用屏。
        let terminal = Terminal::new(CrosstermBackend::new(out))?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            cursor::Show
        );
        let _ = self.terminal.show_cursor();
        let _ = std::io::stdout().flush();
    }
}

/// `--snapshot`：渲染一帧到内存并打印。
///
/// 用途有两个：CI 里验证布局没有退化（不需要 TTY），以及在没有交互终端的环境
/// 里给用户看一眼界面长什么样。
pub fn snapshot_text(width: u16, height: u16, dialog: bool) -> String {
    use ratatui::{backend::TestBackend, Terminal};

    let mut app = App::new("anthropic/claude-sonnet-4-5", "20261106-074512-ab12");
    app.push_user("把 pi 的工具卡片改成两行，超长参数省略");
    app.on_event(&crate::event_helper::thinking_delta(
        "先看卡片是怎么拼出来的，再决定改哪一层",
    ));
    app.on_event(&crate::event_helper::tool_start(
        "read",
        "rust/crates/lania-agent/src/lib.rs",
    ));
    app.on_event(&crate::event_helper::tool_end(true));
    app.on_event(&crate::event_helper::text_delta(
        "已定位到 `render.rs`，准备把工具卡片",
    ));
    app.on_event(&crate::event_helper::text_delta(
        "改成「标题行 + 摘要行」。",
    ));
    app.push_notice(
        crate::runtime::NoticeLevel::Warn,
        "context 148/200k over 70% — compacting",
    );
    app.set_turn_usage(
        lania_agent_core::Usage {
            input_tokens: 148_000,
            output_tokens: 912,
            total_tokens: 148_912,
        },
        148_000,
        Some(200_000),
    );
    if dialog {
        let (tx, _rx) = std::sync::mpsc::channel();
        app.dialog = Some(Dialog {
            request: ApprovalRequest::new(
                "write",
                "edit",
                "/repo/rust/crates/lania-agent/src/render.rs",
            ),
            reply: tx,
        });
    }
    app.composer.insert('>');
    app.composer.insert_str("需要我直接改吗？（Ctrl-J 换行）");

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend");
    terminal
        .draw(|frame| view::draw(frame, &app))
        .expect("draw");
    view::buffer_to_text(terminal.backend().buffer())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_renders_expected_chrome() {
        let text = snapshot_text(100, 24, false);
        assert!(text.contains("lan"), "{text}");
        assert!(text.contains("read"), "{text}");
        assert!(text.contains("ctx 74%"), "{text}");
        assert!(
            text.contains("approval always-ask") || text.contains("approval"),
            "{text}"
        );
    }

    #[test]
    fn snapshot_with_dialog_shows_actions() {
        let text = snapshot_text(100, 24, true);
        assert!(text.contains("approval required"), "{text}");
        assert!(text.contains("always allow"), "{text}");
    }

    #[test]
    fn slash_commands_map_to_actions() {
        assert!(matches!(resolve_slash("/quit"), Some(Action::Quit)));
        assert!(resolve_slash("plain text").is_none());
        match resolve_slash("/model gpt-5") {
            Some(Action::Command(UiCommand::SetModel(model))) => assert_eq!(model, "gpt-5"),
            other => panic!("unexpected: {:?}", other.is_some()),
        }
        match resolve_slash("/thinking high") {
            Some(Action::Command(UiCommand::SetThinking(level))) => {
                assert_eq!(level, ThinkingLevel::High)
            }
            _ => panic!("thinking not parsed"),
        }
    }
}
