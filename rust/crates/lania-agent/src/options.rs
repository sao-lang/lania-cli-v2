//! 产品级选项：**不依赖任何 CLI 框架**。
//!
//! `lan pi`（`lania-plugins-command-pi`）和独立二进制各自把自己那边的输入
//! （`CommandExecutionContext` / clap）映射成 [`AgentOptions`]，
//! 产品只认这个结构。

use lania_agent_core::{AgentConfig, ApprovalMode, ThinkingLevel};

/// 一次 agent 运行的完整输入。
#[derive(Debug, Clone)]
pub struct AgentOptions {
    pub cwd: String,
    /// 位置参数拼成的 prompt；有值时走一次性模式。
    pub prompt: Option<String>,
    pub print: bool,
    pub continue_latest: bool,
    pub resume: Option<String>,
    pub new_session: bool,
    pub no_session: bool,
    pub list_sessions: bool,
    pub json: bool,
    pub no_tui: bool,
    pub snapshot: bool,
    pub verbose: bool,

    // 来自 `custom.agent` 的配置（由适配层填入）
    pub model: Option<String>,
    pub provider: Option<String>,
    pub thinking: ThinkingLevel,
    pub approval_mode: ApprovalMode,
    pub max_tokens: Option<u64>,
    pub system_prompt: Option<String>,
    pub skill_dirs: Vec<String>,
    pub memory_enabled: bool,
    pub compact_threshold: f32,
    pub build_tool: Option<String>,
}

impl Default for AgentOptions {
    fn default() -> Self {
        let config = AgentConfig::default();
        Self {
            cwd: ".".into(),
            prompt: None,
            print: false,
            continue_latest: false,
            resume: None,
            new_session: false,
            no_session: false,
            list_sessions: false,
            json: false,
            no_tui: false,
            snapshot: false,
            verbose: false,
            model: config.model,
            provider: config.provider,
            thinking: config.thinking,
            approval_mode: config.approval_mode,
            max_tokens: config.max_tokens,
            system_prompt: config.system_prompt,
            skill_dirs: config.skill_dirs,
            memory_enabled: config.memory_enabled,
            compact_threshold: config.compact_threshold,
            build_tool: None,
        }
    }
}

impl AgentOptions {
    /// 从 `custom.agent` 配置生成基线；CLI 参数由调用方覆盖字段。
    pub fn from_config(cwd: impl Into<String>, config: &AgentConfig) -> Self {
        Self {
            cwd: cwd.into(),
            model: config.model.clone(),
            provider: config.provider.clone(),
            thinking: config.thinking,
            approval_mode: config.approval_mode,
            max_tokens: config.max_tokens,
            system_prompt: config.system_prompt.clone(),
            skill_dirs: config.skill_dirs.clone(),
            memory_enabled: config.memory_enabled,
            compact_threshold: config.compact_threshold,
            ..Self::default()
        }
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = cwd.into();
        self
    }

    pub fn with_prompt(mut self, prompt: Option<String>) -> Self {
        self.prompt = prompt;
        self
    }

    /// 非交互模式：有 prompt 或显式 `--print`。
    pub fn is_one_shot(&self) -> bool {
        self.print || self.prompt.is_some()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.continue_latest && self.resume.is_some() {
            anyhow::bail!("--continue and --resume cannot be combined; pick one");
        }
        if self.no_session && (self.continue_latest || self.resume.is_some()) {
            anyhow::bail!("--no-session cannot be combined with --continue / --resume");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_is_implied_by_prompt_or_print() {
        assert!(AgentOptions {
            prompt: Some("hi".into()),
            ..AgentOptions::default()
        }
        .is_one_shot());
        assert!(AgentOptions {
            print: true,
            ..AgentOptions::default()
        }
        .is_one_shot());
        assert!(!AgentOptions::default().is_one_shot());
    }

    #[test]
    fn conflicting_session_flags_are_rejected() {
        let conflicting = AgentOptions {
            continue_latest: true,
            resume: Some("abc".into()),
            ..AgentOptions::default()
        };
        assert!(conflicting.validate().is_err());

        let no_session = AgentOptions {
            no_session: true,
            continue_latest: true,
            ..AgentOptions::default()
        };
        assert!(no_session.validate().is_err());
        assert!(AgentOptions::default().validate().is_ok());
    }

    #[test]
    fn config_seeds_defaults() {
        let config = AgentConfig {
            model: Some("m".into()),
            thinking: ThinkingLevel::High,
            skill_dirs: vec![".lan/skills".into()],
            ..AgentConfig::default()
        };
        let options = AgentOptions::from_config("/repo", &config);
        assert_eq!(options.cwd, "/repo");
        assert_eq!(options.model.as_deref(), Some("m"));
        assert_eq!(options.thinking, ThinkingLevel::High);
        assert_eq!(options.skill_dirs.len(), 1);
    }
}
