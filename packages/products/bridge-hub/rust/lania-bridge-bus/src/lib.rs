//! Bridge Hub 本体。
//!
//! Hub 是唯一汇合点：维护端点表、能力表、主题表与 pending 表，用同步 pump 驱动，
//! 不绑定任何 async runtime，也不认识 `language` / `runtime` / `RuntimeKind`。
//! 端点全双工且彼此平等：任何端点都能同时提供能力、发起请求、发布事件、订阅主题。

pub mod channel;
pub mod endpoint;
pub mod hub;
pub mod policy;

/// 生成的端点骨架（correlation id / 帧构造 / 错误包装），由 gen.mjs 产出。
pub mod runtime {
    include!("../../../protocol/gen/rust/runtime.rs");
}

/// 内置 Rust 端点 adaptor：进程内 channel，手写目录只放业务回调。
pub mod rust_adaptor {
    include!("../../../adaptors/rust/endpoint.rs");
}

pub use channel::{InProcessChannel, StdioChannel};
pub use endpoint::{Endpoint, Handler};
pub use hub::{BridgeHub, HubDiagnostics};
pub use policy::ProviderPolicy;
pub use runtime::EndpointRuntime;

// 常用帧类型转发，消费方无需直接依赖 protocol crate。
pub use lania_bridge_protocol::{BusErrorDetail, BusFrame, Value};
