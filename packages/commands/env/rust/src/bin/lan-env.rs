//! `lan-env` 独立二进制（文档 §3.2 / §5.4 D.1 命令双形态）。
//!
//! 与总包共用 `lania_cli_kit::run_single_command`、`spec()` 和 `factory()`，
//! 差别只在「链接了哪些命令 crate」与「注册了几个命令」。
//! 本命令零 TS payload、零语言运行时：`env` / `config` 不调用 `attach_bridge`，
//! 也不登记任何语言 bridge（§5.1 A.1 依赖矩阵）。

use std::env;

use anyhow::Result;
use lania_cli_kit::run_single_command;
use lania_host::{plugin::EmptyPlugin, HostRuntime};
use lania_plugins_command_env::{factory, spec, COMMAND_NAME};

#[tokio::main]
async fn main() -> Result<()> {
    // 所有子进程继承 PATH（§5.1 A.2）；只依赖极小的 runex-home，不拉下载引擎。
    runex_home::activate_managed_runtimes();

    let forwarded: Vec<String> = env::args().skip(1).collect();
    // 构建期产物：describes/env.json 由 `spec()` 序列化得到。
    if forwarded.first().map(String::as_str) == Some("--describe") {
        println!("{}", serde_json::to_string_pretty(&spec())?);
        return Ok(());
    }

    let mut host = HostRuntime::new();
    host.register_plugin(Box::new(EmptyPlugin))?;
    host.register_plugin(factory())?;
    host.initialize().await?;

    let code = run_single_command(&mut host, spec(), forwarded).await?;
    host.shutdown_async().await?;
    debug_assert_eq!(spec().name, COMMAND_NAME);
    std::process::exit(code);
}
