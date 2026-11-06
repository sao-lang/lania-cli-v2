//! `RuntimeHome` 的实现已迁入 `runex-home`（文档 §5.1 A.2）。
//!
//! 这里保留同名模块作为兼容层，确保 `crate::home::{RuntimeHome, HOME_ENV, META_FILE,
//! PATH_ENV}` 与 `runex::api` 的冻结导出面逐字不变，内部引用也无需改动。

pub use runex_home::*;
