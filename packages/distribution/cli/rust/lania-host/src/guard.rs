//! 工具门禁（`onAgentToolCall` 的同步等价物）。
//!
//! 为什么不是直接复用 `lania-hooks` 的 hook bus：
//! - hook bus 的方法是 **async**，而它需要在 `agent.tool.exec` 里同步执行；
//! - `agent.tool.exec` 跑在 bridge 的 reader 线程上，在那里 `block_on` 一个可能调用
//!   子进程的 hook，会把事件流和心跳一起卡住。
//!
//! 所以门禁做成**纯同步的进程内策略**：规则来自 `custom.agent.guard`，
//! 由命令层在进入会话前 [`install`]，宿主在每次工具执行前 [`evaluate`]。
//! 规则不经过 Node，模型无法通过伪造参数绕过。
//!
//! 规则语义（按数组顺序，首个命中即生效）：
//!
//! ```jsonc
//! "guard": [
//!   { "tool": "exec",  "pattern": "rm\\s+-rf", "action": "deny", "reason": "no recursive delete" },
//!   { "tool": "write", "pattern": "\\.env",   "action": "require-approval" },
//!   { "tool": "*",     "action": "allow" }
//! ]
//! ```
//!
//! - `tool` 支持精确名或 `*`；
//! - `pattern` 是对「归一化目标」的正则（文件绝对路径 / 命令行 / `git diff <path>`），省略即匹配任意；
//! - `action`：`deny` / `require-approval` / `allow`。

use std::sync::{Mutex, OnceLock};

use regex::Regex;
use serde_json::Value;

/// 单条规则。
#[derive(Debug, Clone)]
pub struct GuardRule {
    pub tool: String,
    pattern: Option<Regex>,
    pattern_source: Option<String>,
    pub action: GuardAction,
    pub reason: Option<String>,
}

/// 规则动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardAction {
    Deny,
    RequireApproval,
    Allow,
}

/// 一次判定的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    /// 没有规则命中，按原有 tier/审批逻辑走。
    Allow,
    /// 直接拒绝，不执行、不弹审批。
    Deny(String),
    /// 强制走审批（即使当前是 yolo）。
    RequireApproval(String),
}

impl GuardRule {
    /// 解析一条 JSON 规则；格式不合法返回 `None`（配置写错不该让命令起不来）。
    pub fn from_json(value: &Value) -> Option<Self> {
        let tool = value
            .get("tool")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|tool| !tool.is_empty())?
            .to_string();
        let action = match value
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("deny")
            .to_ascii_lowercase()
            .replace('_', "-")
            .as_str()
        {
            "deny" | "block" => GuardAction::Deny,
            "require-approval" | "ask" | "approve" => GuardAction::RequireApproval,
            "allow" | "permit" => GuardAction::Allow,
            _ => return None,
        };
        let pattern_source = value
            .get("pattern")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .map(ToOwned::to_owned);
        let pattern = match pattern_source.as_ref() {
            Some(source) => Some(Regex::new(source).ok()?),
            None => None,
        };
        Some(Self {
            tool,
            pattern,
            pattern_source,
            action,
            reason: value
                .get("reason")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        })
    }

    /// 由已解析字段构造（命令层从 `custom.agent.guard` 转过来时用）。
    pub fn from_parts(
        tool: &str,
        pattern: Option<&str>,
        action: &str,
        reason: Option<&str>,
    ) -> Option<Self> {
        let mut value = serde_json::json!({ "tool": tool, "action": action });
        if let Some(pattern) = pattern {
            value["pattern"] = Value::String(pattern.to_string());
        }
        if let Some(reason) = reason {
            value["reason"] = Value::String(reason.to_string());
        }
        Self::from_json(&value)
    }

    /// 该规则是否命中。
    pub fn matches(&self, tool: &str, target: &str) -> bool {
        if self.tool != "*" && self.tool != tool {
            return false;
        }
        match self.pattern.as_ref() {
            Some(pattern) => pattern.is_match(target),
            None => true,
        }
    }

    /// 用于诊断输出（不泄露正则内部结构）。
    pub fn describe(&self) -> String {
        match self.pattern_source.as_ref() {
            Some(pattern) => format!("{} ~ /{pattern}/", self.tool),
            None => self.tool.clone(),
        }
    }
}

fn slot() -> &'static Mutex<Vec<GuardRule>> {
    static SLOT: OnceLock<Mutex<Vec<GuardRule>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Vec::new()))
}

/// 装上规则；返回的 handle drop 时清空。
pub fn install(rules: Vec<GuardRule>) -> GuardHandle {
    *slot().lock().expect("guard slot poisoned") = rules;
    GuardHandle { active: true }
}

/// 判定一次工具调用。
pub fn evaluate(tool: &str, target: &str) -> GuardDecision {
    let rules = slot();
    let rules = rules.lock().expect("guard slot poisoned");
    for rule in rules.iter() {
        if !rule.matches(tool, target) {
            continue;
        }
        return match rule.action {
            GuardAction::Deny => GuardDecision::Deny(
                rule.reason
                    .clone()
                    .unwrap_or_else(|| format!("guard denied `{tool}` on `{target}`")),
            ),
            GuardAction::RequireApproval => GuardDecision::RequireApproval(
                rule.reason
                    .clone()
                    .unwrap_or_else(|| format!("guard requires approval for `{tool}`")),
            ),
            GuardAction::Allow => GuardDecision::Allow,
        };
    }
    GuardDecision::Allow
}

/// 当前规则条数（诊断用）。
pub fn rule_count() -> usize {
    slot().lock().expect("guard slot poisoned").len()
}

/// RAII 句柄：drop 时清空规则。
#[derive(Debug)]
pub struct GuardHandle {
    active: bool,
}

impl Drop for GuardHandle {
    fn drop(&mut self) {
        if self.active {
            slot().lock().expect("guard slot poisoned").clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn rules(values: &[Value]) -> Vec<GuardRule> {
        values.iter().filter_map(GuardRule::from_json).collect()
    }

    #[test]
    fn deny_rule_blocks_matching_command() {
        let _guard = serial();
        let handle = install(rules(&[json!({
            "tool": "exec",
            "pattern": "rm\\s+-rf",
            "action": "deny",
            "reason": "no recursive delete"
        })]));
        assert_eq!(
            evaluate("exec", "rm -rf /"),
            GuardDecision::Deny("no recursive delete".into())
        );
        // 工具名不符 → 放行。
        assert_eq!(evaluate("read", "rm -rf /"), GuardDecision::Allow);
        // 正则不命中 → 放行。
        assert_eq!(evaluate("exec", "ls -la"), GuardDecision::Allow);
        drop(handle);
        assert_eq!(rule_count(), 0);
    }

    #[test]
    fn wildcard_tool_and_first_match_wins() {
        let _guard = serial();
        let _handle = install(rules(&[
            json!({ "tool": "write", "pattern": "\\.env$", "action": "require-approval" }),
            json!({ "tool": "*", "action": "allow" }),
        ]));
        assert_eq!(
            evaluate("write", "/repo/.env"),
            GuardDecision::RequireApproval("guard requires approval for `write`".into())
        );
        // 命中 `*` 规则后直接 allow，不再往下走（这里是最后一条，语义等价）。
        assert_eq!(evaluate("exec", "ls"), GuardDecision::Allow);
    }

    #[test]
    fn invalid_rules_are_skipped_not_fatal() {
        let _guard = serial();
        let parsed = rules(&[
            json!({ "action": "deny" }),
            json!({ "tool": "exec", "action": "explode" }),
            json!({ "tool": "exec", "pattern": "(unclosed", "action": "deny" }),
            json!({ "tool": "exec", "action": "deny" }),
        ]);
        assert_eq!(parsed.len(), 1, "{parsed:?}");
    }

    #[test]
    fn describe_is_readable() {
        let rule = GuardRule::from_json(&json!({
            "tool": "exec", "pattern": "curl", "action": "deny"
        }))
        .unwrap();
        assert_eq!(rule.describe(), "exec ~ /curl/");
    }
}
