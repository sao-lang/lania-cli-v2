//! `lan-runtime` 独立二进制（文档 §5.6 F.1/F.2）。
//!
//! runtime 设施是**可扩展宿主设施**，不是一条 named 命令：
//! 它承载项目 `lan.config.*` 里声明的 `runtimeCommands`（动态命令）与
//! 脚手架封装出来的命令。core launcher 的 Phase 2 动态回退会调用它。
//!
//! 三种动作：
//! - `--list`：列出当前项目的动态命令（JSON）；
//! - `--resolve <cmd>`：判定命令是否为项目动态命令（JSON）；
//! - `<cmd> [args...]`：执行一条动态命令（与总包同一条执行路径）。

use std::env;

use anyhow::{anyhow, Result};
use lania_cli_kit::run_single_command;
use lania_command_runtime::{parse_runtime_args, resolve_payload, RuntimeAction};
use lania_host::{plugin::EmptyPlugin, Host, HostRuntime};

#[tokio::main]
async fn main() -> Result<()> {
    // 所有子进程继承 PATH（§5.1 A.2）。
    runex_home::activate_managed_runtimes();

    let args: Vec<String> = env::args().skip(1).collect();
    let action = parse_runtime_args(&args);

    let mut host = HostRuntime::new();
    // runtime 设施不注册任何 builtin 命令：它只提供项目级扩展的执行环境。
    host.register_plugin(Box::new(EmptyPlugin))?;
    host.initialize().await?;

    let cwd = env::current_dir()?.display().to_string();
    // 项目级动态命令由 node-bridge 解析（commands.resolveDynamic）；
    // 解析失败不阻断设施本身（`--list` 仍可返回空列表）。
    let _ = host.bootstrap_project_extensions_from_cwd_async(cwd.clone()).await;
    let specs = host.command_specs().to_vec();

    match action {
        RuntimeAction::List => {
            let commands: Vec<serde_json::Value> = specs
                .iter()
                .map(|spec| {
                    serde_json::json!({
                        "command": spec.name,
                        "about": spec.about,
                        "handlerId": spec.handler_id,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({ "commands": commands }))?
            );
            host.shutdown_async().await?;
        }
        RuntimeAction::Resolve(command) => {
            let matched = specs.iter().find(|spec| spec.name == command);
            let payload = resolve_payload(
                &command,
                matched.is_some(),
                matched.map(|spec| spec.handler_id.as_str()),
            );
            println!("{}", serde_json::to_string(&payload)?);
            host.shutdown_async().await?;
        }
        RuntimeAction::Execute(argv) => {
            let command = argv.first().cloned().unwrap_or_default();
            let spec = specs
                .iter()
                .find(|spec| spec.name == command)
                .cloned()
                .ok_or_else(|| anyhow!("unknown dynamic command: {command}"))?;
            // 与命令子包 bin 走同一条执行路径（cli-kit 的 run_single_command），
            // 因此输出/退出码与总包跑动态命令逐字节一致。
            let code = run_single_command(&mut host, spec, argv[1..].to_vec()).await?;
            host.shutdown_async().await?;
            std::process::exit(code);
        }
    }

    Ok(())
}
