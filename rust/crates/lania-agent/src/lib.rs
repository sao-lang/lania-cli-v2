//! `lania-agent`：pi 驱动的编码 agent **产品层**。
//!
//! 依赖纪律（与 `runex` 一致）：本 crate **不依赖任何 lania 宿主 crate**
//! （`lania-host` / `lania-command` / `lania-node-bridge` 都不出现）。
//! 想把它接进 CLI，就在适配层（`lania-plugins-command-pi`）里把
//! 「传输实现」和「审批通道」注入进来。
//!
//! ```text
//! lania-agent-core            ← 引擎：类型、会话文件、压缩、prompt（零 lania 依赖）
//!   ▲
//! lania-agent（本 crate）      ← 产品：运行时 + TUI/REPL + 会话规划（零 lania 依赖）
//!   ▲
//! lania-plugins-command-pi    ← 适配：CommandSpec + 宿主 bridge + 审批/门禁接线
//!   ▲
//! lania-cli
//! ```
//!
//! 产品入口只有一个：[`run`]。它接收 [`AgentOptions`]、一个 [`AgentSessionClient`]
//! （传输）、可选的 [`ApprovalSetup`]（审批 UI 通道），返回 [`RunOutcome`]。

mod approval;
mod context;
mod event_helper;
mod options;
mod render;
mod repl;
mod runtime;
mod session_plan;
pub mod tui;

use std::{
    io::{self, IsTerminal, Read},
    sync::{Arc, Mutex},
};

use anyhow::Result;

use lania_agent_core::{
    build_system_prompt, generate_session_id, AgentSessionClient, SessionHeader, SessionSpec,
    SessionStore,
};

use crate::render::{RenderMode, Renderer};
use crate::runtime::PiRuntime;
use crate::session_plan::SessionPlan;

pub use approval::{
    ApprovalBus, ApprovalDecision, ApprovalPrompt, ApprovalRequest, ApprovalSetup,
};
pub use options::AgentOptions;
pub use lania_agent_core::{AgentEvent, Usage};
pub use runtime::{NoticeLevel, TurnReport, UiSink};

/// 产品名（TUI 标题、错误前缀都用它）。
pub const PRODUCT_NAME: &str = "lan pi";

/// 一次运行的结果。
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub exit_code: i32,
    /// 结束时（可能被 `/new`、`/fork` 改过）的会话 id。
    pub session_id: String,
    /// 是否真的落盘了会话文件。
    pub persisted: bool,
}

/// 跑一次 agent。适配层与独立二进制都走这里。
pub async fn run(
    options: AgentOptions,
    client: Box<dyn AgentSessionClient + Send>,
    approvals: Option<ApprovalSetup>,
) -> Result<RunOutcome> {
    options.validate()?;

    // `--sessions` / `--snapshot` 是纯查询：不建会话、不碰传输（client 必须是惰性的）。
    if options.list_sessions {
        let mut renderer = Renderer::new(RenderMode::Human, use_color());
        print_sessions(&SessionStore::new(&options.cwd), &mut renderer);
        return Ok(RunOutcome {
            exit_code: 0,
            session_id: String::new(),
            persisted: false,
        });
    }
    if options.snapshot {
        print!("{}", tui::snapshot_text(100, 24, false));
        println!();
        print!("{}", tui::snapshot_text(100, 24, true));
        return Ok(RunOutcome {
            exit_code: 0,
            session_id: String::new(),
            persisted: false,
        });
    }

    let plan = SessionPlan::resolve(&options)?;
    let store = if options.no_session {
        None
    } else {
        Some(SessionStore::new(&options.cwd))
    };

    let project = context::collect(&options);
    let mut spec = SessionSpec::new(plan.session_id.clone(), options.cwd.clone());
    spec.provider = options.provider.clone();
    spec.model = options.model.clone();
    spec.thinking = options.thinking;
    spec.approval_mode = options.approval_mode;
    spec.max_tokens = options.max_tokens;
    spec.system_prompt = Some(build_system_prompt(&project));
    spec.messages = plan.messages;

    if plan.is_new {
        if let Some(store) = store.as_ref() {
            let header = SessionHeader::new(&plan.session_id, &options.cwd)
                .with_model(options.model.clone())
                .with_thinking(options.thinking);
            store.create(&header)?;
        }
    }

    // 中断句柄要在 client 被移动前取出来（TUI / REPL 的 Ctrl-C 用）。
    let interruptor = client.interruptor();
    let session_cell = Arc::new(Mutex::new(plan.session_id.clone()));
    let mut runtime = PiRuntime::open(client, store, spec, options.compact_threshold)
        .await?
        .with_session_cell(Arc::clone(&session_cell));

    let interactive = is_interactive(&options);
    let exit_code = if interactive {
        let mut setup = approvals;
        let tui_config = tui::TuiConfig {
            session_cell: Arc::clone(&session_cell),
            interruptor: interruptor.clone(),
            approvals: setup.as_mut().map(|setup| &mut setup.prompts),
        };
        match tui::run(runtime, tui_config).await {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("[{PRODUCT_NAME}] tui failed: {error:#}");
                1
            }
        }
    } else {
        let watcher = spawn_interrupt_watcher(interruptor, Arc::clone(&session_cell));
        let mut renderer = Renderer::new(render_mode(&options), use_color());
        renderer.banner(runtime.info(), options.approval_mode, !plan.is_new);
        if options.verbose {
            let store = SessionStore::new(&options.cwd);
            renderer.notice(&format!(
                "session {} · file {}",
                runtime.session_id(),
                if options.no_session {
                    "(not persisted)".to_string()
                } else {
                    store
                        .layout()
                        .session_file(runtime.session_id())
                        .display()
                        .to_string()
                }
            ));
            renderer.notice(&format!(
                "thinking {} · compact at {:.0}% · {} messages restored",
                options.thinking,
                options.compact_threshold * 100.0,
                runtime.messages().map(|items| items.len()).unwrap_or(0),
            ));
        }

        let code = if options.is_one_shot() {
            run_once(&mut runtime, &options, &mut renderer).await
        } else {
            match repl::run(&mut runtime, &mut renderer).await {
                Ok(_) => 0,
                Err(error) => fail(&mut renderer, &error),
            }
        };
        watcher.abort();
        runtime.close().await;
        code
    };

    let session_id = session_cell
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();

    Ok(RunOutcome {
        exit_code,
        session_id,
        persisted: !options.no_session,
    })
}

async fn run_once<C: AgentSessionClient>(
    runtime: &mut PiRuntime<C>,
    options: &AgentOptions,
    renderer: &mut Renderer,
) -> i32 {
    let Some(prompt) = one_shot_prompt(options) else {
        renderer.error("empty prompt");
        return 1;
    };
    match runtime.run_turn(&prompt, renderer).await {
        Ok(report) => {
            // `--json`：事件流之后补一条终态，脚本不必自己聚合。
            if renderer.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "result",
                        "text": report.text,
                        "compacted": report.compacted,
                        "contextTokens": report.context_tokens,
                        "usage": {
                            "inputTokens": report.usage.input_tokens,
                            "outputTokens": report.usage.output_tokens,
                            "totalTokens": report.usage.total_tokens,
                        },
                    })
                );
            }
            if report.text.trim().is_empty() {
                renderer.warning("the model returned no text");
            }
            if report.compacted {
                renderer.notice("history was compacted; continue from the summary");
            }
            0
        }
        Err(error) => fail(renderer, &error),
    }
}

fn fail(renderer: &mut Renderer, error: &anyhow::Error) -> i32 {
    renderer.error(&format!("{error:#}"));
    1
}

fn one_shot_prompt(options: &AgentOptions) -> Option<String> {
    if let Some(prompt) = options.prompt.clone() {
        return Some(prompt);
    }
    // `-p` 且没有位置参数：从 stdin 读到底，方便管道使用。
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).ok()?;
    let trimmed = buffer.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn print_sessions(store: &SessionStore, renderer: &mut Renderer) {
    let sessions = store.list().unwrap_or_default();
    if sessions.is_empty() {
        renderer.notice("no sessions in this workspace yet");
        return;
    }
    for session in sessions {
        renderer.notice(&format!(
            "{} · {} entries · {} · {}",
            session.session_id,
            session.entry_count,
            session.model.as_deref().unwrap_or("(default model)"),
            session.cwd,
        ));
    }
}

/// Ctrl-C 只中断当前生成（行式模式；TUI 在 raw mode 下自己收按键）。
fn spawn_interrupt_watcher(
    interruptor: Option<Arc<dyn lania_agent_core::SessionInterrupt>>,
    session_cell: Arc<Mutex<String>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(interruptor) = interruptor else {
            return;
        };
        loop {
            if tokio::signal::ctrl_c().await.is_err() {
                return;
            }
            let session_id = session_cell
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            if session_id.is_empty() {
                continue;
            }
            let _ = interruptor.interrupt(&session_id).await;
        }
    })
}

fn render_mode(options: &AgentOptions) -> RenderMode {
    if options.json {
        RenderMode::Json
    } else {
        RenderMode::Human
    }
}

fn use_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("CLICOLOR_FORCE")
        .map(|value| value != "0")
        .unwrap_or(false)
    {
        return true;
    }
    io::stdout().is_terminal()
}

/// 这次运行会不会进 TUI。适配层据此决定要不要装审批 UI。
pub fn is_interactive(options: &AgentOptions) -> bool {
    !options.is_one_shot() && !options.no_tui && io::stdout().is_terminal() && io::stdin().is_terminal()
}

/// 供适配层复用的会话 id 生成（避免各自实现漂移）。
pub fn new_session_id() -> String {
    generate_session_id()
}
