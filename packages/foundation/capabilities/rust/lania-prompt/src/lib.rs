//! 交互式提问抽象，以及答案记录与回放能力。
//!
//! 主要导出：new、kind、choice、default_value、when、goto。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
//! - 包含并发共享状态或消息通道

mod engine;
mod models;
mod parsing;
mod service;
mod ui_dialoguer;
mod ui_terminal;

pub use engine::PromptEngine;
pub use engine::PromptState;
pub use models::*;
pub use service::{PromptFallbackStrategy, PromptRunOptions, PromptService};

pub(crate) const BACK_SIGNAL: &str = "__BACK__";
pub(crate) const EXIT_SIGNAL: &str = "__EXIT__";

#[cfg(test)]
mod tests;
