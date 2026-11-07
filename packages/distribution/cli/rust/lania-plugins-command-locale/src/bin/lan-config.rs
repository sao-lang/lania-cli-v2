//! `lan-config` 独立二进制（文档 §3.2 / §5.4 D.1 命令双形态）。
//!
//! 零 TS payload、零语言运行时：`config` 不调用 `attach_bridge`，
//! 也不登记任何语言 bridge（§5.1 A.1 依赖矩阵）。包级偏好配置由
//! `lania-package-config` 引擎承担（§10.2）。

use std::env;

use anyhow::Result;
use lania_cli_kit::run_single_command;
use lania_host::{plugin::EmptyPlugin, HostRuntime};
use lania_plugins_command_locale::{factory, spec, COMMAND_NAME};

#[tokio::main]
async fn main() -> Result<()> {
    runex_home::activate_managed_runtimes();

    let forwarded: Vec<String> = env::args().skip(1).collect();
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
