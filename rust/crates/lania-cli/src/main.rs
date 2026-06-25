//! `lan` 二进制入口。
//!
//! 这个文件做三件事：
//! 1) 构建 `HostRuntime` 并注册所有 Rust 插件（命令实现、能力、hook）。
//! 2) 由命令规格（`CommandSpec`）生成 CLI（clap），解析 argv 得到 `CommandContext`。
//! 3) 执行命令并以 JSON 输出结果，同时用 exit code 表示成功/失败。
//!
//! 约定：
//! - 内建命令（`--help/--version/help`）会在初始化后、解析命令前“短路返回”。
//! - 成功时 `stdout` 输出执行结果 JSON（pretty），失败时 `stderr` 先打印友好错误，再在 `stdout` 输出错误 JSON。

use std::{env, sync::Arc};

use anyhow::Result;
use clap::error::ErrorKind as ClapErrorKind;
use lania_host::{plugin::EmptyPlugin, ExecutionError, Host, HostRuntime, EXIT_RUNTIME_ERROR};
use lania_logger::{AutoStyledLogRenderer, CliMessageLevel, LogRenderer, StderrLogSink};
use lania_plugins_command_add::AddCommandPlugin;
use lania_plugins_command_build::BuildCommandPlugin;
use lania_plugins_command_create::CreateCommandPlugin;
use lania_plugins_command_dev::DevCommandPlugin;
use lania_plugins_command_generate::GenerateCommandPlugin;
use lania_plugins_command_lint::LintCommandPlugin;
use lania_plugins_command_locale::ConfigCommandPlugin;
use lania_plugins_command_release::ReleaseCommandPlugin;
use lania_plugins_command_sync::SyncCommandPlugin;
use lania_plugins_command_template::TemplateCommandPlugin;
use lania_plugins_command_tools::ToolsCommandPlugin;
use lania_progress::TerminalProgressMode;

mod output;
mod profile;

use output::{execution_json_value, render_output_value};
use profile::{
    cli_message_renderer, cli_parse_error_message, cli_text, cli_version, localized_user_message,
    normalize_os_args, output_profile_from_sources, resolve_effective_locale, EventMode,
    OutputMode, OutputProfile, ProgressMode,
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut host = HostRuntime::new();
    // 插件注册顺序会影响：
    // - CLI 命令树的展示顺序
    // - setup 阶段的 hooks/capabilities/handlers 的注册时机
    host.register_plugin(Box::new(EmptyPlugin))?;
    host.register_plugin(Box::new(DevCommandPlugin))?;
    host.register_plugin(Box::new(BuildCommandPlugin))?;
    host.register_plugin(Box::new(LintCommandPlugin))?;
    host.register_plugin(Box::new(CreateCommandPlugin))?;
    host.register_plugin(Box::new(AddCommandPlugin))?;
    host.register_plugin(Box::new(GenerateCommandPlugin))?;
    host.register_plugin(Box::new(ReleaseCommandPlugin))?;
    host.register_plugin(Box::new(SyncCommandPlugin))?;
    host.register_plugin(Box::new(TemplateCommandPlugin))?;
    host.register_plugin(Box::new(ToolsCommandPlugin))?;
    host.register_plugin(Box::new(ConfigCommandPlugin))?;
    host.initialize().await?;
    let cwd = env::current_dir()?.display().to_string();
    let project_config = host
        .load_lan_config_snapshot_from_cwd_async(cwd.clone())
        .await
        .ok();
    let preferences = lania_preferences::load_preferences();
    let locale = resolve_effective_locale(project_config.as_ref(), &preferences);
    let mut output_profile = output_profile_from_sources(project_config.as_ref(), &preferences);
    output_profile.locale = locale.clone();
    host.set_locale(&locale);
    let _ = host
        .bootstrap_project_extensions_from_cwd_async(cwd.clone())
        .await?;

    let renderer: Arc<dyn LogRenderer> = Arc::new(
        AutoStyledLogRenderer::default()
            .with_locale(locale.clone())
            .with_timestamps(
                preferences.log_timestamps && matches!(output_profile.mode, OutputMode::Human),
            ),
    );
    host.logger()
        .add_sink(Arc::new(StderrLogSink::new(renderer)));
    match output_profile.progress {
        ProgressMode::None => {}
        ProgressMode::Spinner => {
            host.progress()
                .attach_terminal_sink(TerminalProgressMode::Spinner);
        }
        ProgressMode::Bar => {
            host.progress()
                .attach_terminal_sink(TerminalProgressMode::Bar);
        }
    }

    let mut commands = host.command_specs().to_vec();
    lania_command::apply_legacy_aliases(&mut commands);
    let binary_name = "lan";
    let about = cli_text(locale.as_str(), "Lania CLI v2", "Lania CLI v2 命令行工具");
    lania_command::localize_command_specs(locale.as_str(), &mut commands);

    let raw_args = normalize_os_args(env::args_os());
    let builtin_args = if raw_args.len() <= 1 {
        vec![binary_name.to_string(), "help".to_string()]
    } else {
        raw_args.clone()
    };
    if let Some(output) = lania_command::render_builtin_command(
        binary_name,
        about,
        cli_version(),
        &commands,
        &builtin_args,
        EXIT_RUNTIME_ERROR,
        locale.as_str(),
    ) {
        // 内建命令不需要解析 clap matches，也不会进入 handler 执行流程。
        print!("{}", output.output);
        host.shutdown_async().await?;
        if output.exit_code != 0 {
            std::process::exit(output.exit_code);
        }
        return Ok(());
    }
    let cli = lania_command::build_cli(
        binary_name,
        about,
        cli_version(),
        &commands,
        locale.as_str(),
    );
    let matches = match cli.try_get_matches_from(env::args_os()) {
        Ok(matches) => matches,
        Err(error) => {
            if matches!(
                error.kind(),
                ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion
            ) {
                print!("{}", error);
                host.shutdown_async().await?;
                return Ok(());
            }
            let exit_code = EXIT_RUNTIME_ERROR;
            let message = cli_parse_error_message(binary_name, &raw_args, &error, locale.as_str());
            eprintln!(
                "{}",
                cli_message_renderer(locale.as_str(), &preferences, &output_profile)
                    .render(CliMessageLevel::Error, message.clone())
            );
            print!(
                "{}",
                render_output_value(
                    serde_json::json!({
                        "kind": "error",
                        "message": message,
                        "exitCode": exit_code,
                    }),
                    &output_profile,
                )?
            );
            host.shutdown_async().await?;
            std::process::exit(exit_code);
        }
    };

    let result = if let Some(context) =
        lania_command::command_context_from_matches(&commands, &matches, cwd, "trace-1")
    {
        // 解析到了具体命令：交给 HostRuntime 执行（可能走 workflow 或 node-bridge）。
        host.execute_command(&context).await.map(|execution| {
            (
                render_output_value(
                    execution_json_value(&execution, &output_profile),
                    &output_profile,
                ),
                execution.exit_code(),
            )
        })
    } else {
        Ok((Ok(String::new()), 0))
    };

    host.shutdown_async().await?;

    match result {
        Ok((output, exit_code)) => {
            print!("{}", output?);
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Err(error) => {
            let exit_code = error
                .downcast_ref::<ExecutionError>()
                .map(|inner| inner.exit_code)
                .unwrap_or(EXIT_RUNTIME_ERROR);
            let message = localized_user_message(locale.as_str(), &error.to_string());

            // 用统一 renderer 打印友好错误，同时保持 stdout 输出机器可读 JSON。
            eprintln!(
                "{}",
                cli_message_renderer(locale.as_str(), &preferences, &output_profile)
                    .render(CliMessageLevel::Error, message.clone())
            );

            print!(
                "{}",
                render_output_value(
                    serde_json::json!({
                        "kind": "error",
                        "message": message,
                        "exitCode": exit_code,
                    }),
                    &output_profile,
                )?
            );
            std::process::exit(exit_code);
        }
    }

    Ok(())
}
