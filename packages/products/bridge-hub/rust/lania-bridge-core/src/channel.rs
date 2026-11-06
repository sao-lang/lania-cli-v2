use lania_bridge_protocol::BusFrame;

/// 传输错误。Channel 实现不携带任何语言/运行时信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelError {
    /// 对端已断开（stdio EOF / mpsc disconnected）。
    Closed,
    /// 写失败。
    Io(String),
    /// 收到无法解码的帧。
    Decode(String),
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "channel closed"),
            Self::Io(message) => write!(f, "channel io error: {message}"),
            Self::Decode(message) => write!(f, "channel decode error: {message}"),
        }
    }
}

impl std::error::Error for ChannelError {}

/// 端点与 Hub 之间的双向传输。
///
/// 契约刻意保持极小：`send` 发帧，`try_recv` 非阻塞取帧，`is_closed` 报告断连。
/// 实现可以是进程内 mpsc、stdio JSON 行、socket 或 WASM；帧里没有语言字段，
/// 因此实现之间可以自由互换。Hub 用同步 pump 驱动，不绑定任何 async runtime。
pub trait Channel: Send + Sync {
    fn send(&self, frame: BusFrame) -> Result<(), ChannelError>;

    /// `None` 表示当前无帧可用；`Some(Err(..))` 表示解码/读取错误，由 Hub 计入诊断。
    fn try_recv(&self) -> Option<Result<BusFrame, ChannelError>>;

    fn is_closed(&self) -> bool {
        false
    }
}
