//! 任务列表状态机与快照接口。
//!
//! 主要导出：new、group、priority、timeout、retries、rollback。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含并发共享状态或消息通道

mod executor;
mod helpers;
mod service;
mod types;

pub use executor::TaskExecutor;
pub use service::TaskService;
pub use types::{
    TaskDefinition, TaskEvent, TaskEventKind, TaskFailure, TaskOutcome, TaskPriority, TaskRecord,
    TaskRunMode, TaskRunOptions, TaskRunReport, TaskSink, TaskState,
};

#[cfg(test)]
mod tests;
