//! token 估算与压缩计划。
//!
//! 估算是**粗估**：目的只是决定「要不要压缩」，不是计费。
//! CJK 与 ASCII 分开计权，因为同一段字符数下二者 token 数差 3～4 倍，
//! 统一用 `chars/4` 会让中文长会话严重低估、压缩触发得太晚。
//!
//! 压缩策略（§5.4 的 v1 落地）：
//! - 机械裁剪交给渲染层（长工具输出截断）；
//! - 这里只做「保留最近 N% + 生成抽取式摘要」；
//! - 摘要**不额外花一次模型调用**：v1 用抽取式（谁问过什么、动过哪些文件），
//!   确定性、可测、零成本；需要语义摘要时再引入独立 compaction 模型调用。

use serde_json::Value;

const CHARS_PER_TOKEN: f32 = 4.0;
const CJK_TOKEN_COST: f32 = 1.0;
const MESSAGE_OVERHEAD: u64 = 4;
/// 摘要上限（字符），约等于 1500 token。
const SUMMARY_CHAR_BUDGET: usize = 4_500;

/// 估算一段文本的 token 数。
pub fn estimate_text_tokens(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    let mut cjk = 0u64;
    let mut other = 0u64;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    let estimate = cjk as f32 * CJK_TOKEN_COST + other as f32 / CHARS_PER_TOKEN;
    estimate.ceil() as u64
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3000..=0x303F   // CJK 标点
        | 0x3040..=0x30FF // 假名
        | 0x3400..=0x4DBF // 扩展 A
        | 0x4E00..=0x9FFF // 基本区
        | 0xAC00..=0xD7AF // 谚文
        | 0xF900..=0xFAFF // 兼容
        | 0xFF00..=0xFF60 // 全角
        | 0x20000..=0x2FA1F
    )
}

/// 估算一条 pi 消息的 token 数（含结构开销）。
pub fn estimate_message_tokens(message: &Value) -> u64 {
    let mut total = MESSAGE_OVERHEAD;
    match message.get("content") {
        Some(Value::String(text)) => total += estimate_text_tokens(text),
        Some(Value::Array(blocks)) => {
            for block in blocks {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    total += estimate_text_tokens(text);
                }
                // 工具调用的入参同样进上下文。
                if let Some(arguments) = block.get("arguments") {
                    total += estimate_text_tokens(&arguments.to_string());
                }
            }
        }
        _ => {}
    }
    // toolResult 自带 toolCallId/toolName 等字段，额外开销按 8 计。
    if message.get("role").and_then(Value::as_str) == Some("toolResult") {
        total += 8;
    }
    total
}

pub fn estimate_messages_tokens(messages: &[Value]) -> u64 {
    messages.iter().map(estimate_message_tokens).sum()
}

/// 是否应当压缩。
pub fn should_compact(used: u64, context_window: u64, threshold: f32) -> bool {
    if context_window == 0 {
        return false;
    }
    let threshold = threshold.clamp(0.1, 1.0) as f64;
    used as f64 > context_window as f64 * threshold
}

/// 一次压缩的计划。
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionPlan {
    /// 抽取式摘要（进上下文）。
    pub summary: String,
    /// 保留的消息（尾部窗口）。
    pub kept: Vec<Value>,
    /// 被丢弃的消息条数。
    pub dropped: usize,
    pub tokens_before: u64,
    pub tokens_after: u64,
}

/// 规划压缩；`keep_ratio` 是「保留尾部比例」，建议 0.4（§9 风险表：保留最近 40% 原文）。
///
/// 返回 `None` 表示消息太少、压缩无意义（避免把 3 条消息压成 1 条摘要还丢信息）。
pub fn plan_compaction(messages: &[Value], keep_ratio: f32) -> Option<CompactionPlan> {
    if messages.len() < 6 {
        return None;
    }
    let total = estimate_messages_tokens(messages);
    let target = (total as f32 * keep_ratio.clamp(0.2, 0.8)) as u64;

    // 从尾部往前累加，直到达到 target。
    let mut kept_from = messages.len();
    let mut accumulated = 0u64;
    while kept_from > 0 {
        let candidate = &messages[kept_from - 1];
        let cost = estimate_message_tokens(candidate);
        if accumulated + cost > target && kept_from < messages.len() {
            break;
        }
        accumulated += cost;
        kept_from -= 1;
    }

    // 不能把 toolResult 作为窗口首个消息：它引用的 toolCall 已经被丢掉。
    while kept_from < messages.len()
        && messages[kept_from].get("role").and_then(Value::as_str) == Some("toolResult")
    {
        kept_from += 1;
    }
    if kept_from == 0 || kept_from >= messages.len() {
        return None;
    }

    let dropped = &messages[..kept_from];
    let kept = messages[kept_from..].to_vec();
    let summary = build_extractive_summary(dropped);
    let tokens_after = estimate_messages_tokens(&kept);

    Some(CompactionPlan {
        summary,
        kept,
        dropped: dropped.len(),
        tokens_before: total,
        tokens_after,
    })
}

/// 抽取式摘要：用户问题 + 触达的文件 + 用过的工具。
pub fn build_extractive_summary(dropped: &[Value]) -> String {
    let mut user_asks: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut tools: Vec<String> = Vec::new();

    for message in dropped {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        match role {
            "user" => {
                if let Some(text) = first_text(message) {
                    let line = collapse(&text, 160);
                    if !line.is_empty() {
                        user_asks.push(line);
                    }
                }
            }
            "toolResult" => {
                if let Some(tool) = message.get("toolName").and_then(Value::as_str) {
                    if !tools.iter().any(|existing| existing == tool) {
                        tools.push(tool.to_string());
                    }
                }
            }
            "assistant" => {
                if let Some(content) = message.get("content").and_then(Value::as_array) {
                    for block in content {
                        if block.get("type").and_then(Value::as_str) != Some("toolCall") {
                            continue;
                        }
                        let name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if !tools.iter().any(|existing| existing == name) {
                            tools.push(name.to_string());
                        }
                        if let Some(path) = block
                            .get("arguments")
                            .and_then(|arguments| arguments.get("path"))
                            .and_then(Value::as_str)
                        {
                            if !files.iter().any(|existing| existing == path) {
                                files.push(path.to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let mut summary = String::from("Earlier conversation, compacted automatically.\n");
    if !user_asks.is_empty() {
        summary.push_str("\nUser requests:\n");
        for (index, ask) in user_asks.iter().take(12).enumerate() {
            summary.push_str(&format!("{}. {ask}\n", index + 1));
        }
    }
    if !files.is_empty() {
        summary.push_str("\nFiles touched:\n");
        for path in files.iter().take(20) {
            summary.push_str(&format!("- {path}\n"));
        }
    }
    if !tools.is_empty() {
        summary.push_str(&format!("\nTools used: {}\n", tools.join(", ")));
    }

    truncate_chars(&summary, SUMMARY_CHAR_BUDGET)
}

fn first_text(message: &Value) -> Option<String> {
    match message.get("content") {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Array(blocks)) => blocks.iter().find_map(|block| {
            (block.get("type").and_then(Value::as_str) == Some("text")).then(|| {
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            })
        }),
        _ => None,
    }
}

fn collapse(text: &str, limit: usize) -> String {
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&single_line, limit)
}

/// 按字符（而非字节）截断，避免切坏 UTF-8。
pub fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cjk_costs_more_than_ascii() {
        let ascii = estimate_text_tokens("abcdefgh"); // 8 chars → 2
        let cjk = estimate_text_tokens("中文字符八个"); // 6 CJK → 6
        assert_eq!(ascii, 2);
        assert_eq!(cjk, 6);
        assert!(cjk > ascii);
    }

    #[test]
    fn messages_tokens_include_tool_arguments() {
        let message = json!({
            "role": "assistant",
            "content": [{ "type": "toolCall", "name": "edit", "arguments": { "path": "a.ts" } }]
        });
        assert!(estimate_message_tokens(&message) > MESSAGE_OVERHEAD);
    }

    #[test]
    fn compaction_keeps_tail_and_never_starts_with_tool_result() {
        let mut messages = Vec::new();
        for index in 0..10 {
            messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": format!("q{index}") }] }));
            messages.push(json!({
                "role": "assistant",
                "content": [{ "type": "toolCall", "name": "read", "arguments": { "path": "a.ts" } }]
            }));
            messages.push(json!({
                "role": "toolResult",
                "toolName": "read",
                "toolCallId": "c",
                "isError": false,
                "content": [{ "type": "text", "text": "x".repeat(200) }]
            }));
        }
        let plan = plan_compaction(&messages, 0.4).expect("plan");
        assert!(plan.dropped > 0);
        assert_ne!(
            plan.kept[0].get("role").and_then(Value::as_str),
            Some("toolResult")
        );
        assert!(plan.summary.contains("User requests"));
        assert!(plan.summary.contains("a.ts"));
        assert!(plan.tokens_after < plan.tokens_before);
    }

    #[test]
    fn short_conversations_are_not_compacted() {
        let messages = vec![
            json!({ "role": "user", "content": "hi" }),
            json!({ "role": "assistant", "content": "hello" }),
        ];
        assert!(plan_compaction(&messages, 0.4).is_none());
    }

    #[test]
    fn threshold_math() {
        assert!(should_compact(80_000, 100_000, 0.7));
        assert!(!should_compact(60_000, 100_000, 0.7));
        assert!(!should_compact(60_000, 0, 0.7));
    }
}
