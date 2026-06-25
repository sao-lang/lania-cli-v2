//! 进度条状态存储与阶段推进接口。
//!
//! 主要导出：percent、duration_ms、rate_per_sec、eta_ms、add_sink、sink_count。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
//! - 包含并发共享状态或消息通道

mod models;
mod render;
mod service;
mod terminal;

pub use models::{
    CallbackProgressSink, ProgressEvent, ProgressEventKind, ProgressKind, ProgressLevel,
    ProgressSink, ProgressSnapshot, ProgressStatus, ProgressSummary, TaskProgressSink,
};
pub use render::{IndicatifProgressRenderer, JsonProgressRenderer, ProgressRenderer};
pub use service::{ProgressService, TerminalProgressSuspendGuard};
pub use terminal::{IndicatifTerminalProgressSink, TerminalProgressMode};

#[cfg(test)]
mod tests;
