//! `lania-agent-core`：agent 的**能力层**。
//!
//! **对 lania 宿主零依赖**（硬约束，与 `runex-core` 同一条规矩）：
//! 本 crate 只包含 agent 引擎的类型与算法，不认识 `lania-host` / `lania-command`，
//! 也不认识 bridge 传输。传输由产品层（`lania-agent`）或 CLI 适配层实现。
//!
//! ```text
//! lania-agent-core            ← 零 lania 依赖（本 crate）
//!   ▲                 ▲
//! lania-agent（产品）  lania-plugins-command-pi（CLI 适配）
//!   ▲
//! lania-cli
//! ```
//!
//! 模块职责：
//! - [`spec`]：会话/请求的数据模型；
//! - [`event`]：`event.agent_*` → 强类型事件的唯一映射点；
//! - [`client`]：`AgentSessionClient` 抽象 + mock；
//! - [`session`]：`.lan/agent/sessions/*.jsonl` 的读写；
//! - [`config`]：`custom.agent` 解析；
//! - [`tokens`]：token 估算与压缩计划；
//! - [`prompt`]：system prompt 组装。

pub mod client;
pub mod config;
pub mod event;
pub mod prompt;
pub mod session;
pub mod spec;
pub mod tokens;

pub use client::{AgentSessionClient, EventSink, ModelInfo, SessionInterrupt};
pub use config::{AgentConfig, CustomTool, GuardRuleSpec, Isolation, SubagentConfig};
pub use event::{AgentEvent, Usage};
pub use prompt::{build_system_prompt, ProjectContext, SkillRef};
pub use session::{
    generate_session_id, AgentLayout, SessionEntry, SessionHeader, SessionStore, SessionSummary,
};
pub use spec::{
    ApprovalMode, PromptOutcome, PromptRequest, SessionInfo, SessionSpec, SessionState,
    ThinkingLevel, ToolTier,
};

/// 本 crate 的版本（供 `lan pi --version` 之类的诊断输出使用）。
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
