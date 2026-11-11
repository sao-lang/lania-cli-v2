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
use std::sync::Arc;

use lania_host::{ExecutionError, HostRuntime, EXIT_RUNTIME_ERROR};
use lania_logger::{AutoStyledLogRenderer, CliMessageLevel, LogRenderer, StderrLogSink};
use lania_preferences::load_preferences;

use crate::{
    cli_message_renderer, cli_parse_error_message, cli_text, cli_version, error_json_value,
    execution_json_value, localized_user_message, output_profile_from_sources,
    render_output_value, resolve_effective_locale, should_suppress_output, OutputMode,
};

pub async fn run_single_command(
    host: &mut HostRuntime,
    spec: CommandSpec,
    forwarded: Vec<String>,
) -> Result<i32> {
    let bare_invocation = forwarded.is_empty();
    run_command_entry(host, spec, forwarded, bare_invocation).await
}

/// 独立 bin 已把「子命令 token」当根 spec 处理（§5.3 C.4 剥名转发）：
/// 转发参数可能为空但语义上不是裸调用，不能回落 help。
pub async fn run_subcommand_entry(
    host: &mut HostRuntime,
    spec: CommandSpec,
    forwarded: Vec<String>,
) -> Result<i32> {
    run_command_entry(host, spec, forwarded, false).await
}

async fn run_command_entry(
    host: &mut HostRuntime,
    spec: CommandSpec,
    forwarded: Vec<String>,
    bare_invocation: bool,
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

    // 命令子包二进制也要有 stderr sink：dev 的 `[app]` 流式日志、`--grouped-output`、
    // Warn 级提示都依赖它。总包 main.rs 有一份等价装配，这里必须对齐，
    // 否则 `lan-<cmd>` 形态的日志会全部静默丢失。
    let renderer: Arc<dyn LogRenderer> = Arc::new(
        AutoStyledLogRenderer::default()
            .with_locale(locale.clone())
            .with_timestamps(
                preferences.log_timestamps && matches!(output_profile.mode, OutputMode::Human),
            ),
    );
    host.logger().add_sink(Arc::new(StderrLogSink::new(renderer)));

    let mut own = vec![spec.clone()];
    apply_legacy_aliases(&mut own);
    localize_command_specs(&locale, &mut own);

    let binary_name = "lan";
    let about = cli_text(&locale, "Lania CLI v2", "Lania CLI v2 命令行工具");
    let mut argv = vec![binary_name.to_string(), spec.name.clone()];
    argv.extend(forwarded.iter().cloned());
    // 只有用户真的什么都没给（裸调用）才回落 help；子命令级 spec（如
    // `lan workspace lint` -> 根 "lint"）裸调用时要走默认 handler 执行。
    if bare_invocation {
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
