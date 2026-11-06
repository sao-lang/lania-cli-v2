//! NodeBridgeClient 的 transport 分层入口。
//!
//! 这一层只负责把 transport 的不同实现组织起来：
//! - `process_transport`：真实 Node 子进程 + stdin/stdout 协议传输
//! - `mock`：当 process transport 不可用时的兜底实现（保持接口结构稳定）
//! - `call`：面向上层的“发起一次调用/打开流式调用”API
//! - `requests`：所有 bridge method 的 request 构造器（避免到处拼 JSON）
//!
//! 新手建议从 `requests.rs` -> `call.rs` -> `process_transport.rs` 这个顺序阅读。

mod call;
mod mock;
mod process_transport;
mod requests;
