//! 交互式 REPL。
//!
//! 与 omp 的差异不在「长得不一样」，而在**职责位置**：这里没有任何模型/工具逻辑，
//! 只有「读一行 → 交给运行时 → 打印事件」。斜杠命令全部改运行时状态并重建会话，
//! 因此 `/model`、`/compact`、`/new`、`/fork` 走的是同一条代码路径。

use std::io::{self, BufRead, Write};

use anyhow::Result;
use lania_agent_core::{AgentSessionClient, ApprovalMode, ThinkingLevel};

use crate::render::Renderer;
use crate::runtime::PiRuntime;

/// 斜杠命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Help,
    Quit,
    Status,
    Compact,
    Rewind(Option<String>),
    Sessions,
    New,
    Fork,
    Model(Option<String>),
    Thinking(Option<String>),
    Approval(Option<String>),
    /// 以 `/` 开头但不认识：提示而不是当成 prompt 发给模型。
    Unknown(String),
}

/// 解析一行输入；非斜杠命令返回 `None`（应当作为 prompt 处理）。
pub fn parse(line: &str) -> Option<SlashCommand> {
    let trimmed = line.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed[1..].splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default().to_ascii_lowercase();
    let argument = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    Some(match name.as_str() {
        "help" | "h" | "?" => SlashCommand::Help,
        "quit" | "exit" | "q" => SlashCommand::Quit,
        "status" | "st" => SlashCommand::Status,
        "compact" => SlashCommand::Compact,
        "rewind" | "undo" => SlashCommand::Rewind(argument),
        "sessions" => SlashCommand::Sessions,
        "new" => SlashCommand::New,
        "fork" => SlashCommand::Fork,
        "model" => SlashCommand::Model(argument),
        "thinking" | "think" => SlashCommand::Thinking(argument),
        "approve" | "approval" => SlashCommand::Approval(argument),
        other => SlashCommand::Unknown(other.to_string()),
    })
}

/// REPL 的退出原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplExit {
    Quit,
    EndOfInput,
}

/// 跑交互循环。
pub async fn run<C: AgentSessionClient>(
    runtime: &mut PiRuntime<C>,
    renderer: &mut Renderer,
) -> Result<ReplExit> {
    renderer.notice("interactive mode · /help for commands · Ctrl-D to exit");
    loop {
        print!("› ");
        let _ = io::stdout().flush();

        let mut line = String::new();
        // 阻塞读一行：REPL 期间没有别的任务需要这个线程（bridge 在自己的线程上跑）。
        let read = io::stdin().lock().read_line(&mut line)?;
        if read == 0 {
            return Ok(ReplExit::EndOfInput);
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        match parse(input) {
            Some(SlashCommand::Quit) => return Ok(ReplExit::Quit),
            Some(command) => {
                if let Err(error) = handle(command, runtime, renderer).await {
                    renderer.error(&format!("{error:#}"));
                }
            }
            None => {
                if let Err(error) = runtime.run_turn(input, renderer).await {
                    renderer.error(&format!("{error:#}"));
                }
            }
        }
    }
}

async fn handle<C: AgentSessionClient>(
    command: SlashCommand,
    runtime: &mut PiRuntime<C>,
    renderer: &mut Renderer,
) -> Result<()> {
    match command {
        SlashCommand::Quit => {}
        SlashCommand::Help => {
            for (name, help) in [
                ("/help", "show this help"),
                (
                    "/status",
                    "session, model, thinking, approval, context usage",
                ),
                ("/model [id]", "show or switch model (replays history)"),
                ("/thinking [level]", "show or cycle reasoning level"),
                (
                    "/approve [mode]",
                    "switch approval mode: always-ask | write | yolo",
                ),
                ("/compact", "summarize old messages and rebuild the session"),
                (
                    "/rewind [n]",
                    "drop the last n messages from the branch (data stays on disk)",
                ),
                ("/new", "start a fresh session"),
                ("/fork", "copy this session into a new one"),
                ("/sessions", "list sessions in this workspace"),
                ("/quit", "leave"),
            ] {
                renderer.notice(&format!("{name:<20} {help}"));
            }
        }
        SlashCommand::Status => renderer.notice(&runtime.status()),
        SlashCommand::Compact => {
            runtime.compact(renderer).await?;
        }
        SlashCommand::Rewind(argument) => {
            let steps = argument
                .as_deref()
                .and_then(|raw| raw.trim().parse::<usize>().ok())
                .unwrap_or(1);
            runtime.rewind(steps, renderer).await?;
        }
        SlashCommand::New => {
            runtime.reset(renderer).await?;
        }
        SlashCommand::Fork => {
            runtime.fork(renderer).await?;
        }
        SlashCommand::Sessions => {
            let sessions = runtime.sessions();
            if sessions.is_empty() {
                renderer.notice("no sessions yet");
                return Ok(());
            }
            for session in sessions.iter().take(20) {
                renderer.notice(&format!(
                    "{} · {} entries · {}",
                    session.session_id,
                    session.entry_count,
                    session.model.as_deref().unwrap_or("(default model)")
                ));
            }
        }
        SlashCommand::Model(argument) => match argument {
            Some(model) => {
                runtime.set_model(None, &model, renderer).await?;
                renderer.notice(&format!("model → {model}"));
            }
            None => {
                let models = runtime.models(None).await?;
                renderer.notice(&format!("current: {}", runtime.info().model));
                if models.is_empty() {
                    renderer.warning("model catalog unavailable; pass /model <id>");
                } else {
                    for model in models.iter().take(20) {
                        renderer.notice(&format!(
                            "{} · {} context",
                            model.label(),
                            model
                                .context_window
                                .map(|window| window.to_string())
                                .unwrap_or_else(|| "?".into())
                        ));
                    }
                }
            }
        },
        SlashCommand::Thinking(argument) => match argument {
            Some(value) => {
                let Some(level) = ThinkingLevel::parse(&value) else {
                    renderer.warning(&format!(
                        "unknown thinking level `{value}` (minimal|low|medium|high|xhigh|max)"
                    ));
                    return Ok(());
                };
                runtime.set_thinking(level, renderer).await?;
                renderer.notice(&format!("thinking → {level}"));
            }
            None => {
                let next = runtime.spec().thinking.next();
                runtime.set_thinking(next, renderer).await?;
                renderer.notice(&format!("thinking → {next}"));
            }
        },
        SlashCommand::Approval(argument) => match argument {
            Some(value) => {
                let Some(mode) = ApprovalMode::parse(&value) else {
                    renderer.warning(&format!(
                        "unknown approval mode `{value}` (always-ask|write|yolo)"
                    ));
                    return Ok(());
                };
                runtime.set_approval_mode(mode, renderer).await?;
                renderer.notice(&format!("approval → {mode}"));
            }
            None => renderer.notice(&format!("approval mode: {}", runtime.spec().approval_mode)),
        },
        SlashCommand::Unknown(name) => {
            renderer.warning(&format!("unknown command `/{name}` — try /help"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_not_a_command() {
        assert!(parse("fix the bug").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn commands_parse_with_optional_arguments() {
        assert_eq!(parse("/quit"), Some(SlashCommand::Quit));
        assert_eq!(parse("/compact"), Some(SlashCommand::Compact));
        assert_eq!(
            parse("/model anthropic/claude"),
            Some(SlashCommand::Model(Some("anthropic/claude".into())))
        );
        assert_eq!(parse("/model"), Some(SlashCommand::Model(None)));
        assert_eq!(parse("/help"), Some(SlashCommand::Help));
    }

    #[test]
    fn aliases_and_case_are_normalized() {
        assert_eq!(parse("/EXIT"), Some(SlashCommand::Quit));
        assert_eq!(parse("/st"), Some(SlashCommand::Status));
    }

    #[test]
    fn unknown_commands_are_reported() {
        assert_eq!(parse("/nope"), Some(SlashCommand::Unknown("nope".into())));
        // 单独的 "/" 不崩、也不当 prompt。
        assert_eq!(parse("/"), Some(SlashCommand::Unknown(String::new())));
    }
}
