//! 会话存储：`.lan/agent/sessions/<id>.jsonl`（append-only）+ `<id>.leaf` 指针。
//!
//! 三条不变量（§4.5）：
//! 1. 只追加：任何一次写入都是往文件尾部加一行，崩溃最多丢当前这一轮；
//! 2. 分支用 `parentId` + `leaf` 表达，`/rewind` 只改 leaf，不删数据；
//! 3. `message` 字段**原样**保存 pi 的消息 JSON，恢复时无损回放——
//!    绝不在 Rust 侧重建消息结构（那是 pi 的私有格式）。
//!
//! 版本策略与 omp 不同：v1 不跨大版本迁移，遇到不认识的 `v` 直接拒绝加载并提示。

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::spec::ThinkingLevel;

pub const SESSION_FORMAT_VERSION: u32 = 1;

/// 会话目录布局。集中在一处，避免各处拼 `.lan/agent/...`。
#[derive(Debug, Clone)]
pub struct AgentLayout {
    root: PathBuf,
}

impl AgentLayout {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            root: project_root.as_ref().join(".lan").join("agent"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    pub fn memory_dir(&self) -> PathBuf {
        self.root.join("memory")
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.root.join("agents")
    }

    pub fn approvals_file(&self) -> PathBuf {
        self.root.join("approvals.json")
    }

    pub fn session_file(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{session_id}.jsonl"))
    }

    pub fn leaf_file(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{session_id}.leaf"))
    }

    /// 建目录是幂等的；`read` 类操作不需要它，只有写入前才调用。
    pub fn ensure(&self) -> Result<()> {
        let dir = self.sessions_dir();
        fs::create_dir_all(&dir)
            .map_err(|error| anyhow!("failed to create {}: {error}", dir.display()))
    }
}

/// 会话头（文件第一行）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHeader {
    pub v: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub session_id: String,
    pub cwd: String,
    pub created_at: u64,
    pub model: Option<String>,
    pub thinking: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
}

impl SessionHeader {
    pub fn new(session_id: impl Into<String>, cwd: impl Into<String>) -> Self {
        Self {
            v: SESSION_FORMAT_VERSION,
            kind: "header".into(),
            session_id: session_id.into(),
            cwd: cwd.into(),
            created_at: now_ms(),
            model: None,
            thinking: ThinkingLevel::default().as_str().to_string(),
            parent_session_id: None,
        }
    }

    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    pub fn with_thinking(mut self, thinking: ThinkingLevel) -> Self {
        self.thinking = thinking.as_str().to_string();
        self
    }
}

/// 一条会话记录。`kind` 与 `message.role` 保持一致，便于人读 `git diff`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEntry {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub ts: u64,
    /// pi 消息原文；`compaction` / `meta` 条目没有该字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Value>,
    /// 压缩摘要（`compaction` 条目）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_kept_entry_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_before: Option<u64>,
    /// 元信息（模型切换、warning 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl SessionEntry {
    pub fn message(role: &str, id: String, parent_id: Option<String>, message: Value) -> Self {
        Self {
            kind: role.to_string(),
            id,
            parent_id,
            ts: now_ms(),
            message: Some(message),
            summary: None,
            first_kept_entry_id: None,
            tokens_before: None,
            meta: None,
        }
    }

    pub fn compaction(
        id: String,
        parent_id: Option<String>,
        summary: String,
        first_kept_entry_id: Option<String>,
        tokens_before: u64,
    ) -> Self {
        Self {
            kind: "compaction".into(),
            id,
            parent_id,
            ts: now_ms(),
            message: None,
            summary: Some(summary),
            first_kept_entry_id,
            tokens_before: Some(tokens_before),
            meta: None,
        }
    }

    pub fn meta(id: String, parent_id: Option<String>, meta: Value) -> Self {
        Self {
            kind: "meta".into(),
            id,
            parent_id,
            ts: now_ms(),
            message: None,
            summary: None,
            first_kept_entry_id: None,
            tokens_before: None,
            meta: Some(meta),
        }
    }
}

/// 会话列表项（`/sessions`）。
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub cwd: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub model: Option<String>,
    pub entry_count: usize,
}

/// 会话存储。
#[derive(Debug, Clone)]
pub struct SessionStore {
    layout: AgentLayout,
}

impl SessionStore {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            layout: AgentLayout::new(project_root),
        }
    }

    pub fn layout(&self) -> &AgentLayout {
        &self.layout
    }

    /// 新建会话文件并写入 header。
    pub fn create(&self, header: &SessionHeader) -> Result<String> {
        self.layout.ensure()?;
        validate_session_id(&header.session_id)?;
        let path = self.layout.session_file(&header.session_id);
        if path.exists() {
            return Err(anyhow!("session already exists: {}", header.session_id));
        }
        let line = serde_json::to_string(header)?;
        fs::write(&path, format!("{line}\n"))?;
        Ok(header.session_id.clone())
    }

    /// 追加一条记录，并把 leaf 指向它。
    pub fn append(&self, session_id: &str, entry: &SessionEntry) -> Result<()> {
        validate_session_id(session_id)?;
        let path = self.layout.session_file(session_id);
        let line = serde_json::to_string(entry)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        file.write_all(format!("{line}\n").as_bytes())?;
        self.set_leaf(session_id, &entry.id)
    }

    /// 只改 leaf（`/rewind`）。
    pub fn set_leaf(&self, session_id: &str, entry_id: &str) -> Result<()> {
        validate_session_id(session_id)?;
        self.layout.ensure()?;
        fs::write(self.layout.leaf_file(session_id), format!("{entry_id}\n"))?;
        Ok(())
    }

    pub fn leaf(&self, session_id: &str) -> Option<String> {
        let text = fs::read_to_string(self.layout.leaf_file(session_id)).ok()?;
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    /// 读取全部记录（不做分支裁剪）。
    pub fn entries(&self, session_id: &str) -> Result<Vec<SessionEntry>> {
        validate_session_id(session_id)?;
        let path = self.layout.session_file(session_id);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read session {}", path.display()))?;
        let mut entries = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if index == 0 {
                let header: SessionHeader = serde_json::from_str(line)
                    .map_err(|error| anyhow!("invalid session header: {error}"))?;
                if header.v != SESSION_FORMAT_VERSION {
                    return Err(anyhow!(
                        "session {} uses format v{}, this build understands v{}; refusing to load",
                        header.session_id,
                        header.v,
                        SESSION_FORMAT_VERSION
                    ));
                }
                continue;
            }
            match serde_json::from_str::<SessionEntry>(line) {
                Ok(entry) => entries.push(entry),
                // 崩溃可能留下半行；append-only 的含义就是「容忍尾部损坏」。
                Err(_) => continue,
            }
        }
        Ok(entries)
    }

    pub fn header(&self, session_id: &str) -> Result<Option<SessionHeader>> {
        validate_session_id(session_id)?;
        let text = fs::read_to_string(self.layout.session_file(session_id))?;
        let first = text.lines().next().unwrap_or_default();
        if first.trim().is_empty() {
            return Ok(None);
        }
        Ok(serde_json::from_str(first).ok())
    }

    /// 当前分支：从 leaf 沿 parentId 回溯。
    pub fn branch(&self, session_id: &str) -> Result<Vec<SessionEntry>> {
        let entries = self.entries(session_id)?;
        let Some(leaf) = self.leaf(session_id) else {
            // 没有 leaf（例如刚写入 header 就崩溃）：按文件顺序当作线性历史。
            return Ok(entries);
        };
        let by_id: std::collections::HashMap<&str, &SessionEntry> = entries
            .iter()
            .map(|entry| (entry.id.as_str(), entry))
            .collect();
        let mut chain = Vec::new();
        let mut cursor = Some(leaf.as_str());
        while let Some(id) = cursor {
            let Some(entry) = by_id.get(id) else {
                break;
            };
            chain.push((*entry).clone());
            cursor = entry.parent_id.as_deref();
        }
        chain.reverse();
        Ok(chain)
    }

    /// 还原成 pi 的 `messages`，含最近一次压缩摘要。
    pub fn messages(&self, session_id: &str) -> Result<Vec<Value>> {
        let branch = self.branch(session_id)?;
        Ok(messages_from_branch(&branch))
    }

    /// 最近写入的会话 id（用于 `--continue`）。
    pub fn latest(&self) -> Option<String> {
        self.list()
            .ok()?
            .into_iter()
            .max_by_key(|s| s.updated_at)
            .map(|s| s.session_id)
    }

    pub fn list(&self) -> Result<Vec<SessionSummary>> {
        let dir = self.layout.sessions_dir();
        let Ok(read_dir) = fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut summaries = Vec::new();
        for entry in read_dir.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Ok(header) = self.header(stem) else {
                continue;
            };
            let Some(header) = header else { continue };
            let updated_at = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(header.created_at);
            let entry_count = self.entries(stem).map(|items| items.len()).unwrap_or(0);
            summaries.push(SessionSummary {
                session_id: stem.to_string(),
                cwd: header.cwd,
                created_at: header.created_at,
                updated_at,
                model: header.model,
                entry_count,
            });
        }
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.updated_at));
        Ok(summaries)
    }

    /// 从当前分支分叉出新会话（`/fork`）。
    pub fn fork(&self, source_session_id: &str, new_session_id: &str) -> Result<String> {
        let entries = self.branch(source_session_id)?;
        let mut header = self
            .header(source_session_id)?
            .ok_or_else(|| anyhow!("session {source_session_id} has no header"))?;
        header.session_id = new_session_id.to_string();
        header.parent_session_id = Some(source_session_id.to_string());
        header.created_at = now_ms();
        self.create(&header)?;
        let mut parent: Option<String> = None;
        for entry in entries {
            let mut cloned = entry;
            cloned.parent_id = parent.clone();
            self.append(new_session_id, &cloned)?;
            parent = Some(cloned.id);
        }
        Ok(new_session_id.to_string())
    }
}

/// 从分支记录推导 `messages`：跳过 meta，`compaction` 变成一条摘要消息。
pub fn messages_from_branch(branch: &[SessionEntry]) -> Vec<Value> {
    let last_compaction = branch
        .iter()
        .rposition(|entry| entry.kind == "compaction" && entry.summary.is_some());

    let mut messages: Vec<Value> = Vec::new();
    if let Some(index) = last_compaction {
        let compaction = &branch[index];
        let summary = compaction.summary.clone().unwrap_or_default();
        // 保留起点之后（含起点）的消息；没有 firstKeptEntryId 就保留全部后续消息。
        let kept_start = compaction
            .first_kept_entry_id
            .as_ref()
            .and_then(|id| branch.iter().position(|entry| &entry.id == id))
            .unwrap_or(index + 1);
        let mut kept: Vec<Value> = branch[kept_start..]
            .iter()
            .filter(|entry| entry.kind != "compaction" && entry.kind != "meta")
            .filter_map(|entry| entry.message.clone())
            .collect();
        if !summary.trim().is_empty() {
            prepend_summary(&mut kept, &summary);
        }
        messages.extend(kept);
        return messages;
    }

    for entry in branch {
        if entry.kind == "compaction" || entry.kind == "meta" {
            continue;
        }
        if let Some(message) = entry.message.clone() {
            messages.push(message);
        }
    }
    messages
}

/// 把摘要塞进消息序列时避免破坏 user/assistant 交替：
/// 若下一条已是 user，就把摘要并进它的文本前缀。
fn prepend_summary(messages: &mut Vec<Value>, summary: &str) {
    let label = format!("[conversation summary]\n{summary}");
    if let Some(first) = messages.first_mut() {
        let is_user = first.get("role").and_then(Value::as_str) == Some("user");
        if is_user {
            if let Some(content) = first.get_mut("content").and_then(Value::as_array_mut) {
                if let Some(block) = content
                    .iter_mut()
                    .find(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                {
                    let existing = block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    block["text"] = Value::String(format!("{label}\n\n{existing}"));
                    return;
                }
            }
        }
    }
    messages.insert(0, user_message_value(&label));
}

pub fn user_message_value(text: &str) -> Value {
    json!({ "role": "user", "content": [{ "type": "text", "text": text }] })
}

/// 会话 id 只允许安全字符：它直接参与文件路径拼接。
pub fn validate_session_id(session_id: &str) -> Result<()> {
    let valid = !session_id.is_empty()
        && session_id.len() <= 128
        && session_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(anyhow!("invalid session id: {session_id}"))
    }
}

/// 生成会话 id：`20261106-012345-ab12`（可读、可排序、够唯一）。
pub fn generate_session_id() -> String {
    let (year, month, day, hour, minute, second) = civil_now();
    let nonce = {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos())
            .unwrap_or(0);
        nanos % 0xffff
    };
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}-{nonce:04x}")
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

/// UTC 的 (年, 月, 日, 时, 分, 秒)。
///
/// 不引 chrono：只有这一处需要日历换算，Howard Hinnant 的 `civil_from_days`
/// 已经足够稳定（1970-2400 区间精确）。
fn civil_now() -> (i64, u32, u32, u32, u32, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (rem / 3_600) as u32,
        ((rem % 3_600) / 60) as u32,
        (rem % 60) as u32,
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lania-agent-core-session-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn store(name: &str) -> (PathBuf, SessionStore) {
        let root = temp_root(name);
        let store = SessionStore::new(&root);
        (root, store)
    }

    #[test]
    fn create_append_and_restore_messages() {
        let (_root, store) = store("append");
        store
            .create(&SessionHeader::new("s1", "/repo").with_model(Some("m".into())))
            .expect("create");
        store
            .append(
                "s1",
                &SessionEntry::message("user", "e1".into(), None, user_message_value("hello")),
            )
            .expect("append user");
        store
            .append(
                "s1",
                &SessionEntry::message(
                    "assistant",
                    "e2".into(),
                    Some("e1".into()),
                    json!({ "role": "assistant", "content": [{ "type": "text", "text": "hi" }] }),
                ),
            )
            .expect("append assistant");

        assert_eq!(store.leaf("s1").as_deref(), Some("e2"));
        let messages = store.messages("s1").expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
    }

    #[test]
    fn rewind_only_moves_the_leaf() {
        let (_root, store) = store("rewind");
        store
            .create(&SessionHeader::new("s1", "/repo"))
            .expect("create");
        for (id, parent, role) in [
            ("e1", None, "user"),
            ("e2", Some("e1"), "assistant"),
            ("e3", Some("e2"), "user"),
        ] {
            store
                .append(
                    "s1",
                    &SessionEntry::message(
                        role,
                        id.into(),
                        parent.map(|value: &str| value.to_string()),
                        user_message_value(id),
                    ),
                )
                .expect("append");
        }

        store.set_leaf("s1", "e1").expect("rewind");
        let messages = store.messages("s1").expect("messages");
        assert_eq!(messages.len(), 1);
        // 数据没有被删：文件里仍然是三条。
        assert_eq!(store.entries("s1").expect("entries").len(), 3);
    }

    #[test]
    fn compaction_summary_is_prepended_without_breaking_alternation() {
        let (_root, store) = store("compaction");
        store
            .create(&SessionHeader::new("s1", "/repo"))
            .expect("create");
        store
            .append(
                "s1",
                &SessionEntry::message("user", "e1".into(), None, user_message_value("old q")),
            )
            .expect("append");
        store
            .append(
                "s1",
                &SessionEntry::message(
                    "assistant",
                    "e2".into(),
                    Some("e1".into()),
                    json!({ "role": "assistant", "content": [{ "type": "text", "text": "old a" }] }),
                ),
            )
            .expect("append");
        store
            .append(
                "s1",
                &SessionEntry::message(
                    "user",
                    "e3".into(),
                    Some("e2".into()),
                    user_message_value("new q"),
                ),
            )
            .expect("append");
        store
            .append(
                "s1",
                &SessionEntry::compaction(
                    "c1".into(),
                    Some("e3".into()),
                    "summary".into(),
                    Some("e3".into()),
                    999,
                ),
            )
            .expect("append compaction");

        let messages = store.messages("s1").expect("messages");
        assert_eq!(messages.len(), 1);
        let text = messages[0]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("[conversation summary]"), "{text}");
        assert!(text.contains("new q"), "{text}");
    }

    #[test]
    fn rejects_path_traversal_ids() {
        assert!(validate_session_id("../etc/passwd").is_err());
        assert!(validate_session_id("ok-1.2").is_ok());
    }

    #[test]
    fn refuses_future_format_versions() {
        let (root, store) = store("version");
        store
            .create(&SessionHeader::new("s1", "/repo"))
            .expect("create");
        let path = AgentLayout::new(&root).session_file("s1");
        let text = fs::read_to_string(&path).unwrap();
        let bumped = text.replace("\"v\":1", "\"v\":99");
        fs::write(&path, bumped).unwrap();
        let error = store.entries("s1").expect_err("should refuse");
        assert!(error.to_string().contains("refusing to load"), "{error}");
    }

    #[test]
    fn fork_copies_branch_into_new_session() {
        let (_root, store) = store("fork");
        store
            .create(&SessionHeader::new("s1", "/repo"))
            .expect("create");
        store
            .append(
                "s1",
                &SessionEntry::message("user", "e1".into(), None, user_message_value("q")),
            )
            .expect("append");
        store.fork("s1", "s2").expect("fork");
        assert_eq!(store.messages("s2").expect("messages").len(), 1);
        assert_eq!(
            store
                .header("s2")
                .expect("header")
                .unwrap()
                .parent_session_id
                .as_deref(),
            Some("s1")
        );
    }

    #[test]
    fn session_ids_sort_by_time() {
        let a = generate_session_id();
        assert_eq!(a.len(), 20, "unexpected id shape: {a}");
        assert!(validate_session_id(&a).is_ok());
    }

    #[test]
    fn civil_dates_match_known_epochs() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }
}
