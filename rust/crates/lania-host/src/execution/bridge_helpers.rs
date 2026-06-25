//! `CommandExecutionContext` 上与 bridge 调用相关的控制流辅助方法。
//!
//! 这个入口文件只负责组织实现：
//! - `long_running` 处理长任务等待与温和关闭
//! - `events` 处理 bridge events 到日志/进度/UI 的映射
//! - `requests` 处理请求分发、重试、超时与错误提升
//! - `state` 负责执行结束时的 `host_state` 打包

mod events;
mod long_running;
mod requests;
mod state;
