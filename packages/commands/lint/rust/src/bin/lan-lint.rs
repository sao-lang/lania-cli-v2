//! `lan-lint` 独立二进制（文档 §3.2 / §5.4 D.1 命令双形态）。
//!
//! 与总包共用 `lania_cli_kit::run_single_command`、`spec()` 和 `factory()`，
//! 差别只在「链接了哪些命令 crate」与「注册了几个命令」。
//! 语言 bridge（py/go/dart）best-effort 登记：装了就更好，不装也能用；
//! Rust bridge 短路（B11）：同一二进制兼作 `lan __lania_rs_bridge`。

use std::env;

use anyhow::Result;
use lania_cli_kit::run_single_command;
use lania_host::{maybe_run_rust_bridge, configure_command_bridges, plugin::EmptyPlugin, HostRuntime};
use lania_plugins_command_lint::{factory, spec, COMMAND_NAME};

#[tokio::main]
async fn main() -> Result<()> {
    // 所有子进程继承 PATH（§5.1 A.2）；只依赖极小的 runex-home，不拉下载引擎。
    runex_home::activate_managed_runtimes();

    // B11 短路与桥接装配统一收口到 lania-host（与其它命令二进制不再各写一遍）。
    if maybe_run_rust_bridge()? {
        return Ok(());
    }

    let forwarded: Vec<String> = env::args().skip(1).collect();
    // 构建期产物：describes/lint.json 由 `spec()` 序列化得到。
    if forwarded.first().map(String::as_str) == Some("--describe") {
        println!("{}", serde_json::to_string_pretty(&spec())?);
        return Ok(());
    }

    let mut host = HostRuntime::new();
    host.register_plugin(Box::new(EmptyPlugin))?;
    host.register_plugin(factory())?;
    host.initialize().await?;

    let cwd = env::current_dir()?;
    let _ = host
        .bootstrap_project_extensions_from_cwd_async(cwd.display().to_string())
        .await?;
    // 统一登记各语言桥 + 装配 Hub provider + 注入 bridge_slot。
    configure_command_bridges(&mut host, &cwd, false).await;

    let code = run_single_command(&mut host, spec(), forwarded).await?;
    host.shutdown_async().await?;
    debug_assert_eq!(spec().name, COMMAND_NAME);
    std::process::exit(code);
}
