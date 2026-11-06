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

use clap::error::ErrorKind;
use clap::FromArgMatches;

fn main() -> std::process::ExitCode {
    // 帮助文本也要跟随 locale：在 clap 解析前先扫 `--locale` 与配置文件。
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let locale = cli::help_locale(&args);
    let matches = match cli::localized_command(locale).try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(error) => match error.kind() {
            // 帮助在 clap 内部已经渲染完，这里先做结构词替换再输出。
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                print!("{}", cli::localize_help_output(&error.to_string(), locale));
                return std::process::ExitCode::SUCCESS;
            }
            _ => error.exit(),
        },
    };
    let cli = match cli::Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    match commands::run(cli) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            // `{:#}` 带上 anyhow 的上下文链，便于用户复制一段完整的报错。
            eprintln!("runex: {error:#}");
            std::process::ExitCode::from(1)
        }
    }
}
