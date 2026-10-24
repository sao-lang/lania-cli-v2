//! CLI 输出层：把 `CommandExecution` 转换成最终对用户可见的 JSON / 人类可读输出。
//!
//! 本模块是 `lania-cli` 的“最后一公里”：
//! 1) 组装统一 envelope（命令名/参数、cwd、exitCode、result、error、git）
//! 2) 文案本地化（把可本地化的字符串替换成当前 locale 对应文本）
//! 3) 输出格式渲染（JSON pretty / 人类可读）
//!
//! 结构拆分：
//! - `json.rs`：把 `CommandExecution` 转为统一 envelope
//! - `human.rs`：人类可读渲染（命令摘要 + 按 result.kind 分派）

mod human;
mod json;

pub(crate) use human::render_output_value;
pub(crate) use json::error_json_value;
pub(crate) use json::execution_json_value;

/// 交互型命令（如 `lan agent`）在终端里自行渲染，宿主不再追加任何输出。
pub(crate) fn should_suppress_output(value: &serde_json::Value) -> bool {
    let payload = value.get("result").unwrap_or(value);
    payload.get("kind").and_then(|kind| kind.as_str()) == Some("agent_session")
}
