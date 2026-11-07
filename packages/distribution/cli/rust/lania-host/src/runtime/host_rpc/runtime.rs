//! `host.runtime.*`：运行时识别、计划与执行。
//!
//! 与 `host.pm.*` 一样，这里不直接拼 shell；`plan` 只返回命令快照。
//! 唯一例外是 `run`：编译型语言需要 prepare 与临时产物清理，二者必须和真正的执行
//! 放在同一个 Rust 调用里，才能保证清理不会被 Node 侧策略或异常路径绕过。

use std::path::Path;

use anyhow::{anyhow, Result};
use lania_runtime::RunRequest;
use serde_json::{json, Value};

use super::{
    payload_cwd, payload_required_str, payload_strings, HostPayload, HostRpcAdapter,
    HostRpcResponse,
};

pub(super) fn handle_runtime_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<HostRpcResponse> {
    match method {
        "host.runtime.list" => Ok((json!({ "runtimes": adapter.runtime.list() }), Vec::new())),
        "host.runtime.detect" => {
            let file = payload_required_str(payload, "file", method)?;
            let path = resolve_file(payload_cwd(payload), &file);
            let detected = adapter.runtime.detect(&path)?;
            Ok((
                json!({
                    "file": detected.file,
                    "language": detected.language,
                    "runtime": detected.runtime,
                }),
                Vec::new(),
            ))
        }
        "host.runtime.spec" => {
            let language = payload_required_str(payload, "language", method)?;
            let spec = adapter
                .runtime
                .spec(&language)
                .ok_or_else(|| anyhow!("unknown runtime language: {language}"))?;
            Ok((serde_json::to_value(spec)?, Vec::new()))
        }
        "host.runtime.plan" => {
            let file = payload_required_str(payload, "file", method)?;
            let cwd = payload_cwd(payload).to_string();
            let path = resolve_file(&cwd, &file);
            let extra_args = payload_strings(payload, "args");
            let plan = adapter.runtime.plan(&path, Path::new(&cwd))?;
            let command =
                adapter
                    .runtime
                    .build_command(&path, &plan, &extra_args, Path::new(&cwd))?;
            Ok((
                json!({
                    "runtime": plan.runtime,
                    "language": plan.language,
                    "program": command.program,
                    "args": command.args,
                    "cwd": command.cwd,
                    "preparedCommand": prepared_command_json(plan.prepared_command.as_ref()),
                    "preparedSummary": plan.prepared_summary,
                }),
                Vec::new(),
            ))
        }
        "host.runtime.run" => {
            let file = payload_required_str(payload, "file", method)?;
            let cwd = payload_cwd(payload).to_string();
            let request = RunRequest {
                path: resolve_file(&cwd, &file),
                cwd: cwd.into(),
                args: payload_strings(payload, "args"),
            };
            let result = adapter.runtime.run(&adapter.exec, &request)?;
            Ok((serde_json::to_value(result)?, Vec::new()))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}

fn resolve_file(cwd: &str, file: &str) -> std::path::PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    }
}

fn prepared_command_json(command: Option<&lania_exec::ExecCommand>) -> Value {
    match command {
        Some(command) => json!({
            "program": command.program,
            "args": command.args,
            "cwd": command.cwd,
        }),
        None => Value::Null,
    }
}
