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
use lania_agent::AgentCommandPlugin;
use lania_host::{plugin::EmptyPlugin, ExecutionError, Host, HostRuntime, EXIT_RUNTIME_ERROR};
use lania_logger::{AutoStyledLogRenderer, CliMessageLevel, LogRenderer, StderrLogSink};
use lania_plugins_command_add::AddCommandPlugin;
use lania_plugins_command_build::BuildCommandPlugin;
use lania_plugins_command_create::CreateCommandPlugin;
use lania_plugins_command_dev::DevCommandPlugin;
use lania_plugins_command_env::EnvCommandPlugin;
use lania_plugins_command_generate::GenerateCommandPlugin;
use lania_plugins_command_git::GitCommandPlugin;
use lania_plugins_command_lint::LintCommandPlugin;
use lania_plugins_command_locale::ConfigCommandPlugin;
use lania_plugins_command_release::ReleaseCommandPlugin;
use lania_plugins_command_template::TemplateCommandPlugin;
use lania_plugins_command_tools::ToolsCommandPlugin;
use lania_progress::TerminalProgressMode;

mod output;
mod profile;

use output::{error_json_value, execution_json_value, render_output_value, should_suppress_output};
use profile::{
    cli_message_renderer, cli_parse_error_message, cli_text, cli_version, localized_user_message,
    normalize_os_args, output_profile_from_sources, resolve_effective_locale, OutputMode,
    OutputProfile, ProgressMode,
};

#[tokio::main]
async fn main() -> Result<()> {
    // 必须最先执行：之后所有子进程（Rust exec / node bridge / compiler worker / 工具链）
    // 都继承本进程 PATH，`lan env use <runtime> <ver>` 才会真正生效。
    activate_managed_runtimes();

    // B11：同一个二进制兼作 Rust 运行时 bridge（`lan __lania_rs_bridge`）。
    //
    // 必须在任何宿主装配之前短路：bridge 模式只需要 stdin/stdout 协议循环，
    // 不加载插件、不读项目配置，也不打印任何额外输出（§3.1 约定 1）。
    if lania_rs_bridge::is_bridge_invocation(env::args_os()) {
        let code = lania_rs_bridge::run_stdio_bridge()?;
        std::process::exit(code);
    }

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
    host.register_plugin(Box::new(GitCommandPlugin))?;
    host.register_plugin(Box::new(TemplateCommandPlugin))?;
    host.register_plugin(Box::new(ToolsCommandPlugin))?;
    host.register_plugin(Box::new(EnvCommandPlugin))?;
    host.register_plugin(Box::new(AgentCommandPlugin))?;
    host.register_plugin(Box::new(ConfigCommandPlugin))?;
    host.initialize().await?;
    let cwd = env::current_dir()?.display().to_string();
    // 输出 envelope 只关心“当前是否是 git 仓库以及分支名”。
    // 非仓库/未就绪时保持 None，不要让 git 探测失败影响命令本身。
    let git_branch = host
        .git()
        .status(std::path::Path::new(&cwd))
        .ok()
        .filter(|status| status.ready)
        .and_then(|status| status.branch);
    let project_config = host
        .load_lan_config_snapshot_from_cwd_async(cwd.clone())
        .await
        .ok();
    let preferences = lania_preferences::load_preferences();
    let locale = resolve_effective_locale(project_config.as_ref(), &preferences);
    let mut output_profile = output_profile_from_sources(project_config.as_ref(), &preferences);
    output_profile.locale = locale.clone();
    host.set_locale(&locale);

    // 配置校验/加载问题必须让用户看见（§6.5.9），但不阻断命令。
    // 之前 `validation_errors` 只存在于 snapshot 里，没有任何一处渲染它。
    if let Some(config) = project_config.as_ref() {
        let message_renderer = cli_message_renderer(locale.as_str(), &preferences, &output_profile);
        for error in &config.validation_errors {
            eprintln!(
                "{}",
                message_renderer.render(
                    CliMessageLevel::Warn,
                    format!("lan.config {}: {}", error.position.path, error.message),
                )
            );
        }
    }
    let _ = host
        .bootstrap_project_extensions_from_cwd_async(cwd.clone())
        .await?;
    // 登记语言 bridge（§5.3：装了就更好，不装也能用）。
    //
    // 全部 best-effort：解析失败时保持未登记，`lint` / `build` 回落到 JS 簇路径。
    // 必须在 `execute_command` 之前 —— 注册表是懒加载的，登记本身不启动进程。
    //
    // - Python（B8）：登记只做路径探测 + 脚本落盘，成本可忽略。
    // - Go（B10）/ Dart（B9）：**延迟解析**，首次真正需要 bridge 时才编译/探测，
    //   所以 Go 项目里跑 `lan env` / `lan add` 不会白白付一次 `go build`。
    if let Err(error) = host.register_python_bridge(std::path::Path::new(&cwd)) {
        host.logger().log(
            lania_logger::LogLevel::Debug,
            format!("lan-py-bridge not registered: {error}"),
        );
    }
    if let Err(error) = host.register_go_bridge(std::path::Path::new(&cwd)) {
        host.logger().log(
            lania_logger::LogLevel::Debug,
            format!("lan-go-bridge not registered: {error}"),
        );
    }
    if let Err(error) = host.register_dart_bridge(std::path::Path::new(&cwd)) {
        host.logger().log(
            lania_logger::LogLevel::Debug,
            format!("lan-dart-bridge not registered: {error}"),
        );
    }
    if let Err(error) = host.register_rust_bridge(std::path::Path::new(&cwd)) {
        host.logger().log(
            lania_logger::LogLevel::Debug,
            format!("lan-rs-bridge not registered: {error}"),
        );
    }
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
                    error_json_value(None, &cwd, exit_code, &message),
                    &output_profile,
                )?
            );
            host.shutdown_async().await?;
            std::process::exit(exit_code);
        }
    };

    let context =
        lania_command::command_context_from_matches(&commands, &matches, cwd.clone(), "trace-1");
    let execution = match &context {
        // 解析到了具体命令：交给 HostRuntime 执行（可能走 workflow 或 node-bridge）。
        Some(context) => Some(host.execute_command(context).await),
        None => None,
    };

    host.shutdown_async().await?;

    match execution {
        Some(Ok(execution)) => {
            let exit_code = execution.exit_code();
            let value =
                execution_json_value(&execution, &output_profile, &cwd, git_branch.as_deref());
            // 交互型命令（agent）自己拥有终端：REPL 结束后不再追加任何宿主输出。
            let output = if should_suppress_output(&value) {
                String::new()
            } else {
                render_output_value(value, &output_profile)?
            };
            print!("{output}");
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Some(Err(error)) => {
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
                    error_json_value(context.as_ref(), &cwd, exit_code, &message),
                    &output_profile,
                )?
            );
            std::process::exit(exit_code);
        }
        None => {}
    }

    Ok(())
}

/// 把 `~/.runex/shims` 前置到本进程 PATH（幂等）。
///
/// 失败必须**静默忽略**：自管运行时是可选能力，任何解析/权限问题都不应阻断命令本身。
fn activate_managed_runtimes() {
    if let Ok(home) = runex::RuntimeHome::resolve() {
        home.activate();
    }
}
