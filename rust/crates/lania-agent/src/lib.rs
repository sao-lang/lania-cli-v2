//! `lan agent`：pi 驱动的 coding agent 命令。
//!
//! 职责边界：
//! - 本 crate 只做「终端侧」：读输入、渲染流式事件、维护 REPL 生命周期。
//! - 模型循环在 node-bridge 的 agent 插件里（`@lania-cli/agent`）。
//! - 工具执行与审批在宿主 `agent.tool.exec`（见 `lania-host`）。

use std::io::{self, BufRead, Write};

use anyhow::Result;
use async_trait::async_trait;
use lania_command::{ArgSpec, CommandSpec, Example, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{
        CommandExecution, CommandExecutionContext, CommandHandler, ExecutionError,
        EXIT_RUNTIME_ERROR,
    },
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};
use lania_node_bridge::{BridgeEvent, BridgeEventMethod, BridgeExchange};
use serde_json::{json, Value};

pub const HANDLER_ID: &str = "command.agent";

#[derive(Debug, Default)]
pub struct AgentCommandPlugin;

struct AgentCommandHandler;

impl AgentCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new(
            "agent",
            "Run the pi-based coding agent (interactive REPL or one-shot)",
            HANDLER_ID,
        )
        .with_args(vec![ArgSpec {
            name: "prompt".into(),
            required: false,
            multiple: true,
            help: "Prompt text; when present the agent runs one-shot".into(),
        }])
        .with_options(vec![
            OptionSpec {
                long: "model".into(),
                short: Some('m'),
                help: "Model id passed to pi (e.g. claude-sonnet-4-5)".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "provider".into(),
                short: None,
                help: "Provider id (anthropic/openai/...)".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "thinking".into(),
                short: None,
                help: "Reasoning level".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![
                    "minimal".into(),
                    "low".into(),
                    "medium".into(),
                    "high".into(),
                    "xhigh".into(),
                    "max".into(),
                ],
                negatable: false,
            },
            OptionSpec {
                long: "approval-mode".into(),
                short: None,
                help: "Tool approval mode: always-ask | write | yolo".into(),
                value_kind: ValueKind::String,
                default_value: Some("always-ask".into()),
                choices: vec!["always-ask".into(), "write".into(), "yolo".into()],
                negatable: false,
            },
        ])
        .with_examples(vec![
            Example {
                command: "lan agent".into(),
                description: "Start an interactive agent session".into(),
            },
            Example {
                command: "lan agent \"explain the rust crate layout\"".into(),
                description: "One-shot prompt".into(),
            },
            Example {
                command: "lan agent --approval-mode yolo \"run the test suite\"".into(),
                description: "One-shot without approval prompts".into(),
            },
        ])
    }
}

impl Plugin for AgentCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-agent".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Exec,
                CapabilityName::Fs,
                CapabilityName::NodeBridge,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<AgentCommandHandler, _>(
            ctx,
            "command-agent",
            "agent",
            Self::spec(),
            &[HANDLER_ID],
            || AgentCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for AgentCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let cwd = ctx.command().cwd.clone();
        let provider = option_string(ctx, "provider");
        let model = option_string(ctx, "model");
        let thinking = option_string(ctx, "thinking");
        let approval_mode =
            option_string(ctx, "approval-mode").unwrap_or_else(|| "always-ask".into());
        let prompt = positional_prompt(ctx);

        let session_id = format!("lan-{}", std::process::id());
        create_session(
            ctx,
            &session_id,
            &cwd,
            provider.as_deref(),
            model.as_deref(),
            thinking.as_deref(),
            &approval_mode,
        )
        .await?;

        let exit_code = match prompt {
            Some(prompt) => run_prompt(ctx, &session_id, &prompt).await?,
            None => run_repl(ctx, &session_id, &model).await?,
        };

        Ok(ctx.complete_template_info(
            json!({ "kind": "agent_session", "status": "closed", "exitCode": exit_code }),
            exit_code,
        ))
    }
}

fn option_string(ctx: &CommandExecutionContext<'_>, key: &str) -> Option<String> {
    ctx.command()
        .argv
        .options
        .get(key)
        .and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
        .filter(|value| !value.trim().is_empty())
}

fn positional_prompt(ctx: &CommandExecutionContext<'_>) -> Option<String> {
    let value = ctx.command().argv.args.get("prompt")?;
    let parts: Vec<String> = match value {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        Value::String(text) => vec![text.clone()],
        _ => vec![],
    };
    let joined = parts.join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

async fn create_session(
    ctx: &CommandExecutionContext<'_>,
    session_id: &str,
    cwd: &str,
    provider: Option<&str>,
    model: Option<&str>,
    thinking: Option<&str>,
    approval_mode: &str,
) -> Result<()> {
    let request = ctx.node_bridge().request(
        "agent.session.create",
        json!({
            "sessionId": session_id,
            "cwd": cwd,
            "provider": provider,
            "model": model,
            "thinking": thinking,
            "approvalMode": approval_mode,
        }),
    );
    let exchange = ctx.node_bridge().call_async(request).await?;
    ensure_ok(&exchange).map_err(|message| {
        anyhow::Error::from(ExecutionError {
            exit_code: EXIT_RUNTIME_ERROR,
            message,
        })
    })?;

    if let Some(result) = exchange.response.result.as_ref() {
        let model_label = result
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        eprintln!("[lan agent] model: {model_label}");
        eprintln!("[lan agent] tools: {tools}");
        eprintln!("[lan agent] approval mode: {approval_mode}");
    }
    Ok(())
}

fn ensure_ok(exchange: &BridgeExchange) -> std::result::Result<(), String> {
    match exchange.response.error.as_ref() {
        Some(error) => Err(format!("{}: {}", error.code, error.message)),
        None => Ok(()),
    }
}

async fn run_prompt(
    ctx: &CommandExecutionContext<'_>,
    session_id: &str,
    prompt: &str,
) -> Result<i32> {
    let request = ctx.node_bridge().request(
        "agent.prompt",
        json!({ "sessionId": session_id, "prompt": prompt }),
    );
    let mut call = ctx.node_bridge().open_call(request)?;
    while let Some(event) = call.next_event().await {
        render_event(&event);
    }
    let exchange = call.collect_exchange().await?;
    println!();
    if let Err(message) = ensure_ok(&exchange) {
        eprintln!("[lan agent] error: {message}");
        return Ok(EXIT_RUNTIME_ERROR);
    }
    Ok(0)
}

async fn run_repl(
    ctx: &CommandExecutionContext<'_>,
    session_id: &str,
    model: &Option<String>,
) -> Result<i32> {
    eprintln!("[lan agent] interactive mode. /help for commands, /quit to exit.");
    loop {
        eprint!("lan> ");
        let _ = io::stderr().flush();

        let mut line = String::new();
        let read = io::stdin().lock().read_line(&mut line)?;
        if read == 0 {
            eprintln!();
            return Ok(0);
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        match input {
            "/quit" | "/exit" => return Ok(0),
            "/help" => {
                eprintln!("  /help          show this help");
                eprintln!("  /model         show the active model");
                eprintln!("  /quit          exit the session");
                continue;
            }
            "/model" => {
                match model.as_deref() {
                    Some(model) => eprintln!("  model: {model}"),
                    None => eprintln!("  model: provider default"),
                }
                continue;
            }
            _ => {}
        }

        run_prompt(ctx, session_id, input).await?;
    }
}

fn render_event(event: &BridgeEvent) {
    match event.method {
        BridgeEventMethod::AgentTextDelta => {
            if let Some(delta) = event.params.get("delta").and_then(Value::as_str) {
                print!("{delta}");
                let _ = io::stdout().flush();
            }
        }
        BridgeEventMethod::AgentToolStart => {
            let tool = event
                .params
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let args = event
                .params
                .get("args")
                .map(compact_json)
                .unwrap_or_default();
            eprintln!("\n[lan agent] {tool} {args}");
        }
        BridgeEventMethod::AgentToolEnd => {
            let tool = event
                .params
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let ok = event
                .params
                .get("ok")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let duration = event
                .params
                .get("durationMs")
                .and_then(Value::as_u64)
                .map(|value| format!(" {value}ms"))
                .unwrap_or_default();
            eprintln!(
                "[lan agent] {tool} {}{duration}",
                if ok { "done" } else { "failed" }
            );
            let _ = io::stderr().flush();
        }
        BridgeEventMethod::AgentEnd => {
            let _ = io::stdout().flush();
        }
        _ => {}
    }
}

fn compact_json(value: &Value) -> String {
    let text = value.to_string();
    if text.len() <= 200 {
        text
    } else {
        let mut end = 200;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &text[..end])
    }
}
