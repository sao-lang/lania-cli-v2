//! `runex-home`：只负责解析 `~/.runex` 与 PATH 前置（文档 §5.1 A.2 / §3.2）。
//!
//! 刻意零第三方依赖（除 anyhow）与零下载/解压引擎：所有 `lan-<cmd>` 与 `lania-cli`
//! 只依赖本 crate 做 PATH 前置，只有 `env` 命令额外链接 `runex` lib。
//! `RuntimeHome` 的实现整体从 `runex-core/src/home.rs` 迁入，`runex-core` 通过
//! `pub use runex_home::*` 保持 `runex::api` 的冻结导出面不变（§11.3）。

mod home;

pub use home::*;

/// 把 `~/.runex/shims` 前置到本进程 PATH（幂等）。
///
/// 失败必须静默忽略：自管运行时是可选能力，任何解析/权限问题都不应阻断命令本身。
pub fn activate_managed_runtimes() {
    if let Ok(home) = RuntimeHome::resolve() {
        home.activate();
    }
}
