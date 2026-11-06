//! Node bridge 的 mock transport 入口。
//!
//! mock transport 的目标不是完整模拟真实 Node 运行时，而是：
//! - 在缺少 Node 环境或 bridge 进程不可用时，仍能给 Rust 宿主返回结构稳定的 `BridgeExchange`
//! - 让开发、演示、测试中的部分链路继续可跑
//!
//! 由于支持的方法越来越多，这里拆成 `part1/part2` 两个文件维护。

mod part1;
mod part2;

use crate::client::NodeBridgeClient;
use crate::protocol::{BridgeExchange, BridgeRequest};

impl NodeBridgeClient {
    pub(super) fn mock_call(&self, request: BridgeRequest) -> BridgeExchange {
        match part1::handle_part1(self, request) {
            Ok(exchange) => exchange,
            Err(request) => part2::handle_part2(self, request),
        }
    }
}
