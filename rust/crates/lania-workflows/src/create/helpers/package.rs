//! `create` 工作流里的包管理器辅助逻辑。
//!
//! 这一层负责把“模板/工作流想装哪些依赖”转成真正可执行的包管理器动作：
//! - 解析当前项目应使用的包管理器
//! - 为未显式带版本的依赖补齐版本号
//! - 执行安装命令，并把 shell / hook / progress / logger 串起来
use std::{collections::BTreeMap, path::Path};

use anyhow::{anyhow, Result};
use lania_exec::{ExecCommand, ExecError, ExecErrorCode, ExecEvent, ExecRunOptions};
use lania_logger::LogLevel;
use lania_pm::{PackageCommand, PackageManager, PackageManagerService};
use serde_json::{json, Value};
use tokio::{sync::Semaphore, task::JoinSet};

use crate::models::WorkflowServices;

pub(crate) fn resolve_package_manager(
    service: &PackageManagerService,
    raw: Option<&str>,
) -> PackageManager {
    // 显式传入的包管理器优先；否则回退到基于项目文件的自动探测。
    match raw {
        Some("pnpm") => PackageManager::Pnpm,
        Some("yarn") => PackageManager::Yarn,
        Some("bun") => PackageManager::Bun,
        Some("npm") => PackageManager::Npm,
        _ => service.detect_from_files(["package.json"]),
    }
}

pub(crate) fn command_to_vec(command: &PackageCommand) -> Vec<String> {
    // hook 和日志更适合消费扁平命令数组，因此这里统一展开。
    std::iter::once(command.program.clone())
        .chain(command.args.clone())
        .collect()
}

pub(crate) async fn resolve_dependency_versions(
    services: &WorkflowServices,
    manager: PackageManager,
    cwd: &Path,
    dependencies: &[String],
    dev_dependencies: &[String],
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>)> {
    // 版本解析策略：
    // - 如果依赖声明里已经带版本，直接透传
    // - 否则并发执行 `<pm> view <pkg> version --json` 查询
    // - 查询工具不存在时保守回退到 `latest`
    //
    // 这里故意限制并发度为 10，避免一次 create 在大模板上把 registry 查询打得过猛。
    let total = dependencies.len() + dev_dependencies.len();
    let progress_id = "GetDependencies";
    services.progress.begin(progress_id, Some(total as u64));
    services.progress.message(progress_id, "GetDependencies");
    if total == 0 {
        services.progress.finish(progress_id);
        return Ok((BTreeMap::new(), BTreeMap::new()));
    }

    let semaphore = std::sync::Arc::new(Semaphore::new(10));
    let mut tasks = JoinSet::new();
    let exec = services.exec.clone();
    let lookup_program = dependency_lookup_program(manager).to_string();
    let lookup_cwd = cwd.display().to_string();

    for (is_dev, entries) in [(false, dependencies), (true, dev_dependencies)] {
        for entry in entries {
            let entry = entry.clone();
            let exec = exec.clone();
            let lookup_program = lookup_program.clone();
            let lookup_cwd = lookup_cwd.clone();
            let permit = semaphore.clone().acquire_owned().await?;
            tasks.spawn(async move {
                let _permit = permit;
                let (name, preset_version) = split_dependency_spec(&entry);
                let version = match preset_version {
                    Some(version) => version.to_string(),
                    None => {
                        let command = ExecCommand::new(lookup_program)
                            .with_args([
                                "view".to_string(),
                                name.to_string(),
                                "version".to_string(),
                                "--json".to_string(),
                            ])
                            .in_dir(lookup_cwd);
                        match exec
                            .run_with_options_async(command, ExecRunOptions::default())
                            .await
                        {
                            Ok(result) if result.stdout.trim().is_empty() => "latest".to_string(),
                            Ok(result) => parse_dependency_version(&result.stdout)?,
                            Err(error)
                                if error.downcast_ref::<ExecError>().is_some_and(
                                    |exec_error| exec_error.code == ExecErrorCode::BinaryMissing,
                                ) =>
                            {
                                "latest".to_string()
                            }
                            Err(error) => return Err(error),
                        }
                    }
                };
                Ok::<(bool, String, String), anyhow::Error>((is_dev, name.to_string(), version))
            });
        }
    }

    let mut resolved_dependencies = BTreeMap::new();
    let mut resolved_dev_dependencies = BTreeMap::new();
    while let Some(result) = tasks.join_next().await {
        let (is_dev, name, version) = result??;
        if is_dev {
            resolved_dev_dependencies.insert(name, version);
        } else {
            resolved_dependencies.insert(name, version);
        }
        services.progress.advance(progress_id, 1);
        services.progress.detail(
            progress_id,
            format!(
                "{}/{}",
                resolved_dependencies.len() + resolved_dev_dependencies.len(),
                total
            ),
        );
    }
    services.progress.finish(progress_id);
    Ok((resolved_dependencies, resolved_dev_dependencies))
}

fn dependency_lookup_program(manager: PackageManager) -> &'static str {
    // 只有 npm/pnpm 直接使用自身二进制查询版本；其它 manager 统一回退到 npm view。
    match manager {
        PackageManager::Npm | PackageManager::Pnpm => manager.binary(),
        _ => "npm",
    }
}

fn split_dependency_spec(spec: &str) -> (&str, Option<&str>) {
    // `name@version` 拆分时要保留 scoped package 的 `@scope/` 前缀，因此取最后一个 `@`。
    let version_separator = spec.rfind('@');
    match version_separator {
        Some(index) if index > 0 => (&spec[..index], Some(&spec[index + 1..])),
        _ => (spec, None),
    }
}

fn parse_dependency_version(stdout: &str) -> Result<String> {
    // registry 返回值既可能是纯字符串，也可能是 JSON array/object，这里统一收敛。
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("dependency version lookup returned empty output"));
    }
    let parsed = serde_json::from_str::<Value>(trimmed)
        .unwrap_or_else(|_| Value::String(trimmed.to_string()));
    match parsed {
        Value::String(value) if !value.trim().is_empty() => Ok(value),
        Value::Array(values) => values
            .into_iter()
            .find_map(|value| value.as_str().map(ToOwned::to_owned))
            .ok_or_else(|| anyhow!("dependency version lookup returned no string version")),
        Value::Object(map) => map
            .get("version")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow!("dependency version lookup object missing version field")),
        _ => Err(anyhow!(
            "dependency version lookup returned unsupported payload"
        )),
    }
}

pub(crate) async fn run_package_command(
    services: &WorkflowServices,
    workflow: &str,
    cwd: &Path,
    command: &PackageCommand,
) -> Result<()> {
    // 安装/更新依赖时，这里是实际执行入口：
    // - 发 before/after hooks
    // - 把 exec 事件翻译成 logger 输出
    // - 更新依赖下载进度条
    let program = command.program.clone();
    let args = command.args.clone();
    let cwd_string = cwd.display().to_string();
    let scoped = services.logger.scoped("exec");
    let progress_id = "DownloadDependencies";
    services.progress.begin(progress_id, Some(1));
    services
        .progress
        .message(progress_id, "DownloadDependencies");
    services.progress.detail(
        progress_id,
        format!("{} {}", command.program, command.args.join(" ")),
    );

    services
        .hooks
        .call_parallel(
            format!("workflow:{workflow}"),
            lania_hooks::hook_keys::ON_DEPENDENCIES_INSTALL.to_string(),
            json!({
                "cwd": services.hook_cwd.as_str(),
                "traceId": services.hook_trace_id.as_str(),
                "command": { "name": services.hook_command_handler_id.as_str(), "handlerId": services.hook_command_handler_id.as_str() },
                "workflow": { "name": workflow },
                "install": { "stage": "before", "cwd": cwd_string, "command": command_to_vec(command) }
            }),
        )
        .await
        .ok();
    services
        .hooks
        .call_parallel(
            format!("workflow:{workflow}"),
            lania_hooks::hook_keys::ON_SHELL_COMMAND.to_string(),
            json!({
                "cwd": services.hook_cwd.as_str(),
                "traceId": services.hook_trace_id.as_str(),
                "command": { "name": services.hook_command_handler_id.as_str(), "handlerId": services.hook_command_handler_id.as_str() },
                "workflow": { "name": workflow },
                "shell": { "stage": "before", "cwd": cwd_string, "program": program, "args": args }
            }),
        )
        .await
        .ok();

    scoped.log(
        LogLevel::Debug,
        format!("run: {} {}", command.program, command.args.join(" ")),
    );

    let options = ExecRunOptions {
        on_event: Some(std::sync::Arc::new(move |event| match event {
            ExecEvent::Started { command, cwd } => {
                scoped.log(
                    LogLevel::Debug,
                    format!("started: {} (cwd={})", command, cwd.unwrap_or_default()),
                );
            }
            ExecEvent::Stdout(line) => {
                if !line.trim().is_empty() {
                    scoped.log(LogLevel::Debug, line);
                }
            }
            ExecEvent::Stderr(line) => {
                if !line.trim().is_empty() {
                    scoped.log(LogLevel::Warn, line);
                }
            }
            ExecEvent::TimedOut { timeout_ms } => {
                scoped.log(LogLevel::Error, format!("timed out after {}ms", timeout_ms));
            }
            ExecEvent::Cancelled => {
                scoped.log(LogLevel::Warn, "cancelled".to_string());
            }
            ExecEvent::Finished { exit_code } => {
                if exit_code == 0 {
                    scoped.log(LogLevel::Debug, "finished: exit=0".to_string());
                } else {
                    scoped.log(LogLevel::Warn, format!("finished: exit={}", exit_code));
                }
            }
        })),
        ..ExecRunOptions::default()
    };

    let result = services
        .exec
        .run_with_options_async(
            command.to_exec_command().in_dir(cwd.display().to_string()),
            options,
        )
        .await?;

    services
        .hooks
        .call_parallel(
            format!("workflow:{workflow}"),
            lania_hooks::hook_keys::ON_SHELL_COMMAND.to_string(),
            json!({
                "cwd": services.hook_cwd.as_str(),
                "traceId": services.hook_trace_id.as_str(),
                "command": { "name": services.hook_command_handler_id.as_str(), "handlerId": services.hook_command_handler_id.as_str() },
                "workflow": { "name": workflow },
                "shell": { "stage": "after", "cwd": cwd.display().to_string(), "exitCode": result.exit_code }
            }),
        )
        .await
        .ok();
    services
        .hooks
        .call_parallel(
            format!("workflow:{workflow}"),
            lania_hooks::hook_keys::ON_DEPENDENCIES_INSTALL.to_string(),
            json!({
                "cwd": services.hook_cwd.as_str(),
                "traceId": services.hook_trace_id.as_str(),
                "command": { "name": services.hook_command_handler_id.as_str(), "handlerId": services.hook_command_handler_id.as_str() },
                "workflow": { "name": workflow },
                "install": { "stage": "after", "cwd": cwd.display().to_string(), "exitCode": result.exit_code }
            }),
        )
        .await
        .ok();

    if result.exit_code != 0 {
        services
            .progress
            .fail(progress_id, format!("exit={}", result.exit_code));
        return Err(anyhow!(
            "package manager command failed with exit code {}",
            result.exit_code
        ));
    }
    services.progress.advance(progress_id, 1);
    services.progress.finish(progress_id);
    Ok(())
}
