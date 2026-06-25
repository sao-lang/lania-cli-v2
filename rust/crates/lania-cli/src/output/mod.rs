//! CLI 输出层：把 `CommandExecution` 转换成最终对用户可见的 JSON/JSONL/人类可读 输出。
//!
//! 本模块是 `lania-cli` 的“最后一公里”：把 host/runtime 返回的结构化结果做
//! 1) 字段裁剪（根据 output profile）
//! 2) 事件去重（events=stream 时避免重复）
//! 3) 文案本地化（把可本地化的字符串替换成当前 locale 对应文本）
//! 4) 输出格式渲染（JSON / JSONL / 人类可读）
//!
//! 结构拆分：
//! - `json.rs`：把 `CommandExecution` 转为用于输出的 `serde_json::Value`，并做裁剪/本地化
//! - `human.rs`：人类可读渲染（按 kind 做分派）

mod human;
mod json;

pub(crate) use human::render_output_value;
pub(crate) use json::execution_json_value;
