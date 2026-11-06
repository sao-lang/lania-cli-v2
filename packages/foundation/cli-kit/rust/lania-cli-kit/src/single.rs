//! `run_single_command`：命令双形态的共同执行支点（§5.4 D.1）。
//!
//! 总包 `lan <cmd> ...` 与独立 `lan-<cmd> ...` 最终都汇入这里：
//! 补回命令名 → `apply_legacy_aliases` → `localize_command_specs` →
//! `render_builtin_command` / `build_cli` → `command_context_from_matches` →
//! `HostRuntime::execute_command` → `execution_json_value` / `render_output_value` → 退出码。
//!
//! argv 契约：调度器剥掉命令名后转发，`lan env list --json` → `spawn lan-env ["list","--json"]`；
//! 补回动作在本函数内部完成，因此 envelope 里的 `command` 名与总包一致。

use std::env;

use anyhow::Result;
use clap::error::ErrorKind as ClapErrorKind;
use lania_command::{
    apply_legacy_aliases, build_cli, command_context_from_matches, localize_command_specs,
    render_builtin_command, CommandSpec,
};
use lania_host::{ExecutionError, HostRuntime, EXIT_RUNTIME_ERROR};
use lania_logger::CliMessageLevel;
use lania_preferences::load_preferences;

use crate::{
    cli_message_renderer, cli_parse_error_message, cli_text, cli_version, error_json_value,
    execution_json_value, localized_user_message, output_profile_from_sources,
    render_output_value, resolve_effective_locale, should_suppress_output,
};

pub async fn run_single_command(
    host: &mut HostRuntime,
    spec: CommandSpec,
    forwarded: Vec<String>,
) -> Result<i32> {
    let cwd = env::current_dir()?.display().to_string();
    let project_config = host
        .load_lan_config_snapshot_from_cwd_async(cwd.clone())
        .await
        .ok();
    let preferences = load_preferences();
    let locale = resolve_effective_locale(project_config.as_ref(), &preferences);
    let mut output_profile = output_profile_from_sources(project_config.as_ref(), &preferences);
    output_profile.locale = locale.clone();
    host.set_locale(&locale);

    let mut own = vec![spec.clone()];
    apply_legacy_aliases(&mut own);
    localize_command_specs(&locale, &mut own);

    let binary_name = "lan";
    let about = cli_text(&locale, "Lania CLI v2", "Lania CLI v2 命令行工具");
    let mut argv = vec![binary_name.to_string(), spec.name.clone()];
    argv.extend(forwarded.iter().cloned());
    if argv.len() <= 2 {
        argv.push("help".to_string());
    }

    if let Some(output) = render_builtin_command(
        binary_name,
        about,
        cli_version(),
        &own,
        &argv,
        EXIT_RUNTIME_ERROR,
        &locale,
    ) {
        print!("{}", output.output);
        return Ok(output.exit_code);
    }

    let cli = build_cli(binary_name, about, cli_version(), &own, &locale);
    let matches = match cli.try_get_matches_from(argv.clone()) {
        Ok(matches) => matches,
        Err(error) => {
            if matches!(
                error.kind(),
                ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion
            ) {
                print!("{error}");
                return Ok(0);
            }
            let message = cli_parse_error_message(binary_name, &argv, &error, &locale);
            eprintln!(
                "{}",
                cli_message_renderer(&locale, &preferences, &output_profile)
                    .render(CliMessageLevel::Error, message.clone())
            );
            print!(
                "{}",
                render_output_value(
                    error_json_value(None, &cwd, EXIT_RUNTIME_ERROR, &message),
                    &output_profile,
                )?
            );
            return Ok(EXIT_RUNTIME_ERROR);
        }
    };

    let context = command_context_from_matches(&own, &matches, cwd.clone(), "trace-1");
    let execution = match &context {
        Some(context) => Some(host.execute_command(context).await),
        None => None,
    };

    match execution {
        Some(Ok(execution)) => {
            let exit_code = execution.exit_code();
            let value = execution_json_value(&execution, &output_profile, &cwd, None);
            let output = if should_suppress_output(&value) {
                String::new()
            } else {
                render_output_value(value, &output_profile)?
            };
            print!("{output}");
            Ok(exit_code)
        }
        Some(Err(error)) => {
            let exit_code = error
                .downcast_ref::<ExecutionError>()
                .map(|inner| inner.exit_code)
                .unwrap_or(EXIT_RUNTIME_ERROR);
            let message = localized_user_message(&locale, &error.to_string());
            eprintln!(
                "{}",
                cli_message_renderer(&locale, &preferences, &output_profile)
                    .render(CliMessageLevel::Error, message.clone())
            );
            print!(
                "{}",
                render_output_value(
                    error_json_value(context.as_ref(), &cwd, exit_code, &message),
                    &output_profile,
                )?
            );
            Ok(exit_code)
        }
        None => Ok(0),
    }
}
