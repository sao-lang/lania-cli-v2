//! TUI 与运行时之间的 actor 边界。
//!
//! 为什么不是「UI 直接调运行时」：
//! - `run_turn` 会在 `await` 上停留很久，期间界面必须继续刷新、还能响应 Ctrl-C；
//! - 所以把运行时搬进一个独立 task（拥有 `NodeBridgeClient` 的 clone，`'static`），
//!   UI 只发 [`UiCommand`]、只收 [`UiUpdate`]。
//!
//! 这带来的一个额外好处：`/model`、`/compact`、`/fork` 都是**串行**处理的，
//! 不存在「用户在生成中切模型」把 Node 侧状态搞乱的可能。

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use lania_agent_core::{AgentEvent, ApprovalMode, ThinkingLevel, Usage};

use crate::runtime::{NoticeLevel, PiRuntime, UiSink};

/// UI → 运行时。
#[derive(Debug, Clone)]
pub enum UiCommand {
    Prompt(String),
    SetModel(String),
    SetThinking(ThinkingLevel),
    /// 不带参数时循环到下一档。
    CycleThinking,
    SetApproval(ApprovalMode),
    Compact,
    /// 回退 n 条消息（`/rewind [n]`）。
    Rewind(usize),
    New,
    Fork,
    Status,
    Sessions,
    Help,
    Quit,
}

/// 运行时 → UI。
#[derive(Debug, Clone)]
pub enum UiUpdate {
    Event(AgentEvent),
    Notice(NoticeLevel, String),
    TurnFinished {
        usage: Usage,
        context_tokens: u64,
        context_window: Option<u64>,
    },
    /// 一次性文本输出（`/status`、`/sessions`）。
    Reply(String),
    Busy(bool),
    /// 会话元信息变化（切模型、分叉、新建后同步状态栏）。
    Metadata {
        model: String,
        thinking: String,
        approval: String,
        session_id: String,
        context_window: Option<u64>,
    },
    Error(String),
    Exited,
}

/// 统一的错误上报。
fn report(tx: &UnboundedSender<UiUpdate>, result: anyhow::Result<()>) {
    if let Err(error) = result {
        let _ = tx.send(UiUpdate::Error(format!("{error:#}")));
    }
}

/// 把运行时事件转成消息的 `UiSink`。
struct ChannelUi {
    tx: UnboundedSender<UiUpdate>,
}

impl ChannelUi {
    fn send(&self, update: UiUpdate) {
        // UI 已经关掉时静默丢弃：此时用户按了退出，再报错只会污染终端。
        let _ = self.tx.send(update);
    }
}

impl UiSink for ChannelUi {
    fn event(&mut self, event: AgentEvent) {
        self.send(UiUpdate::Event(event));
    }

    fn notice(&mut self, level: NoticeLevel, text: &str) {
        self.send(UiUpdate::Notice(level, text.to_string()));
    }

    fn turn_finished(&mut self, usage: Usage, context_tokens: u64, context_window: Option<u64>) {
        self.send(UiUpdate::TurnFinished {
            usage,
            context_tokens,
            context_window,
        });
    }
}

/// `/help` 的正文（TUI 里直接进对话区）。
pub const HELP_TEXT: &str = "\
/status             session, model, thinking, approval, context
/model <id>         switch model (history is replayed)
/thinking [level]   show or cycle minimal|low|medium|high|xhigh|max
/approve [mode]     always-ask | write | yolo
/compact            summarize old messages and rebuild the session
/rewind [n]         drop the last n messages from the branch (data stays on disk)
/new                start a fresh session
/fork               copy this session into a new one
/sessions           list sessions in this workspace
/quit               leave

keys: enter send · ctrl-j newline · pgup/pgdn scroll · esc interrupt · ctrl-c interrupt/quit";

/// 运行时 actor。
pub struct Actor<C: lania_agent_core::AgentSessionClient> {
    runtime: PiRuntime<C>,
    rx: UnboundedReceiver<UiCommand>,
    tx: UnboundedSender<UiUpdate>,
}

impl<C: lania_agent_core::AgentSessionClient> Actor<C> {
    pub fn new(
        runtime: PiRuntime<C>,
        rx: UnboundedReceiver<UiCommand>,
        tx: UnboundedSender<UiUpdate>,
    ) -> Self {
        Self { runtime, rx, tx }
    }

    /// 主循环：一条命令跑完再收下一条。
    pub async fn run(mut self) {
        let tx = self.tx.clone();
        let mut ui = ChannelUi { tx: tx.clone() };

        // 首帧需要知道模型/会话，先同步一次。
        self.emit_metadata();

        while let Some(command) = self.rx.recv().await {
            match command {
                UiCommand::Quit => {
                    self.runtime.close().await;
                    let _ = tx.send(UiUpdate::Exited);
                    return;
                }
                UiCommand::Prompt(text) => {
                    let _ = tx.send(UiUpdate::Busy(true));
                    match self.runtime.run_turn(&text, &mut ui).await {
                        Ok(report) => {
                            if report.compacted {
                                let _ = tx.send(UiUpdate::Notice(
                                    NoticeLevel::Info,
                                    "history compacted".into(),
                                ));
                            }
                        }
                        Err(error) => {
                            let _ = tx.send(UiUpdate::Error(format!("{error:#}")));
                        }
                    }
                    let _ = tx.send(UiUpdate::Busy(false));
                    self.emit_metadata();
                }
                UiCommand::SetModel(model) => {
                    report(&tx, self.runtime.set_model(None, &model, &mut ui).await);
                    self.emit_metadata();
                }
                UiCommand::SetThinking(level) => {
                    report(&tx, self.runtime.set_thinking(level, &mut ui).await);
                    self.emit_metadata();
                }
                UiCommand::CycleThinking => {
                    let next = self.runtime.spec().thinking.next();
                    report(&tx, self.runtime.set_thinking(next, &mut ui).await);
                    self.emit_metadata();
                }
                UiCommand::SetApproval(mode) => {
                    report(&tx, self.runtime.set_approval_mode(mode, &mut ui).await);
                    self.emit_metadata();
                }
                UiCommand::Compact => {
                    let _ = tx.send(UiUpdate::Busy(true));
                    report(&tx, self.runtime.compact(&mut ui).await.map(|_| ()));
                    let _ = tx.send(UiUpdate::Busy(false));
                }
                UiCommand::Rewind(steps) => {
                    let _ = tx.send(UiUpdate::Busy(true));
                    report(&tx, self.runtime.rewind(steps, &mut ui).await.map(|_| ()));
                    let _ = tx.send(UiUpdate::Busy(false));
                    self.emit_metadata();
                }
                UiCommand::New => {
                    report(&tx, self.runtime.reset(&mut ui).await.map(|_| ()));
                    self.emit_metadata();
                }
                UiCommand::Fork => {
                    report(&tx, self.runtime.fork(&mut ui).await.map(|_| ()));
                    self.emit_metadata();
                }
                UiCommand::Status => {
                    let _ = tx.send(UiUpdate::Reply(self.runtime.status()));
                }
                UiCommand::Sessions => {
                    let sessions = self.runtime.sessions();
                    let text = if sessions.is_empty() {
                        "no sessions yet".to_string()
                    } else {
                        sessions
                            .iter()
                            .take(20)
                            .map(|session| {
                                format!(
                                    "{} · {} entries · {}",
                                    session.session_id,
                                    session.entry_count,
                                    session.model.as_deref().unwrap_or("(default model)")
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let _ = tx.send(UiUpdate::Reply(text));
                }
                UiCommand::Help => {
                    let _ = tx.send(UiUpdate::Reply(HELP_TEXT.to_string()));
                }
            }
        }
    }

    fn emit_metadata(&self) {
        let info = self.runtime.info();
        let spec = self.runtime.spec();
        let _ = self.tx.send(UiUpdate::Metadata {
            model: info.model.clone(),
            thinking: spec.thinking.to_string(),
            approval: spec.approval_mode.to_string(),
            session_id: self.runtime.session_id().to_string(),
            context_window: info.context_window,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn actor_streams_a_turn_and_reports_metadata() {
        use lania_agent_core::client::mock::MockSessionClient;
        use lania_agent_core::SessionSpec;

        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel();
        let client = MockSessionClient::new();
        let spec = SessionSpec::new("s1", "/tmp");
        let runtime = PiRuntime::open(client, None, spec, 0.7).await.unwrap();
        let handle = tokio::spawn(Actor::new(runtime, command_rx, update_tx).run());

        command_tx
            .send(UiCommand::Prompt("hi".into()))
            .expect("send prompt");

        let mut events = 0usize;
        let mut metadata = 0usize;
        loop {
            let update = update_rx.recv().await.expect("update");
            match update {
                UiUpdate::Event(_) => events += 1,
                UiUpdate::Metadata { .. } => metadata += 1,
                UiUpdate::Busy(false) => break,
                _ => {}
            }
        }
        // 首帧元信息 + 一轮结束后的元信息。
        assert!(metadata >= 1, "metadata updates: {metadata}");
        assert!(events >= 2, "events: {events}");

        command_tx.send(UiCommand::Quit).expect("quit");
        // actor 收到 Quit 后应该主动发 Exited。
        let mut exited = false;
        while let Some(update) = update_rx.recv().await {
            if matches!(update, UiUpdate::Exited) {
                exited = true;
                break;
            }
        }
        assert!(exited, "actor must report Exited on quit");
        let _ = handle.await;
    }
}
