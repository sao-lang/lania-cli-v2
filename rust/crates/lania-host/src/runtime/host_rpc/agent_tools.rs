//! `agent.*` 的宿主 RPC 实现：agent 工具执行 + 审批。
//!
//! 设计要点（与 omp 的结构性差异）：
//! - 工具在 Rust 执行，Node 只发一次 RPC 并等待结果；
//! - 策略与审批在**同一次调用内**完成，不存在“先授权、后执行”的窗口；
//! - 审批结果可持久化到 `.lan/agent/approvals.json`，键为 `tool:target`。
use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::Path,
    time::Duration,
};

use anyhow::{anyhow, Result};
use ignore::{overrides::OverrideBuilder, WalkBuilder};
use regex::Regex;
use serde_json::{json, Value};

use lania_exec::{ExecCommand, ExecRunOptions};

use super::{
    payload_cwd, payload_required_str, resolve_path_from_cwd, HostPayload, HostRpcAdapter,
    HostRpcResponse,
};

const MAX_READ_LINES: usize = 2_000;
const MAX_OUTPUT_BYTES: usize = 20_000;
const MAX_GLOB_RESULTS: usize = 500;
const MAX_GREP_MATCHES: usize = 200;
const DEFAULT_EXEC_TIMEOUT_MS: u64 = 600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Read,
    Write,
    Exec,
}

impl Tier {
    fn label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Exec => "exec",
        }
    }
}

enum Approval {
    Granted { remember: bool },
    Denied,
}

pub(super) fn handle_agent_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<HostRpcResponse> {
    match method {
        "agent.tool.exec" => exec_tool(adapter, payload),
        _ => Err(anyhow!("unsupported host rpc method: {method}")),
    }
}

fn tier_for(tool: &str) -> Result<Tier> {
    match tool {
        "read" | "glob" | "grep" => Ok(Tier::Read),
        "edit" | "write" => Ok(Tier::Write),
        "exec" | "project_command" => Ok(Tier::Exec),
        other => Err(anyhow!("unknown agent tool: {other}")),
    }
}

fn exec_tool(adapter: &HostRpcAdapter, payload: &HostPayload) -> Result<HostRpcResponse> {
    let cwd = payload_cwd(payload).to_string();
    let tool = payload_required_str(payload, "tool", "agent.tool.exec")?;
    let params = payload.get("params").cloned().unwrap_or(Value::Null);
    let approval_mode = payload
        .get("approvalMode")
        .and_then(Value::as_str)
        .unwrap_or("always-ask");
    let tier = tier_for(&tool)?;
    let target = approval_target(&tool, &params, &cwd);

    if !auto_approved(tier, approval_mode) {
        let grant_key = format!("{tool}:{target}");
        if !has_grant(&cwd, &grant_key) {
            match ask_user(tier, &tool, &target) {
                Approval::Granted { remember } => {
                    if remember {
                        save_grant(&cwd, &grant_key)?;
                    }
                }
                Approval::Denied => {
                    return Ok((
                        json!({
                            "status": "denied",
                            "isError": true,
                            "content": format!(
                                "tool call denied by user: {tool} ({target}). Do not retry the same call; ask the user how to proceed."
                            ),
                            "details": { "tier": tier.label(), "target": target },
                        }),
                        Vec::new(),
                    ));
                }
            }
        }
    }

    match run_tool(adapter, &tool, &params, &cwd) {
        Ok(outcome) => Ok((
            json!({
                "status": "ok",
                "content": outcome.content,
                "details": outcome.details,
                "isError": outcome.is_error,
            }),
            Vec::new(),
        )),
        // 工具失败（文件不存在、命令非零退出等）作为模型可见结果返回，而不是 RPC 错误，
        // 这样 agent 可以自行纠正，而不是整个请求失败。
        Err(error) => Ok((
            json!({
                "status": "ok",
                "isError": true,
                "content": format!("{error:#}"),
                "details": { "tier": tier.label(), "target": target },
            }),
            Vec::new(),
        )),
    }
}

fn auto_approved(tier: Tier, mode: &str) -> bool {
    match mode {
        "yolo" => true,
        "write" => matches!(tier, Tier::Read | Tier::Write),
        _ => tier == Tier::Read,
    }
}

fn approval_target(tool: &str, params: &Value, cwd: &str) -> String {
    match tool {
        "exec" => params
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("<no command>")
            .trim()
            .to_string(),
        "project_command" => {
            let args = params
                .get("args")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            format!("lan {args}")
        }
        _ => params
            .get("path")
            .and_then(Value::as_str)
            .map(|path| resolve_path_from_cwd(cwd, path).display().to_string())
            .unwrap_or_else(|| "<no path>".to_string()),
    }
}

fn approvals_path(cwd: &str) -> std::path::PathBuf {
    Path::new(cwd).join(".lan/agent/approvals.json")
}

fn read_grants(cwd: &str) -> Vec<String> {
    let path = approvals_path(cwd);
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    value
        .get("grants")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn has_grant(cwd: &str, key: &str) -> bool {
    read_grants(cwd).iter().any(|grant| grant == key)
}

fn save_grant(cwd: &str, key: &str) -> Result<()> {
    let mut grants = read_grants(cwd);
    if !grants.iter().any(|grant| grant == key) {
        grants.push(key.to_string());
    }
    let path = approvals_path(cwd);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(&json!({ "version": 1, "grants": grants }))?,
    )?;
    Ok(())
}

fn ask_user(tier: Tier, tool: &str, target: &str) -> Approval {
    if !io::stdin().is_terminal() {
        eprintln!(
            "[lan agent] {tier} {tool} requires approval, but stdin is not a terminal; denying. Re-run with --approval-mode yolo to allow.",
            tier = tier.label()
        );
        return Approval::Denied;
    }

    eprintln!();
    eprintln!("[lan agent] approval required ({})", tier.label());
    eprintln!("  {tool}: {target}");
    eprint!("  [a] allow once  [s] allow always  [d] deny > ");
    let _ = io::stderr().flush();

    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return Approval::Denied;
    }
    match line.trim().to_ascii_lowercase().as_str() {
        "a" | "y" | "allow" => Approval::Granted { remember: false },
        "s" | "always" => Approval::Granted { remember: true },
        _ => Approval::Denied,
    }
}

#[derive(Debug)]
struct ToolOutcome {
    content: String,
    details: Value,
    is_error: bool,
}

fn run_tool(adapter: &HostRpcAdapter, tool: &str, params: &Value, cwd: &str) -> Result<ToolOutcome> {
    match tool {
        "read" => tool_read(params, cwd),
        "glob" => tool_glob(params, cwd),
        "grep" => tool_grep(params, cwd),
        "edit" => tool_edit(params, cwd),
        "exec" => tool_exec(adapter, params, cwd),
        "project_command" => tool_project_command(adapter, params, cwd),
        other => Err(anyhow!("unknown agent tool: {other}")),
    }
}

fn param_str<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing `{key}` parameter"))
}

fn param_opt_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn truncate(text: String, limit: usize) -> String {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n… [truncated {} bytes]",
        &text[..end],
        text.len() - end
    )
}

fn tool_read(params: &Value, cwd: &str) -> Result<ToolOutcome> {
    let path = param_str(params, "path")?;
    let resolved = resolve_path_from_cwd(cwd, path);
    let content = fs::read_to_string(&resolved)
        .map_err(|error| anyhow!("failed to read {}: {error}", resolved.display()))?;
    if content.contains('\0') {
        return Err(anyhow!(
            "{} looks like a binary file; use `exec` if you really need it",
            resolved.display()
        ));
    }

    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let start = params
        .get("start")
        .and_then(Value::as_u64)
        .map(|value| value.max(1) as usize)
        .unwrap_or(1);
    let end = params
        .get("end")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(total)
        .min(total);

    if total == 0 {
        return Ok(ToolOutcome {
            content: String::new(),
            details: json!({ "path": path, "lines": 0 }),
            is_error: false,
        });
    }
    if start > end {
        return Err(anyhow!("start ({start}) is greater than end ({end})"));
    }

    let body: String = lines[start - 1..end]
        .iter()
        .take(MAX_READ_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ToolOutcome {
        content: truncate(body, MAX_OUTPUT_BYTES),
        details: json!({
            "path": path,
            "lines": total,
            "shownFrom": start,
            "shownTo": end.min(start.saturating_sub(1) + MAX_READ_LINES),
        }),
        is_error: false,
    })
}

fn build_walker(base: &Path, pattern: Option<&str>) -> Result<WalkBuilder> {
    let mut builder = WalkBuilder::new(base);
    builder.standard_filters(true);
    if let Some(pattern) = pattern {
        let mut overrides = OverrideBuilder::new(base);
        overrides.add(pattern)?;
        builder.overrides(overrides.build()?);
    }
    Ok(builder)
}

fn relative_label(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn tool_glob(params: &Value, cwd: &str) -> Result<ToolOutcome> {
    let pattern = param_str(params, "pattern")?;
    let base = resolve_path_from_cwd(cwd, param_opt_str(params, "path").unwrap_or("."));
    let walker = build_walker(&base, Some(pattern))?.build();

    let mut paths = Vec::new();
    for entry in walker.filter_map(Result::ok) {
        let is_file = entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false);
        if !is_file {
            continue;
        }
        paths.push(relative_label(&base, entry.path()));
        if paths.len() >= MAX_GLOB_RESULTS {
            break;
        }
    }
    paths.sort();

    Ok(ToolOutcome {
        content: if paths.is_empty() {
            "no files matched".to_string()
        } else {
            paths.join("\n")
        },
        details: json!({ "count": paths.len(), "truncated": paths.len() >= MAX_GLOB_RESULTS }),
        is_error: false,
    })
}

fn tool_grep(params: &Value, cwd: &str) -> Result<ToolOutcome> {
    let pattern = param_str(params, "pattern")?;
    let regex = Regex::new(pattern).map_err(|error| anyhow!("invalid regex: {error}"))?;
    let base = resolve_path_from_cwd(cwd, param_opt_str(params, "path").unwrap_or("."));
    let file_glob = param_opt_str(params, "glob");
    let walker = build_walker(&base, file_glob)?.build();

    let mut matches = Vec::new();
    for entry in walker.filter_map(Result::ok) {
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(content) = fs::read_to_string(entry.path()) else {
            continue;
        };
        if content.contains('\0') {
            continue;
        }
        let label = relative_label(&base, entry.path());
        for (index, line) in content.lines().enumerate() {
            if regex.is_match(line) {
                matches.push(format!("{label}:{}: {}", index + 1, line.trim_end()));
                if matches.len() >= MAX_GREP_MATCHES {
                    break;
                }
            }
        }
        if matches.len() >= MAX_GREP_MATCHES {
            break;
        }
    }

    Ok(ToolOutcome {
        content: if matches.is_empty() {
            "no matches".to_string()
        } else {
            matches.join("\n")
        },
        details: json!({ "count": matches.len(), "truncated": matches.len() >= MAX_GREP_MATCHES }),
        is_error: false,
    })
}

fn tool_edit(params: &Value, cwd: &str) -> Result<ToolOutcome> {
    let path = param_str(params, "path")?;
    let old_string = param_str(params, "old_string")?;
    let new_string = param_str(params, "new_string")?;
    let replace_all = params
        .get("replace_all")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let resolved = resolve_path_from_cwd(cwd, path);
    let content = fs::read_to_string(&resolved)
        .map_err(|error| anyhow!("failed to read {}: {error}", resolved.display()))?;

    let occurrences = content.matches(old_string).count();
    if occurrences == 0 {
        return Err(anyhow!(
            "old_string not found in {path}; read the file again and include exact context"
        ));
    }
    if occurrences > 1 && !replace_all {
        return Err(anyhow!(
            "old_string matches {occurrences} times in {path}; add more context or pass replace_all=true"
        ));
    }

    let updated = if replace_all {
        content.replace(old_string, new_string)
    } else {
        content.replacen(old_string, new_string, 1)
    };
    fs::write(&resolved, &updated)
        .map_err(|error| anyhow!("failed to write {}: {error}", resolved.display()))?;

    let replaced = if replace_all { occurrences } else { 1 };
    Ok(ToolOutcome {
        content: format!(
            "edited {path}: {replaced} replacement(s), {} -> {} bytes",
            content.len(),
            updated.len()
        ),
        details: json!({
            "path": path,
            "replacements": replaced,
            "bytesBefore": content.len(),
            "bytesAfter": updated.len(),
        }),
        is_error: false,
    })
}

fn exec_result_outcome(result: lania_exec::ExecResult, label: &str) -> ToolOutcome {
    let mut body = String::new();
    if !result.stdout.trim().is_empty() {
        body.push_str(&result.stdout);
    }
    if !result.stderr.trim().is_empty() {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&result.stderr);
    }
    if body.trim().is_empty() {
        body = format!("{label}: no output");
    }
    if result.timed_out {
        body = format!("{body}\n[timed out]");
    }

    ToolOutcome {
        content: truncate(body, MAX_OUTPUT_BYTES),
        details: json!({
            "exitCode": result.exit_code,
            "timedOut": result.timed_out,
            "cancelled": result.cancelled,
            "skipped": result.skipped,
        }),
        is_error: result.exit_code != 0 || result.timed_out,
    }
}

fn run_options(params: &Value) -> ExecRunOptions {
    let timeout_ms = params
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_EXEC_TIMEOUT_MS)
        .max(1_000);
    ExecRunOptions {
        timeout: Some(Duration::from_millis(timeout_ms)),
        ..ExecRunOptions::default()
    }
}

fn tool_exec(adapter: &HostRpcAdapter, params: &Value, cwd: &str) -> Result<ToolOutcome> {
    let command = param_str(params, "command")?;
    let request = ExecCommand::shell(command).in_dir(cwd);
    let result = adapter.exec.run_with_options(request, run_options(params))?;
    Ok(exec_result_outcome(result, command))
}

fn tool_project_command(
    adapter: &HostRpcAdapter,
    params: &Value,
    cwd: &str,
) -> Result<ToolOutcome> {
    let args: Vec<String> = params
        .get("args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if args.is_empty() {
        return Err(anyhow!("`args` must contain at least one argument"));
    }

    let program = std::env::var("LANIA_AGENT_LAN_BIN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .map(|path| path.display().to_string())
        })
        .ok_or_else(|| anyhow!("unable to resolve the lan binary; set LANIA_AGENT_LAN_BIN"))?;

    let label = format!("lan {}", args.join(" "));
    let request = ExecCommand::new(program).with_args(args).in_dir(cwd);
    let result = adapter.exec.run_with_options(request, run_options(params))?;
    Ok(exec_result_outcome(result, &label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lania-agent-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn read_tier_is_auto_approved_in_every_mode() {
        assert!(auto_approved(Tier::Read, "always-ask"));
        assert!(auto_approved(Tier::Read, "write"));
        assert!(auto_approved(Tier::Read, "yolo"));
    }

    #[test]
    fn write_and_exec_require_approval_outside_yolo() {
        assert!(!auto_approved(Tier::Write, "always-ask"));
        assert!(auto_approved(Tier::Write, "write"));
        assert!(!auto_approved(Tier::Exec, "always-ask"));
        assert!(!auto_approved(Tier::Exec, "write"));
        assert!(auto_approved(Tier::Exec, "yolo"));
    }

    #[test]
    fn unknown_tool_is_rejected() {
        assert!(tier_for("rm_rf").is_err());
    }

    #[test]
    fn exec_target_is_the_trimmed_command() {
        let params = json!({ "command": "  pnpm test  " });
        assert_eq!(approval_target("exec", &params, "/repo"), "pnpm test");
    }

    #[test]
    fn file_target_is_resolved_against_cwd() {
        let params = json!({ "path": "src/a.ts" });
        let target = approval_target("edit", &params, "/repo").replace('\\', "/");
        assert!(target.ends_with("/repo/src/a.ts"), "unexpected target: {target}");
    }

    #[test]
    fn read_returns_requested_line_range() {
        let dir = temp_workspace("read");
        fs::write(dir.join("f.txt"), "one\ntwo\nthree\n").unwrap();
        let cwd = dir.display().to_string();
        let outcome = tool_read(&json!({ "path": "f.txt", "start": 2, "end": 3 }), &cwd).unwrap();
        assert_eq!(outcome.content, "two\nthree");
        assert!(!outcome.is_error);
    }

    #[test]
    fn edit_rejects_ambiguous_match_without_replace_all() {
        let dir = temp_workspace("edit");
        fs::write(dir.join("f.txt"), "x\nx\n").unwrap();
        let cwd = dir.display().to_string();
        let error = tool_edit(
            &json!({ "path": "f.txt", "old_string": "x", "new_string": "y" }),
            &cwd,
        )
        .unwrap_err();
        assert!(error.to_string().contains("matches 2 times"));
    }

    #[test]
    fn grep_reports_path_and_line() {
        let dir = temp_workspace("grep");
        fs::write(dir.join("f.txt"), "alpha\nbeta\n").unwrap();
        let cwd = dir.display().to_string();
        let outcome = tool_grep(&json!({ "pattern": "beta" }), &cwd).unwrap();
        assert!(outcome.content.contains("f.txt:2: beta"), "{}", outcome.content);
    }

    #[test]
    fn glob_lists_matching_files() {
        let dir = temp_workspace("glob");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/a.ts"), "x").unwrap();
        let cwd = dir.display().to_string();
        let outcome = tool_glob(&json!({ "pattern": "src/*.ts" }), &cwd).unwrap();
        assert_eq!(outcome.content, "src/a.ts");
    }
}
