//! Bridge Hub 共享核心：零 lania 依赖，不认识任何语言或运行时。
//!
//! 只提供 `Channel` 传输抽象、`PendingTable`、端点生命周期记录与资产落盘；
//! 路由/广播逻辑在 `lania-bridge-bus`，进程装配在 `lania-bridge-host`。

pub mod assets;
pub mod channel;
pub mod lifecycle;
pub mod pending;

pub use assets::AssetStore;
pub use channel::{Channel, ChannelError};
pub use lifecycle::{EndpointRecord, EndpointStatus};
pub use pending::PendingTable;

// 帧类型与错误码由 protocol 生成物提供，core 只做转发，避免重复定义。
pub use lania_bridge_protocol::{
    bus_error, decode_frame, encode_frame, BusErrorDetail, BusFrame, Value, FRAME_KINDS,
    E_BAD_FRAME, E_ENDPOINT_GONE, E_INTERNAL, E_INVALID_PARAMS, E_METHOD_NOT_FOUND, E_TIMEOUT,
};
