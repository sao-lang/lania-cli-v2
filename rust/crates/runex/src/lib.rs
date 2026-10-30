//! `runex` —— 独立的多运行时版本管理器。
//!
//! 本 crate 同时提供三种形态：
//!
//! - **CLI**：`runex install / use / remove / list / ...`（`src/main.rs`）；
//! - **TUI**：`runex ui`（[`tui`]）；
//! - **Rust API**：[`api::Runex`]，宿主 / GUI / 脚本直接调用。
//!
//! 对 lania 宿主零依赖：`runex` 只依赖 `lania-runtime-manager`（内部引擎）
//! 与第三方 crate，可用 `cargo build --release -p runex` 产出单文件。

pub mod api;
pub mod tui;

pub use api::*;
