//! 会话描述与请求模型：跨「命令层 / 能力层 / bridge」传递的纯数据结构。
//!
//! 这一层刻意不引入任何 I/O：结构体可以直接在单测里构造，
//! 由 [`crate::client::AgentSessionClient`] 的实现负责翻译成 bridge 参数。

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 工具的能力层级。层级决定「是否需要审批」，而不是决定「能不能执行」——
/// 后者只由宿主的策略快照决定（见 `lania-host` 的 `agent.tool.exec`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTier {
    Read,
    Write,
    Exec,
}

impl ToolTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Exec => "exec",
        }
    }
}

impl fmt::Display for ToolTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 审批模式：命令行 `--approval-mode` 的取值。
///
/// 语义与 omp 对齐（`always-ask` / `write` / `yolo`），但判定收敛在宿主：
/// 命令层只把模式透传，真正“放不放行”由 Rust 的 `agent.tool.exec` 同一次调用内完成。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    /// 读自动放行，写与执行都要问。
    #[default]
    AlwaysAsk,
    /// 读 + 写自动放行，执行要问。
    Write,
    /// 全部放行（等价 omp 的 yolo）。
    Yolo,
}

impl ApprovalMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AlwaysAsk => "always-ask",
            Self::Write => "write",
            Self::Yolo => "yolo",
        }
    }

    /// 宽容解析：未知值返回 `None`，由调用方决定是报错还是回落默认值。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "always-ask" | "always_ask" | "ask" => Some(Self::AlwaysAsk),
            "write" | "auto-write" | "auto_edit" | "auto-edit" => Some(Self::Write),
            "yolo" | "auto" | "never" | "bypass" => Some(Self::Yolo),
            _ => None,
        }
    }

    /// 该模式下某层级是否**免审批**。
    ///
    /// 注意：这是「宿主的判定」的投影，仅用于命令层提前决定是否展示审批提示。
    /// 安全边界永远在 `lania-host`，不在这里。
    pub fn auto_allows(self, tier: ToolTier) -> bool {
        match self {
            Self::Yolo => true,
            Self::Write => matches!(tier, ToolTier::Read | ToolTier::Write),
            Self::AlwaysAsk => tier == ToolTier::Read,
        }
    }
}

impl fmt::Display for ApprovalMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 推理档位（pi 的 `thinkingLevel`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    XHigh,
    Max,
}

impl ThinkingLevel {
    pub const ALL: [Self; 6] = [
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
        Self::Max,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|level| level.as_str() == normalized)
    }

    /// 循环到下一档，用于 `/thinking` 之类「不带参数就切换」的交互。
    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|level| *level == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

impl fmt::Display for ThinkingLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 创建会话所需的全部输入。
///
/// `messages` 是「续接」的关键：历史由 Rust 侧从 `.lan/agent/sessions/*.jsonl`
/// 还原后一次性传给 Node，Node 不落盘、不做迁移。
#[derive(Debug, Clone, Default)]
pub struct SessionSpec {
    pub session_id: String,
    pub cwd: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: ThinkingLevel,
    pub approval_mode: ApprovalMode,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u64>,
    /// pi 形状的历史消息（原样回放，保证无损）。
    pub messages: Vec<Value>,
}

impl SessionSpec {
    pub fn new(session_id: impl Into<String>, cwd: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            cwd: cwd.into(),
            ..Self::default()
        }
    }

    /// 翻译成 `agent.session.create` 的 params。
    pub fn create_params(&self) -> Value {
        json!({
            "sessionId": self.session_id,
            "cwd": self.cwd,
            "provider": self.provider,
            "model": self.model,
            "thinking": self.thinking.as_str(),
            "approvalMode": self.approval_mode.as_str(),
            "systemPrompt": self.system_prompt,
            "maxTokens": self.max_tokens,
            "messages": self.messages,
        })
    }
}

/// 一轮提问。
#[derive(Debug, Clone)]
pub struct PromptRequest {
    pub session_id: String,
    pub prompt: String,
}

impl PromptRequest {
    pub fn new(session_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            prompt: prompt.into(),
        }
    }

    pub fn params(&self) -> Value {
        json!({ "sessionId": self.session_id, "prompt": self.prompt })
    }
}

/// 会话创建成功后的回执。
#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    pub session_id: String,
    pub model: String,
    pub context_window: Option<u64>,
    pub tools: Vec<String>,
}

impl SessionInfo {
    pub fn from_result(session_id: &str, result: &Value) -> Self {
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            session_id: session_id.to_string(),
            model: result
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            context_window: result.get("contextWindow").and_then(Value::as_u64),
            tools,
        }
    }
}

/// 一轮结束后的产出。
#[derive(Debug, Clone, Default)]
pub struct PromptOutcome {
    /// 本轮的最终文本（用于落盘/非交互输出）。
    pub text: String,
    /// 本轮新增的 pi 消息（assistant + tool result），按顺序 append 到会话文件。
    pub messages: Vec<Value>,
    pub usage: crate::event::Usage,
    pub stop_reason: Option<String>,
}

/// `/model`、`/thinking` 之类运行期切换的回执。
#[derive(Debug, Clone, Default)]
pub struct SessionState {
    pub is_streaming: bool,
    pub pending_tool_calls: usize,
    pub context_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_modes_parse_aliases() {
        assert_eq!(
            ApprovalMode::parse("always-ask"),
            Some(ApprovalMode::AlwaysAsk)
        );
        assert_eq!(ApprovalMode::parse("YOLO"), Some(ApprovalMode::Yolo));
        assert_eq!(ApprovalMode::parse("auto-edit"), Some(ApprovalMode::Write));
        assert_eq!(ApprovalMode::parse("nonsense"), None);
    }

    #[test]
    fn approval_matrix_matches_host_policy() {
        // 与 lania-host::agent_tools::auto_approved 保持一致的矩阵（改一处必须改另一处）。
        assert!(ApprovalMode::AlwaysAsk.auto_allows(ToolTier::Read));
        assert!(!ApprovalMode::AlwaysAsk.auto_allows(ToolTier::Write));
        assert!(!ApprovalMode::AlwaysAsk.auto_allows(ToolTier::Exec));
        assert!(ApprovalMode::Write.auto_allows(ToolTier::Write));
        assert!(!ApprovalMode::Write.auto_allows(ToolTier::Exec));
        assert!(ApprovalMode::Yolo.auto_allows(ToolTier::Exec));
    }

    #[test]
    fn thinking_levels_cycle() {
        assert_eq!(ThinkingLevel::Max.next(), ThinkingLevel::Minimal);
        assert_eq!(ThinkingLevel::parse("xhigh"), Some(ThinkingLevel::XHigh));
    }

    #[test]
    fn create_params_are_camel_case() {
        let spec = SessionSpec::new("s1", "/repo");
        let params = spec.create_params();
        assert_eq!(params["sessionId"], "s1");
        assert_eq!(params["thinking"], "medium");
        assert_eq!(params["approvalMode"], "always-ask");
        assert!(params["messages"].as_array().unwrap().is_empty());
    }
}
