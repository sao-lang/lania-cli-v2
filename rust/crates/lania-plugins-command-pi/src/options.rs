//! `lan pi` 的 argv 解析：**只属于 CLI 适配层**，产品不认这些字段。
//!
//! 解析结果通过 [`CliArgs::apply_to`] 覆盖到 [`AgentOptions`] 上，
//! 这样产品侧永远只有一份「已解析」的输入。

use anyhow::Result;
use lania_agent::AgentOptions;
use lania_agent_core::{ApprovalMode, ThinkingLevel};
use lania_host::execution::CommandExecutionContext;
use serde_json::Value;

/// 命令行参数（与 `custom.agent` 配置分离）。
#[derive(Debug, Clone, Default)]
pub struct CliArgs {
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub thinking: Option<ThinkingLevel>,
    pub approval_mode: Option<ApprovalMode>,
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
}

impl CliArgs {
    pub fn from_context(ctx: &CommandExecutionContext<'_>) -> Result<Self> {
        let thinking = match option_string(ctx, "thinking") {
            Some(raw) => Some(
                ThinkingLevel::parse(&raw)
                    .ok_or_else(|| anyhow::anyhow!("unknown thinking level `{raw}`"))?,
            ),
            None => None,
        };
        let approval_mode = match option_string(ctx, "approval-mode") {
            Some(raw) => Some(
                ApprovalMode::parse(&raw)
                    .ok_or_else(|| anyhow::anyhow!("unknown approval mode `{raw}`"))?,
            ),
            None => None,
        };
        let args = Self {
            prompt: positional_prompt(ctx),
            model: option_string(ctx, "model"),
            provider: option_string(ctx, "provider"),
            thinking,
            approval_mode,
            print: option_bool(ctx, "print"),
            continue_latest: option_bool(ctx, "continue"),
            resume: option_string(ctx, "resume"),
            new_session: option_bool(ctx, "new"),
            no_session: option_bool(ctx, "no-session"),
            list_sessions: option_bool(ctx, "sessions"),
            json: option_bool(ctx, "json"),
            no_tui: option_bool(ctx, "no-tui"),
            snapshot: option_bool(ctx, "snapshot"),
            verbose: option_bool(ctx, "verbose"),
        };
        args.validate()?;
        Ok(args)
    }

    /// 互斥检查放在「合并前」，报错信息才能提到具体开关名。
    fn validate(&self) -> Result<()> {
        if self.continue_latest && self.resume.is_some() {
            anyhow::bail!("--continue and --resume cannot be combined; pick one");
        }
        if self.no_session && (self.continue_latest || self.resume.is_some()) {
            anyhow::bail!("--no-session cannot be combined with --continue / --resume");
        }
        Ok(())
    }

    /// 覆盖到产品选项上（CLI > 配置）。
    pub fn apply_to(&self, options: &mut AgentOptions) {
        options.prompt = self.prompt.clone();
        if let Some(model) = self.model.as_ref() {
            options.model = Some(model.clone());
        }
        if let Some(provider) = self.provider.as_ref() {
            options.provider = Some(provider.clone());
        }
        if let Some(thinking) = self.thinking {
            options.thinking = thinking;
        }
        if let Some(mode) = self.approval_mode {
            options.approval_mode = mode;
        }
        options.print = self.print;
        options.continue_latest = self.continue_latest;
        options.resume = self.resume.clone();
        options.new_session = self.new_session;
        options.no_session = self.no_session;
        options.list_sessions = self.list_sessions;
        options.json = self.json;
        options.no_tui = self.no_tui;
        options.snapshot = self.snapshot;
        options.verbose = self.verbose;
    }
}

pub fn option_string(ctx: &CommandExecutionContext<'_>, key: &str) -> Option<String> {
    ctx.command()
        .argv
        .options
        .get(key)
        .and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn option_bool(ctx: &CommandExecutionContext<'_>, key: &str) -> bool {
    ctx.bool_option(key)
}

pub fn positional_prompt(ctx: &CommandExecutionContext<'_>) -> Option<String> {
    let value = ctx.command().argv.args.get("prompt")?;
    let parts: Vec<String> = match value {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        Value::String(text) => vec![text.clone()],
        _ => Vec::new(),
    };
    let joined = parts.join(" ").trim().to_string();
    (!joined.is_empty()).then_some(joined)
}
