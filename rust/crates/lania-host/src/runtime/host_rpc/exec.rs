use std::time::Duration;

use anyhow::{anyhow, Result};
use lania_exec::{ExecCommand, ExecRunOptions};
use serde_json::{json, Value};

use super::{
    deny, payload_cwd, payload_required_str, payload_strings, HostPayload, HostRpcAdapter,
    HostRpcResponse, HostToolsPolicySnapshot,
};

/// 执行命令相关的 RPC（`host.exec.*`）。
///
/// 这是最敏感的能力之一：
/// - 需要在宿主侧做策略兜底校验（例如 allowShell / allowEnvWrite）
/// - 需要对 payload 做归一化，避免把不安全/不可预期的数据形状带进执行层
///
/// 本模块负责：
/// - payload 归一化
/// - 宿主侧策略二次校验
/// 父文件保留统一的 RPC 分发与审计日志，避免分散在多个域里。
pub(super) fn handle_exec_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
    tools_policy: &HostToolsPolicySnapshot,
) -> Result<HostRpcResponse> {
    match method {
        "host.exec.shell" => handle_exec_shell(adapter, method, payload, tools_policy),
        "host.exec.run" | "host.exec.runChecked" => {
            handle_exec_run(adapter, method, payload, tools_policy)
        }
        "host.exec.history" => {
            let history = adapter
                .exec
                .history()
                .into_iter()
                .map(|command| {
                    json!({
                        "program": command.program,
                        "args": command.args,
                        "cwd": command.cwd,
                        "env": command.env,
                        "useShell": command.use_shell,
                    })
                })
                .collect::<Vec<_>>();
            Ok((json!(history), Vec::new()))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}

fn handle_exec_shell(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
    tools_policy: &HostToolsPolicySnapshot,
) -> Result<HostRpcResponse> {
    if tools_policy.exec_allow_shell == Some(false) {
        return Err(deny(
            "host.exec.shell is blocked (config.tools.exec.allowShell=false)",
        ));
    }
    let cwd = payload_cwd(payload).to_string();
    let script = payload_required_str(payload, "script", method)?;
    let timeout_ms = payload.get("timeoutMs").and_then(Value::as_u64);
    let checked = payload
        .get("checked")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let command = ExecCommand::shell(script).in_dir(cwd);
    let options = ExecRunOptions {
        timeout: timeout_ms.map(Duration::from_millis),
        ..ExecRunOptions::default()
    };
    let result = adapter.exec.run_with_options(command, options)?;
    if checked && result.exit_code != 0 {
        return Err(anyhow!(
            "host.exec.shell failed with exit code {}: {}",
            result.exit_code,
            result.stderr
        ));
    }
    Ok((
        json!({
            "exitCode": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
        }),
        Vec::new(),
    ))
}

fn handle_exec_run(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
    tools_policy: &HostToolsPolicySnapshot,
) -> Result<HostRpcResponse> {
    let program = payload_required_str(payload, "program", method)?;
    let args = payload_strings(payload, "args");
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let env = payload
        .get("env")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|item| (key.clone(), item.to_string()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !env.is_empty() && tools_policy.exec_allow_env_write == Some(false) {
        return Err(deny(
            "host.exec.run is blocked (config.tools.exec.allowEnvWrite=false)",
        ));
    }
    let timeout_ms = payload.get("timeoutMs").and_then(Value::as_u64);
    let kill_process_tree = payload
        .get("killProcessTree")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let use_shell = payload
        .get("useShell")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if use_shell && tools_policy.exec_allow_shell == Some(false) {
        return Err(deny(
            "host.exec.run is blocked (config.tools.exec.allowShell=false)",
        ));
    }

    let mut command = if use_shell {
        ExecCommand::shell(program)
    } else {
        ExecCommand::new(program).with_args(args)
    };
    if let Some(cwd) = cwd {
        command = command.in_dir(cwd);
    }
    for (key, value) in env {
        command = command.with_env(key, value);
    }

    let options = ExecRunOptions {
        timeout: timeout_ms.map(Duration::from_millis),
        kill_process_tree,
        ..ExecRunOptions::default()
    };
    let result = if method == "host.exec.runChecked" {
        adapter.exec.run_checked_with_options(command, options)?
    } else {
        adapter.exec.run_with_options(command, options)?
    };
    Ok((
        json!({
            "exitCode": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "skipped": result.skipped,
            "timedOut": result.timed_out,
            "cancelled": result.cancelled,
        }),
        Vec::new(),
    ))
}
