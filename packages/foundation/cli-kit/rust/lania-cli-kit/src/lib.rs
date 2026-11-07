//! `lania-cli-kit`：CLI 输出渲染与单命令执行支点
//! （`package-split-command-atomization.zh-CN.md` §5.1 A.4 / §5.4 D.1）。
//!
//! `output/` 与 `profile.rs` 从 `lania-cli/src/` 整体迁入，代码零改动（仅把
//! `pub(crate)` 提升为 `pub`），因此总包与 `lan-<cmd>` 独立二进制共用同一套
//! envelope / 本地化 / 渲染函数，是「输出逐字节不变」的支点。
//!
//! 过渡说明：`output/json.rs` 仍引用 `lania_host::CommandExecution`（原状）。
//! 阶段 2a 把该类型下沉到 `lania-command` 之后，本 crate 即可去掉对 host 的依赖。

pub mod output;
pub mod profile;
pub mod single;
pub mod template_catalog;

pub use output::{
    error_json_value, execution_json_value, render_output_value, should_suppress_output,
};
pub use profile::*;
pub use single::run_single_command;
pub use template_catalog::{
    template_highlights, template_summary, BUILTIN_TEMPLATES,
};
