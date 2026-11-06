//! 配置：`lan.config.*` 的 `custom.agent` 段 → 强类型。
//!
//! v1 不提升为 `LanConfigSnapshot` 的强类型字段（§4.6），
//! 因此这里自己兜底：任何字段缺失/类型不对都回落默认值，绝不因为配置写错而让 `lan pi` 起不来。

use serde_json::Value;

use crate::spec::{ApprovalMode, ThinkingLevel};

/// 子 agent 隔离方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Isolation {
    /// 共享工作区（写操作串行、持全局写锁）。
    #[default]
    None,
    /// 每个子 agent 一个 git worktree。
    Worktree,
}

#[derive(Debug, Clone)]
pub struct SubagentConfig {
    pub max_concurrent: usize,
    pub isolation: Isolation,
    pub max_tokens_per_agent: Option<u64>,
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            isolation: Isolation::None,
            max_tokens_per_agent: None,
        }
    }
}

/// 插件工具声明：`lan <command>` + JSON stdin。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomTool {
    pub name: String,
    pub command: String,
    pub approval: String,
    pub description: Option<String>,
}

/// 工具门禁规则（等价 omp 的 `onAgentToolCall` 拒绝能力）。
///
/// 只做「声明」，判定在宿主（`lania_host::guard`）：
/// 规则不经过 Node，模型无法通过伪造参数绕过。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardRuleSpec {
    pub tool: String,
    pub pattern: Option<String>,
    /// `deny` / `require-approval` / `allow`
    pub action: String,
    pub reason: Option<String>,
}

/// `custom.agent` 的强类型视图。
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub thinking: ThinkingLevel,
    pub approval_mode: ApprovalMode,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u64>,
    pub context_window: Option<u64>,
    /// 触发压缩的上下文占用比例（0..1）。
    pub compact_threshold: f32,
    pub subagents: SubagentConfig,
    pub exec_allow: Vec<String>,
    pub skill_dirs: Vec<String>,
    pub custom_tools: Vec<CustomTool>,
    pub memory_enabled: bool,
    pub guard: Vec<GuardRuleSpec>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: None,
            provider: None,
            thinking: ThinkingLevel::default(),
            approval_mode: ApprovalMode::default(),
            system_prompt: None,
            max_tokens: None,
            context_window: None,
            compact_threshold: 0.7,
            subagents: SubagentConfig::default(),
            exec_allow: Vec::new(),
            skill_dirs: Vec::new(),
            custom_tools: Vec::new(),
            memory_enabled: true,
            guard: Vec::new(),
        }
    }
}

impl AgentConfig {
    /// 从 `LanConfigSnapshot.custom` 解析。
    pub fn from_custom(custom: &Value) -> Self {
        let mut config = Self::default();
        let Some(agent) = custom.get("agent").filter(|value| value.is_object()) else {
            return config;
        };

        config.model = string_field(agent, "model");
        config.provider = string_field(agent, "provider");
        config.system_prompt = string_field(agent, "systemPrompt");
        config.max_tokens = agent.get("maxTokens").and_then(Value::as_u64);
        config.context_window = agent.get("contextWindow").and_then(Value::as_u64);
        config.memory_enabled = agent
            .get("memory")
            .and_then(|memory| memory.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true);

        if let Some(thinking) =
            string_field(agent, "thinking").and_then(|raw| ThinkingLevel::parse(&raw))
        {
            config.thinking = thinking;
        }
        if let Some(mode) =
            string_field(agent, "approvalMode").and_then(|raw| ApprovalMode::parse(&raw))
        {
            config.approval_mode = mode;
        }
        if let Some(threshold) = agent.get("compactThreshold").and_then(Value::as_f64) {
            config.compact_threshold = (threshold as f32).clamp(0.2, 0.95);
        }

        if let Some(subagents) = agent.get("subagents").filter(|value| value.is_object()) {
            if let Some(max) = subagents.get("maxConcurrent").and_then(Value::as_u64) {
                config.subagents.max_concurrent = (max as usize).clamp(1, 8);
            }
            if let Some(isolation) = subagents.get("isolation").and_then(Value::as_str) {
                config.subagents.isolation = if isolation.eq_ignore_ascii_case("worktree") {
                    Isolation::Worktree
                } else {
                    Isolation::None
                };
            }
            config.subagents.max_tokens_per_agent =
                subagents.get("maxTokens").and_then(Value::as_u64);
        }

        config.exec_allow = string_array(agent.get("exec").and_then(|exec| exec.get("allow")));

        if let Some(rules) = agent.get("guard").and_then(Value::as_array) {
            config.guard = rules
                .iter()
                .filter_map(|rule| {
                    let tool = rule.get("tool").and_then(Value::as_str)?.trim();
                    if tool.is_empty() {
                        return None;
                    }
                    Some(GuardRuleSpec {
                        tool: tool.to_string(),
                        pattern: rule
                            .get("pattern")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|pattern| !pattern.is_empty())
                            .map(ToOwned::to_owned),
                        action: rule
                            .get("action")
                            .and_then(Value::as_str)
                            .unwrap_or("deny")
                            .to_string(),
                        reason: rule
                            .get("reason")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                    })
                })
                .collect();
        }
        config.skill_dirs = string_array(agent.get("skills"));

        if let Some(tools) = agent.get("tools").and_then(Value::as_array) {
            config.custom_tools = tools
                .iter()
                .filter_map(|tool| {
                    let name = tool.get("name").and_then(Value::as_str)?.trim();
                    let command = tool.get("command").and_then(Value::as_str)?.trim();
                    if name.is_empty() || command.is_empty() {
                        return None;
                    }
                    Some(CustomTool {
                        name: name.to_string(),
                        command: command.to_string(),
                        approval: tool
                            .get("approval")
                            .and_then(Value::as_str)
                            .unwrap_or("exec")
                            .to_string(),
                        description: tool
                            .get("description")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                    })
                })
                .collect();
        }

        config
    }

    /// 命令行覆盖（优先级：CLI > 配置 > 默认）。
    pub fn apply_overrides(
        &mut self,
        provider: Option<&str>,
        model: Option<&str>,
        thinking: Option<&str>,
        approval_mode: Option<&str>,
    ) {
        if let Some(provider) = provider.filter(|value| !value.trim().is_empty()) {
            self.provider = Some(provider.to_string());
        }
        if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
            self.model = Some(model.to_string());
        }
        if let Some(thinking) = thinking
            .filter(|value| !value.trim().is_empty())
            .and_then(ThinkingLevel::parse)
        {
            self.thinking = thinking;
        }
        if let Some(mode) = approval_mode
            .filter(|value| !value.trim().is_empty())
            .and_then(ApprovalMode::parse)
        {
            self.approval_mode = mode;
        }
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_custom_yields_defaults() {
        let config = AgentConfig::from_custom(&json!({}));
        assert_eq!(config.thinking, ThinkingLevel::Medium);
        assert_eq!(config.approval_mode, ApprovalMode::AlwaysAsk);
        assert_eq!(config.subagents.max_concurrent, 4);
        assert!(config.memory_enabled);
    }

    #[test]
    fn reads_documented_example() {
        let config = AgentConfig::from_custom(&json!({
            "agent": {
                "model": "anthropic/claude-sonnet-4-5",
                "thinking": "high",
                "approvalMode": "write",
                "subagents": { "maxConcurrent": 99, "isolation": "worktree" },
                "exec": { "allow": ["pnpm test"] },
                "skills": [".lan/skills"],
                "tools": [
                    { "name": "db_query", "command": "lan db query", "approval": "exec" },
                    { "name": "", "command": "lan nope" }
                ]
            }
        }));
        assert_eq!(config.model.as_deref(), Some("anthropic/claude-sonnet-4-5"));
        assert_eq!(config.thinking, ThinkingLevel::High);
        assert_eq!(config.approval_mode, ApprovalMode::Write);
        // 上限被夹住，避免配置写出 99 个并发子 agent。
        assert_eq!(config.subagents.max_concurrent, 8);
        assert_eq!(config.subagents.isolation, Isolation::Worktree);
        assert_eq!(config.exec_allow, vec!["pnpm test".to_string()]);
        assert_eq!(config.custom_tools.len(), 1);
        assert_eq!(config.custom_tools[0].name, "db_query");
    }

    #[test]
    fn overrides_ignore_blank_values() {
        let mut config = AgentConfig::default();
        config.apply_overrides(Some("anthropic"), Some("  "), None, Some("yolo"));
        assert_eq!(config.model, None);
        assert_eq!(config.provider.as_deref(), Some("anthropic"));
        assert_eq!(config.approval_mode, ApprovalMode::Yolo);
    }

    #[test]
    fn parses_guard_rules() {
        let config = AgentConfig::from_custom(&json!({
            "agent": {
                "guard": [
                    { "tool": "exec", "pattern": "rm\\s+-rf", "action": "deny", "reason": "no rm -rf" },
                    { "tool": "*" },
                    { "pattern": "no-tool-field" }
                ]
            }
        }));
        assert_eq!(config.guard.len(), 2, "{:?}", config.guard);
        assert_eq!(config.guard[0].tool, "exec");
        assert_eq!(config.guard[0].pattern.as_deref(), Some("rm\\s+-rf"));
        assert_eq!(config.guard[0].reason.as_deref(), Some("no rm -rf"));
        // action 缺省按 deny。
        assert_eq!(config.guard[1].action, "deny");
    }
}
