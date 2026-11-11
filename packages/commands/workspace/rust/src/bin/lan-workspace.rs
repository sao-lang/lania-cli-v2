//! `lan-workspace` 独立二进制（文档 §3.2 / §5.4 D.1 命令双形态）。
//!
//! 与总包共用 `lania_cli_kit::run_single_command`、`spec()` 和 `factory()`，
//! 差别只在「链接了哪些命令 crate」与「注册了几个命令」。
//! 语言 bridge（py/go/dart）best-effort 登记：装了就更好，不装也能用；
//! Rust bridge 短路（B11）：同一二进制兼作 `lan __lania_rs_bridge`。

use std::env;

use anyhow::Result;
use lania_cli_kit::run_subcommand_entry;
use lania_host::{maybe_run_rust_bridge, configure_command_bridges, plugin::EmptyPlugin, HostRuntime};
use lania_plugins_command_workspace::{factory, spec, subcommand_spec, COMMAND_NAME};

#[tokio::main]
async fn main() -> Result<()> {
    // 所有子进程继承 PATH（§5.1 A.2）；只依赖极小的 runex-home，不拉下载引擎。
    runex_home::activate_managed_runtimes();

    // B11 短路与桥接装配统一收口到 lania-host（与其它命令二进制不再各写一遍）。
    if maybe_run_rust_bridge()? {
        return Ok(());
    }

    let forwarded: Vec<String> = env::args().skip(1).collect();
    // 构建期产物：describes/workspace.json 由 `spec()` 序列化得到。
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
    // include_rust_bridge=true（B11）：lan-workspace 自身兼作 `lan-rs-bridge`
    // （maybe_run_rust_bridge），rust 项目的 lint/build 才能走 clippy/cargo bridge。
    configure_command_bridges(&mut host, &cwd, true).await;

    // 独立二进制的根 = 转发参数的首个 token（§5.3 C.4：launcher 剥掉 "workspace" 后转发）。
    // `lan workspace dev` 在本进程里是 `lan dev`：JSON 的 command.name 应为 "dev"，
    // 与 e2e 契约一致；未匹配到子命令时回落完整 spec（name="workspace"）。
    let (command_spec, command_argv): (lania_command::CommandSpec, Vec<String>) =
        match forwarded
            .first()
            .and_then(|token| subcommand_spec(token))
            .map(|sub| (sub, forwarded[1..].to_vec()))
        {
            Some((sub_spec, argv)) => (sub_spec, argv),
            None => (spec(), forwarded.clone()),
        };

    let code = lania_cli_kit::run_subcommand_entry(&mut host, command_spec, command_argv).await?;
    host.shutdown_async().await?;
    debug_assert_eq!(spec().name, COMMAND_NAME);
    std::process::exit(code);
}
