//! `runex` —— 独立的多运行时版本管理器（nvm / fnm 那一类工具）。
//!
//! 设计约束（与 `self-managed-runtime-manager.zh-CN.md` §11/§12/§13 对应）：
//! - **不依赖 lania 宿主**：只依赖 `runex-core`（内部引擎）与第三方 crate，
//!   可单独编译成一个可执行文件；
//! - 能力层公开为 `runex::api::Runex` 与 `runex::tui`，本文件只是 CLI 前端；
//! - 语义与 `lan env` 共用同一套 Runex API / Store / Provider，不另写一套下载安装逻辑；
//! - 默认「直接执行」（nvm 习惯），`--dry-run` 才只打印计划；
//! - `--json` 给脚本与 GUI 用，stdout 只输出机器可读内容，进度写 stderr。

mod cli;
mod commands;

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = cli::Cli::parse();
    match commands::run(cli) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            // `{:#}` 带上 anyhow 的上下文链，便于用户复制一段完整的报错。
            eprintln!("runex: {error:#}");
            std::process::ExitCode::from(1)
        }
    }
}
