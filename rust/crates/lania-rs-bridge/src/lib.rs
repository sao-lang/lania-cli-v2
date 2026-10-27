//! Rust 运行时 bridge（`lan-rs-bridge`，§3.5 / §11 B11）。
//!
//! 与其它语言 bridge 不同，这里**没有内嵌源码、也没有单独的产物**：
//! `lan` 二进制本身以「bridge 模式」启动（`lan __lania_rs_bridge`）就承担这个角色。
//!
//! 为什么这么做：
//! - §3.5 的结论是「零 API 收益」—— `cargo build` / `cargo clippy` 架构上必须是子进程，
//!   所以 bridge 内不需要一门语言的运行时；
//! - 但协议层仍然需要一个「独立的进程」来统一 `Diagnostic` / `event.diag`，
//!   用已有二进制复用协议与 `serde_json`，比再造一个单文件 Rust 程序（手写 JSON）
//!   或新增 3 个平台包便宜得多；
//! - 顺带满足 §8.2 的 `--version` 自报版本要求（同一二进制）。
//!
//! 支持的 method：
//!
//! | method          | 行为                                                        |
//! | --------------- | ----------------------------------------------------------- |
//! | `lint.clippy`   | bridge 内 exec `cargo clippy --message-format=json`，JSONL 解析 |
//! | `build.rust`    | bridge 内 exec Rust 侧解析好的 argv，逐行 `event.diag`       |
//!
//! 注意：`lint.rust` 是文档里的方法名，实际 adaptor id 是 `clippy`（见 `toolchain.ts`），
//! 因此这里以 `lint.clippy` 为准，同时接受 `lint.rust` 作为别名。

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};

/// `lan` 进入 bridge 模式的隐藏参数（`lan __lania_rs_bridge`）。
pub const BRIDGE_ARG: &str = "__lania_rs_bridge";

/// 覆盖 bridge 可执行文件（测试用：指向不存在的路径即可断言回落路径）。
pub const BRIDGE_BINARY_ENV: &str = "LANIA_RS_BRIDGE";

pub const BRIDGE_NAME: &str = "lan-rs-bridge";

const PROTOCOL_VERSION: &str = "0.1.0";
const HEARTBEAT_INTERVAL_MS: u64 = 10_000;
const MAX_PENDING_EVENTS: u64 = 32;

const METHODS: &[&str] = &[
    "bridge.ping",
    "bridge.heartbeat",
    "bridge.shutdown",
    "lint.clippy",
    "build.rust",
];

const EVENTS: &[&str] = &[
    "event.ready",
    "event.log",
    "event.diag",
    "event.lint_start",
    "event.lint_result",
    "event.lint_summary",
    "event.shutdown",
    "event.heartbeat",
];

/// bridge 启动方式：`program + args + dir`（复用 `NodeBridgeClient`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustBridgeLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

/// 当前进程是否以 bridge 模式被调用。
pub fn is_bridge_invocation<I>(args: I) -> bool
where
    I: IntoIterator,
    I::Item: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .nth(1)
        .and_then(|value| value.as_ref().to_str().map(str::to_owned))
        .is_some_and(|value| value == BRIDGE_ARG)
}

/// 生成 `lan-rs-bridge` 的启动参数。
///
/// 默认就是「自己再起一个自己」：`current_exe + [BRIDGE_ARG]`。
/// `LANIA_RS_BRIDGE` 可覆盖（预编译产物 / 测试里制造不可用场景）。
pub fn launch_spec(cwd: &Path) -> Result<RustBridgeLaunch> {
    let (program, args) =
        match std::env::var_os(BRIDGE_BINARY_ENV).filter(|value| !value.is_empty()) {
            Some(explicit) => (PathBuf::from(explicit), Vec::new()),
            None => (
                std::env::current_exe()
                    .map_err(|error| anyhow!("failed to resolve current executable: {error}"))?,
                vec![BRIDGE_ARG.to_string()],
            ),
        };
    Ok(RustBridgeLaunch {
        program,
        args,
        dir: cwd.to_path_buf(),
    })
}

// ---------------------------------------------------------------------------
// 诊断模型（§7.3，字段与 Rust `Diagnostic` / TS `diagnostics.ts` 一致）
// ---------------------------------------------------------------------------

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "error" => 0,
        "warning" => 1,
        "info" => 2,
        _ => 3,
    }
}

fn start_line(item: &Value) -> i64 {
    item["range"]["start"]["line"].as_i64().unwrap_or(0)
}

fn start_col(item: &Value) -> i64 {
    item["range"]["start"]["col"].as_i64().unwrap_or(0)
}

/// 去重键 `(tool, file, line, col, id)` + 排序 `file → line → col → severity → id`。
///
/// 与 `lania_node_bridge::normalize_diagnostics` / Python bridge 的
/// `normalize_diagnostics` 同语义：多语言 bridge 必须给出同一份顺序，
/// 否则同一份 clippy 输出经不同路径渲染结果会不同。
pub fn normalize_diagnostics(items: Vec<Value>) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped: Vec<Value> = items
        .into_iter()
        .filter(|item| {
            let key = format!(
                "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
                item["tool"].as_str().unwrap_or(""),
                item["file"].as_str().unwrap_or(""),
                start_line(item),
                start_col(item),
                item["id"].as_str().unwrap_or("")
            );
            seen.insert(key)
        })
        .collect();
    deduped.sort_by(|left, right| {
        let left_file = left["file"].as_str().unwrap_or("");
        let right_file = right["file"].as_str().unwrap_or("");
        left_file
            .cmp(right_file)
            .then_with(|| start_line(left).cmp(&start_line(right)))
            .then_with(|| start_col(left).cmp(&start_col(right)))
            .then_with(|| {
                severity_rank(left["severity"].as_str().unwrap_or("info"))
                    .cmp(&severity_rank(right["severity"].as_str().unwrap_or("info")))
            })
            .then_with(|| {
                left["id"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(right["id"].as_str().unwrap_or(""))
            })
    });
    deduped
}

fn lint_result(adaptor: &str, implementation: &str, diagnostics: Vec<Value>) -> Value {
    let normalized = normalize_diagnostics(diagnostics);
    let mut files: Vec<Value> = Vec::new();
    let mut errors = 0u64;
    let mut warnings = 0u64;
    for item in &normalized {
        let file_path = item["file"].as_str().unwrap_or(".");
        let is_error = item["severity"].as_str() == Some("error");
        if is_error {
            errors += 1;
        } else {
            warnings += 1;
        }
        match files
            .iter_mut()
            .find(|entry: &&mut Value| entry["filePath"].as_str() == Some(file_path))
        {
            Some(entry) => {
                let counter = if is_error { "errors" } else { "warnings" };
                let current = entry[counter].as_u64().unwrap_or(0);
                entry[counter] = json!(current + 1);
            }
            None => files.push(json!({
                "filePath": file_path,
                "errors": if is_error { 1 } else { 0 },
                "warnings": if is_error { 0 } else { 1 },
            })),
        }
    }
    json!({
        "adaptor": adaptor,
        "implementation": implementation,
        "errors": errors,
        "warnings": warnings,
        "files": files,
        "diagnostics": normalized,
        "tool": adaptor,
    })
}

/// 工具**执行失败**（不是「没问题」）必须是一条可见诊断（与 Go / Dart bridge 同语义）。
pub fn failed_lint_result(adaptor: &str, message: &str) -> Value {
    let message = message.trim();
    let message = if message.is_empty() {
        format!("{adaptor} failed")
    } else {
        truncate(message, 400)
    };
    lint_result(
        adaptor,
        "runtime",
        vec![json!({
            "tool": adaptor,
            "severity": "error",
            "message": message,
        })],
    )
}

fn truncate(text: &str, limit: usize) -> String {
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        out.push('…');
    }
    out
}

// ---------------------------------------------------------------------------
// 解析器：`cargo clippy --message-format=json`
// ---------------------------------------------------------------------------

fn severity_from_level(level: &str) -> &'static str {
    match level {
        "error" | "deny" => "error",
        "warning" | "warn" => "warning",
        "info" | "note" => "info",
        "hint" | "help" => "hint",
        _ => "error",
    }
}

/// `cargo clippy --message-format=json` 的 JSONL：只取 `reason == "compiler-message"`。
///
/// 与 `ts/packages/node-bridge/src/plugins/lint/parsers.ts::parseClippyDiagnostics`
/// 逐字段对齐：`code.code` / `message` / 第一个带 `file_name` 的 span /
/// 任一 span 的 `suggested_replacement` ⇒ `fix.available`。
pub fn parse_clippy_jsonl(output: &str) -> Vec<Value> {
    let mut diagnostics = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record["reason"].as_str() != Some("compiler-message") {
            continue;
        }
        let message = &record["message"];
        let level = message["level"].as_str().unwrap_or("error");
        let spans: Vec<&Value> = message["spans"]
            .as_array()
            .map(|items| items.iter().collect())
            .unwrap_or_default();
        let span = spans
            .iter()
            .find(|span| span["file_name"].as_str().is_some())
            .copied();
        let fixable = spans
            .iter()
            .any(|span| span["suggested_replacement"].as_str().is_some());

        let mut diagnostic = Map::new();
        diagnostic.insert("tool".into(), json!("clippy"));
        if let Some(code) = message["code"]["code"].as_str() {
            diagnostic.insert("id".into(), json!(code));
        }
        diagnostic.insert("severity".into(), json!(severity_from_level(level)));
        diagnostic.insert(
            "message".into(),
            json!(message["message"].as_str().unwrap_or("")),
        );
        if let Some(span) = span {
            if let Some(file) = span["file_name"].as_str() {
                diagnostic.insert("file".into(), json!(file));
            }
            if let Some(line) = span["line_start"].as_i64() {
                let mut range = Map::new();
                range.insert(
                    "start".into(),
                    json!({
                        "line": line,
                        "col": span["column_start"].as_i64().unwrap_or(1),
                    }),
                );
                if let Some(end_line) = span["line_end"].as_i64() {
                    range.insert(
                        "end".into(),
                        json!({
                            "line": end_line,
                            "col": span["column_end"].as_i64().unwrap_or(1),
                        }),
                    );
                }
                diagnostic.insert("range".into(), Value::Object(range));
            }
        }
        if fixable {
            diagnostic.insert("fix".into(), json!({ "available": true, "kind": "safe" }));
        }
        diagnostics.push(Value::Object(diagnostic));
    }
    diagnostics
}

/// 编译错误文本：`file:line:col: message`（与 Go / Dart bridge 同一套正则语义）。
///
/// 反向贪心（`rsplit` 找最后两个数字段）而不是正则：路径可能含 `:`（Windows 盘符），
/// 用「最后一个 `:N:N:`」定位最稳。
pub fn parse_location_text(tool: &str, severity: &str, output: &str) -> Vec<Value> {
    let mut diagnostics = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let Some((rest, message)) = split_location(line) else {
            continue;
        };
        let mut diagnostic = Map::new();
        diagnostic.insert("tool".into(), json!(tool));
        diagnostic.insert("severity".into(), json!(severity));
        diagnostic.insert("message".into(), json!(message));
        diagnostic.insert("file".into(), json!(rest.0));
        diagnostic.insert(
            "range".into(),
            json!({ "start": { "line": rest.1, "col": rest.2 } }),
        );
        diagnostics.push(Value::Object(diagnostic));
    }
    diagnostics
}

/// 把 `file:line:col: message` 拆成 `((file, line, col), message)`。
fn split_location(line: &str) -> Option<((String, i64, i64), String)> {
    // message 部分：第一个 `: ` 之后
    let message_index = line.find(": ")?;
    let head = &line[..message_index];
    let message = line[message_index + 2..].trim();
    if message.is_empty() {
        return None;
    }
    // head 的末尾是 `...:line:col`
    let mut parts = head.rsplitn(3, ':');
    let col = parts.next()?.trim().parse::<i64>().ok()?;
    let line_number = parts.next()?.trim().parse::<i64>().ok()?;
    let file = parts.next()?.to_string();
    if file.is_empty() {
        return None;
    }
    Some(((file, line_number, col), message.to_string()))
}

// ---------------------------------------------------------------------------
// 子进程执行 + 流式劫持（§7.2）
// ---------------------------------------------------------------------------

static SEQ: AtomicU64 = AtomicU64::new(0);

fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed) + 1
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

/// 一行协议输出（stdout）。所有日志必须走 stderr（§3.1 约定 1）。
fn emit(value: &Value) {
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{value}").is_err() {
        return;
    }
    let _ = stdout.flush();
}

fn emit_event(method: &str, params: Value, request_id: &str) {
    emit(&json!({
        "type": "event",
        "requestId": if request_id.is_empty() { "bridge" } else { request_id },
        "payload": { "method": method, "params": params },
    }));
}

fn emit_diag(request_id: &str, text: &str, stream: &str, channel: &str, level: &str, tool: &str) {
    let mut params = json!({
        "source": "tool",
        "stream": stream,
        "channel": channel,
        "level": level,
        "text": text,
        "seq": next_seq(),
        "ts": now_ms(),
    });
    params["tool"] = json!(tool);
    emit_event("event.diag", params, request_id);
}

fn emit_response(request_id: &str, result: Option<Value>, error: Option<Value>) {
    emit(&json!({
        "type": "response",
        "payload": { "id": request_id, "result": result, "error": error },
    }));
}

fn error_payload(code: &str, message: &str) -> Value {
    json!({ "code": code, "message": message, "data": null })
}

struct RunOutcome {
    exit_code: i32,
    stdout: String,
    stderr: String,
    failure: Option<String>,
}

/// 执行命令；`stream_diag` 为 `Some((request_id, channel))` 时逐行发 `event.diag`。
fn run_command(
    cwd: &Path,
    program: &str,
    args: &[String],
    tool: &str,
    stream_diag: Option<(&str, &str)>,
) -> RunOutcome {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .env("FORCE_COLOR", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return RunOutcome {
                exit_code: 127,
                stdout: String::new(),
                stderr: String::new(),
                failure: Some(format!("failed to start {program}: {error}")),
            };
        }
    };
    // 两个流都要读出来，否则大输出会把子进程堵在半路。
    let mut stderr = String::new();
    if let Some(handle) = child.stderr.take() {
        for line in std::io::BufReader::new(handle)
            .lines()
            .map_while(Result::ok)
        {
            if let Some((request_id, channel)) = stream_diag {
                if !line.trim().is_empty() {
                    let level = if line.to_ascii_lowercase().contains("error") {
                        "error"
                    } else {
                        "info"
                    };
                    emit_diag(request_id, &line, "stderr", channel, level, tool);
                }
            }
            stderr.push_str(&line);
            stderr.push('\n');
        }
    }
    let mut stdout = String::new();
    if let Some(handle) = child.stdout.take() {
        for line in std::io::BufReader::new(handle)
            .lines()
            .map_while(Result::ok)
        {
            if let Some((request_id, channel)) = stream_diag {
                if !line.trim().is_empty() {
                    emit_diag(request_id, &line, "stdout", channel, "info", tool);
                }
            }
            stdout.push_str(&line);
            stdout.push('\n');
        }
    }
    let exit_code = match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            return RunOutcome {
                exit_code: 1,
                stdout,
                stderr,
                failure: Some(format!("failed to wait for {program}: {error}")),
            };
        }
    };
    RunOutcome {
        exit_code,
        stdout,
        stderr,
        failure: None,
    }
}

// ---------------------------------------------------------------------------
// 请求处理
// ---------------------------------------------------------------------------

fn param_string(params: &Value, key: &str) -> Option<String> {
    params[key].as_str().map(ToOwned::to_owned)
}

fn param_string_list(params: &Value, key: &str) -> Vec<String> {
    params[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn default_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn handle_lint(adaptor: &str, params: &Value, request_id: &str) -> Value {
    let cwd = param_string(params, "cwd")
        .map(PathBuf::from)
        .unwrap_or_else(default_cwd);
    let mut args = param_string_list(params, "args");

    emit_diag(
        request_id,
        &format!("running {adaptor}"),
        "stdout",
        "lint",
        "info",
        adaptor,
    );

    if adaptor != "clippy" && adaptor != "rust" {
        return json!({
            "adaptor": adaptor,
            "implementation": "fallback",
            "errors": 0,
            "warnings": 0,
            "files": [],
            "diagnostics": [],
            "message": format!("unsupported lint adaptor: {adaptor}"),
            "tool": adaptor,
        });
    }
    if args.is_empty() {
        args = vec!["clippy".into(), "--message-format=json".into()];
    }
    let outcome = run_command(&cwd, "cargo", &args, "clippy", None);
    if let Some(failure) = outcome.failure {
        // `cargo` 不可用：这是「工具缺失」而不是 lint 结果。
        return failed_lint_result("clippy", &failure);
    }
    let mut diagnostics = parse_clippy_jsonl(&outcome.stdout);
    if diagnostics.is_empty() {
        // clippy 有时把编译错误写到 stderr（JSON 解析失败时也一样），兜底按文本解析。
        diagnostics = parse_location_text("clippy", "error", &outcome.stderr);
    }
    if diagnostics.is_empty() && outcome.exit_code != 0 {
        return failed_lint_result("clippy", &outcome.stderr);
    }
    lint_result("clippy", "runtime", diagnostics)
}

fn handle_build(params: &Value, request_id: &str) -> Value {
    let cwd = param_string(params, "cwd")
        .map(PathBuf::from)
        .unwrap_or_else(default_cwd);
    let output_dir = param_string(params, "outputDir").unwrap_or_else(|| "bin/app".into());
    let mut command = param_string_list(params, "command");
    if command.is_empty() {
        command = vec!["build".into(), "--release".into()];
    }
    let program = command.remove(0);
    let outcome = run_command(
        &cwd,
        &program,
        &command,
        "rust",
        Some((request_id, "build")),
    );

    let mut diagnostics = parse_location_text("rust", "error", &outcome.stderr);
    diagnostics.extend(parse_location_text("rust", "error", &outcome.stdout));
    let diagnostics = normalize_diagnostics(diagnostics);
    let success = outcome.failure.is_none() && outcome.exit_code == 0;
    let mut result = json!({
        "success": success,
        "tool": "rust",
        "runtime": "rust",
        "action": "build",
        "bridge": BRIDGE_NAME,
        "exitCode": outcome.exit_code,
        "outputDir": output_dir,
        "diagnostics": diagnostics,
        "command": std::iter::once(program).chain(command).collect::<Vec<_>>(),
    });
    if !success {
        let message = outcome
            .failure
            .clone()
            .or_else(|| {
                outcome
                    .stderr
                    .lines()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim().to_string())
            })
            .unwrap_or_else(|| format!("cargo build exited with code {}", outcome.exit_code));
        result["error"] = json!(message);
    }
    result
}

fn handshake_response(request: &Value) -> Value {
    let protocol = request["protocolVersion"]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": protocol,
        "bridgeName": BRIDGE_NAME,
        "methods": METHODS,
        "events": EVENTS,
        "heartbeatIntervalMs": HEARTBEAT_INTERVAL_MS,
        "maxPendingEvents": MAX_PENDING_EVENTS,
        "failureStrategy": "reconnect",
    })
}

/// 处理一条 request；返回 `false` 表示应当退出（`bridge.shutdown`）。
fn handle_request(envelope: &Value) -> bool {
    let payload = &envelope["payload"];
    let request_id = payload["id"].as_str().unwrap_or("");
    let method = payload["method"].as_str().unwrap_or("");
    let params = payload["params"].clone();
    let params = if params.is_object() {
        params
    } else {
        json!({})
    };

    match method {
        "bridge.ping" => {
            emit_response(
                request_id,
                Some(
                    json!({ "ok": true, "bridgeName": BRIDGE_NAME, "version": env!("CARGO_PKG_VERSION") }),
                ),
                None,
            );
            true
        }
        "bridge.heartbeat" => {
            emit_response(
                request_id,
                Some(json!({ "ok": true, "ts": now_ms() })),
                None,
            );
            true
        }
        "bridge.shutdown" => {
            emit_event(
                "event.shutdown",
                json!({ "reason": "shutdown" }),
                request_id,
            );
            emit_response(request_id, Some(json!({ "ok": true })), None);
            false
        }
        _ if method.starts_with("lint.") => {
            let adaptor = method.trim_start_matches("lint.");
            emit_event(
                "event.lint_start",
                json!({ "adaptors": [adaptor] }),
                request_id,
            );
            let result = handle_lint(adaptor, &params, request_id);
            emit_event(
                "event.lint_result",
                json!({
                    "adaptor": result["adaptor"],
                    "errors": result["errors"],
                    "warnings": result["warnings"],
                    "implementation": result["implementation"],
                }),
                request_id,
            );
            emit_response(request_id, Some(result), None);
            true
        }
        "build.rust" => {
            let result = handle_build(&params, request_id);
            emit_response(request_id, Some(result), None);
            true
        }
        other => {
            emit_response(
                request_id,
                None,
                Some(error_payload(
                    "E_UNSUPPORTED",
                    &format!("unsupported method: {other}"),
                )),
            );
            true
        }
    }
}

/// bridge 主循环：逐行读 stdin，EOF（父进程消失）即退出（§3.1 约定 4）。
pub fn run_stdio_bridge() -> Result<i32> {
    emit_event(
        "event.ready",
        json!({
            "bridgeName": BRIDGE_NAME,
            "methods": METHODS,
            "pid": std::process::id(),
        }),
        "",
    );

    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut buffer = String::new();
    loop {
        buffer.clear();
        let read = reader.read_line(&mut buffer)?;
        if read == 0 {
            return Ok(0);
        }
        let line = buffer.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(envelope) = serde_json::from_str::<Value>(line) else {
            eprintln!("failed to parse envelope: {line}");
            continue;
        };
        match envelope["type"].as_str() {
            Some("handshake") => {
                let empty = json!({});
                let request = if envelope["payload"].is_object() {
                    &envelope["payload"]
                } else {
                    &empty
                };
                emit(&json!({
                    "type": "handshake",
                    "payload": handshake_response(request),
                }));
            }
            Some("request") => {
                if !handle_request(&envelope) {
                    return Ok(0);
                }
            }
            // 本 bridge 不发起 host RPC；收到响应说明上游版本不匹配，忽略。
            Some("host_response") => {}
            other => eprintln!("unsupported envelope type: {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_invocation_is_detected_by_the_hidden_argument() {
        assert!(is_bridge_invocation(["lan", BRIDGE_ARG]));
        assert!(is_bridge_invocation([
            "C:\\bin\\lan.exe",
            BRIDGE_ARG,
            "--version"
        ]));
        assert!(!is_bridge_invocation(["lan"]));
        assert!(!is_bridge_invocation(["lan", "lint"]));
    }

    #[test]
    fn launch_spec_reuses_the_current_binary_by_default() {
        let dir = std::env::temp_dir();
        // 不依赖 LANIA_RS_BRIDGE 的开发者环境：先清掉再断言。
        let previous = std::env::var_os(BRIDGE_BINARY_ENV);
        std::env::remove_var(BRIDGE_BINARY_ENV);
        let launch = launch_spec(&dir).expect("launch spec");
        assert_eq!(launch.args, vec![BRIDGE_ARG.to_string()]);
        assert!(launch.program.is_absolute());
        assert_eq!(launch.dir, dir);
        if let Some(previous) = previous {
            std::env::set_var(BRIDGE_BINARY_ENV, previous);
        }
    }

    #[test]
    fn launch_spec_honours_the_override() {
        let previous = std::env::var_os(BRIDGE_BINARY_ENV);
        std::env::set_var(BRIDGE_BINARY_ENV, "/opt/lan-rs-bridge");
        let launch = launch_spec(Path::new("/repo")).expect("launch spec");
        assert_eq!(launch.program, PathBuf::from("/opt/lan-rs-bridge"));
        assert!(launch.args.is_empty());
        match previous {
            Some(value) => std::env::set_var(BRIDGE_BINARY_ENV, value),
            None => std::env::remove_var(BRIDGE_BINARY_ENV),
        }
    }

    #[test]
    fn parses_clippy_jsonl_into_full_diagnostics() {
        let output = r#"
{"reason":"compiler-artifact","target":{"name":"demo"}}
{"reason":"compiler-message","message":{"level":"warning","message":"this loop could be written as an iterator","code":{"code":"clippy::needless_range_loop"},"spans":[{"file_name":"src/main.rs","line_start":4,"column_start":5,"line_end":4,"column_end":28,"suggested_replacement":"v.iter().enumerate()"}]}}
{"reason":"compiler-message","message":{"level":"error","message":"mismatched types","spans":[]}}
"#;
        let diagnostics = parse_clippy_jsonl(output);
        assert_eq!(diagnostics.len(), 2);

        let first = &diagnostics[0];
        assert_eq!(first["tool"], "clippy");
        assert_eq!(first["id"], "clippy::needless_range_loop");
        assert_eq!(first["severity"], "warning");
        assert_eq!(first["file"], "src/main.rs");
        assert_eq!(first["range"]["start"]["line"], 4);
        assert_eq!(first["range"]["start"]["col"], 5);
        assert_eq!(first["range"]["end"]["line"], 4);
        assert_eq!(first["fix"]["available"], true);

        // 没有 span 的 crate 级错误：`file` / `range` 缺省（§7.3 可选性约定）。
        let second = &diagnostics[1];
        assert_eq!(second["severity"], "error");
        assert!(second.get("file").is_none(), "{second}");
        assert!(second.get("range").is_none(), "{second}");
        assert!(second.get("id").is_none(), "{second}");
    }

    #[test]
    fn clippy_parser_ignores_non_message_reasons_and_broken_lines() {
        let output = "not json\n{\"reason\":\"build-finished\",\"success\":true}\n";
        assert!(parse_clippy_jsonl(output).is_empty());
    }

    #[test]
    fn location_text_parses_windows_paths_and_colons_in_paths() {
        let output = "   Compiling demo v0.1.0\nC:\\repo\\src\\main.rs:12:9: error[E0425]: cannot find value `x`\n";
        let diagnostics = parse_location_text("rust", "error", output);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["file"], "C:\\repo\\src\\main.rs");
        assert_eq!(diagnostics[0]["range"]["start"]["line"], 12);
        assert_eq!(diagnostics[0]["range"]["start"]["col"], 9);
        assert_eq!(
            diagnostics[0]["message"],
            "error[E0425]: cannot find value `x`"
        );
    }

    #[test]
    fn normalize_dedupes_and_sorts_like_the_other_bridges() {
        let items = vec![
            json!({"tool":"clippy","id":"b","severity":"warning","message":"second","file":"b.rs","range":{"start":{"line":1,"col":1}}}),
            json!({"tool":"clippy","id":"a","severity":"error","message":"first","file":"a.rs","range":{"start":{"line":2,"col":1}}}),
            json!({"tool":"clippy","id":"a","severity":"error","message":"first","file":"a.rs","range":{"start":{"line":2,"col":1}}}),
        ];
        let normalized = normalize_diagnostics(items);
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0]["file"], "a.rs");
        assert_eq!(normalized[1]["file"], "b.rs");
    }

    #[test]
    fn failed_lint_result_is_visible_as_an_error_diagnostic() {
        let result = failed_lint_result("clippy", "  cargo is not installed  ");
        assert_eq!(result["implementation"], "runtime");
        assert_eq!(result["errors"], 1);
        assert_eq!(result["diagnostics"][0]["severity"], "error");
        assert_eq!(
            result["diagnostics"][0]["message"],
            "cargo is not installed"
        );
    }

    #[test]
    fn lint_result_derives_files_from_diagnostics() {
        let result = lint_result(
            "clippy",
            "runtime",
            vec![
                json!({"tool":"clippy","severity":"error","message":"boom","file":"src/main.rs","range":{"start":{"line":3,"col":1}}}),
                json!({"tool":"clippy","severity":"warning","message":"meh","file":"src/main.rs","range":{"start":{"line":9,"col":1}}}),
            ],
        );
        assert_eq!(result["errors"], 1);
        assert_eq!(result["warnings"], 1);
        assert_eq!(result["files"][0]["filePath"], "src/main.rs");
        assert_eq!(result["files"][0]["errors"], 1);
        assert_eq!(result["files"][0]["warnings"], 1);
    }
}
