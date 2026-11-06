//! 会话运行时：把「core 客户端 + 会话文件 + UI 事件」缝在一起。
//!
//! 两条关键设计：
//!
//! 1. **运行时不写终端**。所有输出都通过 [`UiSink`] 出口：
//!    行式 REPL 把它接到 `Renderer`，全屏 TUI 把它接到 mpsc 通道。
//!    这样会话逻辑（落盘、压缩、重建）只有一份实现。
//! 2. **运行时可重建**。`/model`、`/compact`、`/new`、`/fork` 都只改 `SessionSpec`
//!    然后 `close + create(messages)`；Node 侧因此永远是无状态的一次性执行体。
//!
//! 泛型参数 `C` 让整个运行时可以用 `MockSessionClient` 在不起 node 进程的前提下跑通。

use anyhow::{anyhow, Result};
use serde_json::Value;

use lania_agent_core::client::AgentSessionClient;
use lania_agent_core::session::{
    messages_from_branch, user_message_value, SessionEntry, SessionStore,
};
use lania_agent_core::tokens::{estimate_messages_tokens, plan_compaction, should_compact};
use lania_agent_core::{AgentEvent, PromptOutcome, PromptRequest, SessionInfo, SessionSpec, Usage};

/// 提示级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warn,
}

/// UI 出口。运行时只发这三种消息，具体怎么画由调用方决定。
///
/// `Send` 是硬要求：TUI 把运行时搬进独立 task，事件回调必须能跨线程传递。
pub trait UiSink: Send {
    /// 模型/工具事件。
    fn event(&mut self, event: AgentEvent);
    /// 运行时提示（压缩、重建、分叉……）。
    fn notice(&mut self, level: NoticeLevel, text: &str);
    /// 一轮结束后的用量（状态栏用）。默认忽略。
    fn turn_finished(&mut self, _usage: Usage, _context_tokens: u64, _context_window: Option<u64>) {
    }
}

/// 一轮的结果。
#[derive(Debug, Clone, Default)]
pub struct TurnReport {
    pub text: String,
    pub usage: Usage,
    /// 本轮结束后触发了自动压缩。
    pub compacted: bool,
    pub context_tokens: u64,
}

/// 会话运行时。
pub struct PiRuntime<C: AgentSessionClient> {
    client: C,
    store: Option<SessionStore>,
    spec: SessionSpec,
    session_id: String,
    info: SessionInfo,
    /// 当前分支末端条目 id（用于建立 parentId 链）。
    parent: Option<String>,
    seq: u64,
    /// 上一轮实际送入模型的 token 数（比估算准，用于状态栏与压缩判定）。
    last_input_tokens: u64,
    compact_threshold: f32,
    /// 会话 id 的共享槽位：Ctrl-C 监听任务需要知道「当前该中断哪个会话」。
    session_cell: Option<std::sync::Arc<std::sync::Mutex<String>>>,
}

impl<C: AgentSessionClient> PiRuntime<C> {
    /// 创建（或恢复）会话。
    pub async fn open(
        client: C,
        store: Option<SessionStore>,
        spec: SessionSpec,
        compact_threshold: f32,
    ) -> Result<Self> {
        let session_id = spec.session_id.clone();
        let info = client.create(&spec).await?;

        let (parent, seq) = if let Some(store) = store.as_ref() {
            let branch = store.branch(&session_id).unwrap_or_default();
            (
                branch.last().map(|entry| entry.id.clone()),
                branch.len() as u64,
            )
        } else {
            (None, 0)
        };

        Ok(Self {
            client,
            store,
            spec,
            session_id,
            info,
            parent,
            seq,
            last_input_tokens: 0,
            compact_threshold,
            session_cell: None,
        })
    }

    /// 安装会话 id 槽位（供 Ctrl-C 监听任务读取当前会话）。
    pub fn with_session_cell(mut self, cell: std::sync::Arc<std::sync::Mutex<String>>) -> Self {
        if let Ok(mut guard) = cell.lock() {
            *guard = self.session_id.clone();
        }
        self.session_cell = Some(cell);
        self
    }

    fn sync_session_cell(&self) {
        if let Some(cell) = self.session_cell.as_ref() {
            if let Ok(mut guard) = cell.lock() {
                *guard = self.session_id.clone();
            }
        }
    }

    pub fn info(&self) -> &SessionInfo {
        &self.info
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn spec(&self) -> &SessionSpec {
        &self.spec
    }

    pub fn context_tokens(&self) -> u64 {
        if self.last_input_tokens > 0 {
            self.last_input_tokens
        } else {
            self.store
                .as_ref()
                .and_then(|store| store.messages(&self.session_id).ok())
                .map(|messages| estimate_messages_tokens(&messages))
                .unwrap_or_default()
        }
    }

    /// 跑一轮。
    pub async fn run_turn(&mut self, prompt: &str, ui: &mut dyn UiSink) -> Result<TurnReport> {
        self.persist_user_message(prompt);

        let request = PromptRequest::new(self.session_id.clone(), prompt);
        // 闭包只借用 ui，不碰 self —— 否则与 `client.prompt(&self)` 冲突。
        let outcome: PromptOutcome = {
            let ui_ref = &mut *ui;
            let mut sink = move |event: AgentEvent| ui_ref.event(event);
            self.client.prompt(request, &mut sink).await?
        };

        self.persist_outcome(&outcome);
        self.last_input_tokens = outcome.usage.input_tokens;

        let context_tokens = self.context_tokens().max(outcome.usage.input_tokens);
        ui.turn_finished(outcome.usage, context_tokens, self.info.context_window);

        let compacted = self.maybe_compact(ui).await?;
        Ok(TurnReport {
            text: outcome.text,
            usage: outcome.usage,
            compacted,
            context_tokens,
        })
    }

    /// 用当前会话文件里的消息重建 Node 会话（模型切换、压缩、fork 之后都要走一次）。
    pub async fn refresh(&mut self, ui: &mut dyn UiSink) -> Result<()> {
        let messages = match self.store.as_ref() {
            Some(store) => store.messages(&self.session_id)?,
            None => Vec::new(),
        };
        self.spec.messages = messages;
        let _ = self.client.close(&self.session_id).await;
        self.info = self.client.create(&self.spec).await?;
        self.last_input_tokens = 0;
        ui.notice(
            NoticeLevel::Info,
            &format!("session rebuilt · {} messages", self.spec.messages.len()),
        );
        Ok(())
    }

    /// 切换模型。
    pub async fn set_model(
        &mut self,
        provider: Option<&str>,
        model: &str,
        ui: &mut dyn UiSink,
    ) -> Result<()> {
        self.spec.model = Some(model.to_string());
        if let Some(provider) = provider.filter(|value| !value.is_empty()) {
            self.spec.provider = Some(provider.to_string());
        }
        self.refresh(ui).await
    }

    /// 切换思考档位。
    pub async fn set_thinking(
        &mut self,
        thinking: lania_agent_core::ThinkingLevel,
        ui: &mut dyn UiSink,
    ) -> Result<()> {
        self.spec.thinking = thinking;
        self.refresh(ui).await
    }

    /// 切换审批模式。审批判定发生在宿主执行工具时，因此要重建会话把新值送到 Node。
    pub async fn set_approval_mode(
        &mut self,
        mode: lania_agent_core::ApprovalMode,
        ui: &mut dyn UiSink,
    ) -> Result<()> {
        self.spec.approval_mode = mode;
        self.refresh(ui).await
    }

    /// 列出可用模型（`/model` 不带参数时）。
    pub async fn models(&self, provider: Option<&str>) -> Result<Vec<lania_agent_core::ModelInfo>> {
        self.client.models(provider).await
    }

    /// 分叉出新会话并把运行时装上去（`/fork`）。
    pub async fn fork(&mut self, ui: &mut dyn UiSink) -> Result<String> {
        let Some(store) = self.store.as_ref().cloned() else {
            return Err(anyhow!("--no-session 模式无法 fork"));
        };
        let new_id = lania_agent_core::generate_session_id();
        store.fork(&self.session_id, &new_id)?;
        let _ = self.client.close(&self.session_id).await;

        self.session_id = new_id.clone();
        self.sync_session_cell();
        self.spec.session_id = new_id.clone();
        self.spec.messages = store.messages(&new_id)?;
        self.parent = store.branch(&new_id)?.last().map(|entry| entry.id.clone());
        self.info = self.client.create(&self.spec).await?;
        self.last_input_tokens = 0;
        ui.notice(NoticeLevel::Info, &format!("forked into session {new_id}"));
        Ok(new_id)
    }

    /// 开一个新会话（`/new`），旧会话文件保留可 `--resume`。
    pub async fn reset(&mut self, ui: &mut dyn UiSink) -> Result<String> {
        let new_id = lania_agent_core::generate_session_id();
        if let Some(store) = self.store.as_ref().cloned() {
            let header = lania_agent_core::SessionHeader::new(&new_id, self.spec.cwd.clone())
                .with_model(self.spec.model.clone())
                .with_thinking(self.spec.thinking);
            store.create(&header)?;
        }
        let _ = self.client.close(&self.session_id).await;
        self.session_id = new_id.clone();
        self.sync_session_cell();
        self.spec.session_id = new_id.clone();
        self.spec.messages.clear();
        self.parent = None;
        self.last_input_tokens = 0;
        self.info = self.client.create(&self.spec).await?;
        ui.notice(NoticeLevel::Info, &format!("new session {new_id}"));
        Ok(new_id)
    }

    /// 回退 `steps` 条消息（`/rewind [n]`）。
    ///
    /// 只移动 `.leaf` 指针并重建 Node 会话，**不删任何行**——
    /// 回退到之前的位置仍然可以再走回来（数据在文件里，git diff 能看到）。
    pub async fn rewind(&mut self, steps: usize, ui: &mut dyn UiSink) -> Result<String> {
        let Some(store) = self.store.clone() else {
            return Err(anyhow!(
                "rewind needs a session file (running with --no-session)"
            ));
        };
        let branch = store.branch(&self.session_id)?;
        let entries: Vec<&SessionEntry> = branch
            .iter()
            .filter(|entry| entry.message.is_some())
            .collect();
        if entries.is_empty() {
            return Err(anyhow!("nothing to rewind"));
        }
        let keep = entries.len().saturating_sub(steps.max(1));
        if keep == 0 {
            return Err(anyhow!(
                "rewind would empty the conversation ({} messages, asked to drop {steps})",
                entries.len()
            ));
        }
        let target_id = entries[keep - 1].id.clone();
        store.set_leaf(&self.session_id, &target_id)?;
        self.parent = Some(target_id.clone());
        self.refresh(ui).await?;
        ui.notice(
            NoticeLevel::Info,
            &format!(
                "rewound {} message(s) · {} left",
                entries.len() - keep,
                keep
            ),
        );
        Ok(target_id)
    }

    /// 手动压缩（`/compact`）。
    pub async fn compact(&mut self, ui: &mut dyn UiSink) -> Result<bool> {
        if self.store.is_none() {
            ui.notice(
                NoticeLevel::Warn,
                "compaction needs a session file (running with --no-session)",
            );
            return Ok(false);
        }
        let changed = self.write_compaction(ui)?;
        if changed {
            self.refresh(ui).await?;
        }
        Ok(changed)
    }

    /// 当前工作区的会话列表（`/sessions`）。
    pub fn sessions(&self) -> Vec<lania_agent_core::SessionSummary> {
        self.store
            .as_ref()
            .and_then(|store| store.list().ok())
            .unwrap_or_default()
    }

    /// 当前会话的可读状态（`/status`）。
    pub fn status(&self) -> String {
        format!(
            "session {} · model {} · thinking {} · approval {} · context {} tok",
            self.session_id,
            self.info.model,
            self.spec.thinking,
            self.spec.approval_mode,
            self.context_tokens(),
        )
    }

    /// 分支里的消息（诊断用；TUI 用它做历史回放）。
    pub fn messages(&self) -> Result<Vec<Value>> {
        match self.store.as_ref() {
            Some(store) => store.messages(&self.session_id),
            None => Ok(self.spec.messages.clone()),
        }
    }

    /// 释放 Node 侧会话对象。
    ///
    /// 取 `&mut self` 而不是 `&self`：TUI 会把运行时搬进 task，
    /// `&self` 会把 `PiRuntime: Sync` 变成隐形约束，而这里根本不需要共享。
    pub async fn close(&mut self) {
        let _ = self.client.close(&self.session_id).await;
    }

    /// 达到阈值时自动压缩。
    async fn maybe_compact(&mut self, ui: &mut dyn UiSink) -> Result<bool> {
        let Some(window) = self.info.context_window.filter(|window| *window > 0) else {
            return Ok(false);
        };
        let used = self.context_tokens();
        if !should_compact(used, window, self.compact_threshold) {
            return Ok(false);
        }
        ui.notice(
            NoticeLevel::Info,
            &format!(
                "context {used}/{window} over {:.0}% — compacting",
                self.compact_threshold * 100.0
            ),
        );
        if !self.write_compaction(ui)? {
            return Ok(false);
        }
        self.refresh(ui).await?;
        Ok(true)
    }

    /// 写入 `compaction` 条目，返回是否真的写了。
    fn write_compaction(&mut self, ui: &mut dyn UiSink) -> Result<bool> {
        let Some(store) = self.store.clone() else {
            return Ok(false);
        };
        let branch = store.branch(&self.session_id)?;
        let messages = messages_from_branch(&branch);
        let Some(plan) = plan_compaction(&messages, 0.4) else {
            ui.notice(NoticeLevel::Info, "conversation too short to compact");
            return Ok(false);
        };

        // `messages` 可能比「消息类条目」多出一条压缩摘要，需要扣掉偏移。
        let message_entries: Vec<&SessionEntry> = branch
            .iter()
            .filter(|entry| entry.message.is_some())
            .collect();
        let synthetic = messages.len().saturating_sub(message_entries.len());
        let drop_entries = plan.dropped.saturating_sub(synthetic);
        let Some(first_kept) = message_entries.get(drop_entries) else {
            ui.notice(
                NoticeLevel::Warn,
                "compaction aborted: nothing would remain",
            );
            return Ok(false);
        };
        let first_kept_id = first_kept.id.clone();

        let id = format!("c-{}-{}", lania_agent_core::session::now_ms(), self.seq);
        self.seq += 1;
        let entry = SessionEntry::compaction(
            id,
            self.parent.clone(),
            plan.summary,
            Some(first_kept_id),
            plan.tokens_before,
        );
        store.append(&self.session_id, &entry)?;
        self.parent = Some(entry.id.clone());

        ui.notice(
            NoticeLevel::Info,
            &format!(
                "compacted {} messages · {} → {} tok",
                plan.dropped, plan.tokens_before, plan.tokens_after
            ),
        );
        Ok(true)
    }

    fn persist_user_message(&mut self, prompt: &str) {
        // 先克隆 store：`append` 之后还要改 `self.parent` / `self.seq`，
        // 直接借用 `self.store` 会和可变借用冲突。
        let Some(store) = self.store.clone() else {
            return;
        };
        let id = self.next_entry_id();
        let entry = SessionEntry::message(
            "user",
            id.clone(),
            self.parent.clone(),
            user_message_value(prompt),
        );
        // 落盘失败不应中断对话：用户在交互中，丢一条历史比直接崩更可接受。
        if store.append(&self.session_id, &entry).is_ok() {
            self.parent = Some(id);
        }
    }

    fn persist_outcome(&mut self, outcome: &PromptOutcome) {
        let Some(store) = self.store.clone() else {
            return;
        };
        for message in &outcome.messages {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("assistant");
            let kind = match role {
                "user" => "user",
                "assistant" => "assistant",
                "toolResult" | "tool" => "tool",
                other => other,
            };
            let id = self.next_entry_id();
            let entry =
                SessionEntry::message(kind, id.clone(), self.parent.clone(), message.clone());
            if store.append(&self.session_id, &entry).is_ok() {
                self.parent = Some(id);
            }
        }
    }

    fn next_entry_id(&mut self) -> String {
        let id = format!("e-{}-{}", lania_agent_core::session::now_ms(), self.seq);
        self.seq += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lania_agent_core::client::mock::MockSessionClient;
    use lania_agent_core::session::{AgentLayout, SessionHeader};
    use lania_agent_core::ApprovalMode;
    use std::path::PathBuf;

    /// 收集事件的测试 UI。
    #[derive(Default)]
    struct RecordingUi {
        events: Vec<AgentEvent>,
        notices: Vec<(NoticeLevel, String)>,
        last_usage: Option<Usage>,
    }

    impl UiSink for RecordingUi {
        fn event(&mut self, event: AgentEvent) {
            self.events.push(event);
        }
        fn notice(&mut self, level: NoticeLevel, text: &str) {
            self.notices.push((level, text.to_string()));
        }
        fn turn_finished(&mut self, usage: Usage, _ctx: u64, _window: Option<u64>) {
            self.last_usage = Some(usage);
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lania-agent-runtime-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    async fn runtime(
        root: &PathBuf,
    ) -> (
        PiRuntime<MockSessionClient>,
        MockSessionClient,
        SessionStore,
    ) {
        let store = SessionStore::new(root);
        store
            .create(&SessionHeader::new("s1", root.display().to_string()))
            .unwrap();
        let client = MockSessionClient::new();
        let mut spec = SessionSpec::new("s1", root.display().to_string());
        spec.approval_mode = ApprovalMode::Yolo;
        let runtime = PiRuntime::open(client.clone(), Some(store.clone()), spec, 0.7)
            .await
            .unwrap();
        (runtime, client, store)
    }

    #[tokio::test]
    async fn turn_persists_user_and_assistant_entries() {
        let root = temp_root("turn");
        let (mut runtime, _client, store) = runtime(&root).await;
        let mut ui = RecordingUi::default();
        let report = runtime.run_turn("hello", &mut ui).await.unwrap();
        assert_eq!(report.text, "hello");
        assert!(!report.compacted);
        assert_eq!(report.usage.total_tokens, 15);
        assert!(ui.last_usage.is_some());
        assert_eq!(ui.events.len(), 2, "start + text delta");

        let entries = store.entries("s1").unwrap();
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(entries[0].kind, "user");
        assert_eq!(entries[1].kind, "assistant");
        // parentId 链必须接上，否则 /rewind 会断。
        assert_eq!(
            entries[1].parent_id.as_deref(),
            Some(entries[0].id.as_str())
        );
        assert_eq!(store.leaf("s1").as_deref(), Some(entries[1].id.as_str()));
    }

    #[tokio::test]
    async fn refresh_replays_messages_into_new_session() {
        let root = temp_root("refresh");
        let (mut runtime, client, _store) = runtime(&root).await;
        let mut ui = RecordingUi::default();
        runtime.run_turn("first", &mut ui).await.unwrap();
        runtime
            .set_model(Some("mock"), "model-2", &mut ui)
            .await
            .unwrap();

        let specs = client.create_calls.lock().unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[1].model.as_deref(), Some("model-2"));
        // 第二次 create 必须带上历史（user + assistant）。
        assert_eq!(specs[1].messages.len(), 2);
    }

    #[tokio::test]
    async fn compaction_writes_entry_and_rebuilds() {
        let root = temp_root("compact");
        let store = SessionStore::new(&root);
        store.create(&SessionHeader::new("s1", "x")).unwrap();
        // 造 8 轮线性历史，足够触发压缩。
        let mut parent: Option<String> = None;
        for index in 0..8 {
            for (role, id, text) in [
                ("user", format!("u{index}"), format!("question {index}")),
                ("assistant", format!("a{index}"), "x".repeat(400)),
            ] {
                store
                    .append(
                        "s1",
                        &SessionEntry::message(
                            role,
                            id.clone(),
                            parent.clone(),
                            if role == "user" {
                                user_message_value(&text)
                            } else {
                                serde_json::json!({
                                    "role": "assistant",
                                    "content": [{ "type": "text", "text": text }]
                                })
                            },
                        ),
                    )
                    .unwrap();
                parent = Some(id);
            }
        }
        let client = MockSessionClient::new();
        let spec = SessionSpec::new("s1", "x");
        let mut runtime = PiRuntime::open(client.clone(), Some(store.clone()), spec, 0.7)
            .await
            .unwrap();

        let mut ui = RecordingUi::default();
        let compacted = runtime.compact(&mut ui).await.unwrap();
        assert!(compacted, "expected compaction to happen");
        assert!(ui
            .notices
            .iter()
            .any(|(_, text)| text.contains("compacted")));

        let entries = store.entries("s1").unwrap();
        assert!(entries.iter().any(|entry| entry.kind == "compaction"));
        // 重建后的 create 带的是压缩后的消息窗口（比原始 16 条少）。
        let specs = client.create_calls.lock().unwrap();
        let last = specs.last().unwrap();
        assert!(last.messages.len() < 16, "{}", last.messages.len());
    }

    #[tokio::test]
    async fn no_session_store_keeps_working() {
        let client = MockSessionClient::new();
        let spec = SessionSpec::new("s1", "/tmp");
        let mut runtime = PiRuntime::open(client, None, spec, 0.7).await.unwrap();
        let mut ui = RecordingUi::default();
        let report = runtime.run_turn("hi", &mut ui).await.unwrap();
        assert_eq!(report.text, "hello");
        assert!(runtime.messages().unwrap().is_empty());
    }

    #[tokio::test]
    async fn fork_copies_history_and_rebinds_runtime() {
        let root = temp_root("fork");
        let (mut runtime, client, store) = runtime(&root).await;
        let mut ui = RecordingUi::default();
        runtime.run_turn("q", &mut ui).await.unwrap();
        let new_id = runtime.fork(&mut ui).await.unwrap();
        assert_ne!(new_id, "s1");
        assert_eq!(runtime.session_id(), new_id);
        assert_eq!(store.messages(&new_id).unwrap().len(), 2);
        assert_eq!(client.create_count(), 2);
    }

    #[tokio::test]
    async fn rewind_moves_leaf_without_deleting_entries() {
        let root = temp_root("rewind");
        let (mut runtime, _client, store) = runtime(&root).await;
        let mut ui = RecordingUi::default();
        runtime.run_turn("first", &mut ui).await.unwrap();
        runtime.run_turn("second", &mut ui).await.unwrap();
        assert_eq!(store.entries("s1").unwrap().len(), 4);

        runtime.rewind(2, &mut ui).await.unwrap();
        // 只留第一轮（user + assistant）。
        assert_eq!(runtime.messages().unwrap().len(), 2);
        // 数据没被删：文件里仍然是 4 条。
        assert_eq!(store.entries("s1").unwrap().len(), 4);
        assert!(ui.notices.iter().any(|(_, text)| text.contains("rewound")));
    }

    #[tokio::test]
    async fn rewind_refuses_to_empty_the_session() {
        let root = temp_root("rewind-empty");
        let (mut runtime, _client, _store) = runtime(&root).await;
        let mut ui = RecordingUi::default();
        runtime.run_turn("only", &mut ui).await.unwrap();
        let error = runtime.rewind(99, &mut ui).await.unwrap_err();
        assert!(error.to_string().contains("rewind would empty"), "{error}");
    }

    #[test]
    fn layout_helpers_are_consistent() {
        let layout = AgentLayout::new("/repo");
        assert!(layout.session_file("s1").ends_with("s1.jsonl"));
        assert!(layout.leaf_file("s1").ends_with("s1.leaf"));
    }
}
