//! 命令执行上下文与执行结果封装，统一 bridge 调用、超时、重试和中断处理。
//!
//! 主要导出：exit_code、from_env、new、command、has_capability、require_capability。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定

use anyhow::Result;
use async_trait::async_trait;
use lania_command::CommandContext;
use lania_node_bridge::{BridgeExchange, BridgeRequest};
use lania_workflows::WorkflowExecution;
use serde::Serialize;

use super::context::CommandExecutionContext;

pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_LINT_FAILED: i32 = 1;
pub const EXIT_RUNTIME_ERROR: i32 = 2;
pub const EXIT_TIMEOUT: i32 = 124;
pub const EXIT_CANCELLED: i32 = 130;

#[async_trait(?Send)]
pub trait CommandHandler: Send + Sync {
    // 每个命令最终都会落到一个 `CommandHandler` 上执行。
    // 这里返回 `CommandExecution`，而不是直接返回字符串/JSON，
    // 是因为宿主想统一表达“这次命令到底是 bridge 命令、workflow 命令，还是模板查询命令”。
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution>;
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeCommandRun {
    // `BridgeCommandRun` 保留了一次 bridge 调用的完整上下文：
    // - request：最初发出去的请求
    // - exchange：主调用返回的 response + events
    // - follow_up：长任务被中断后额外发出的 shutdown 请求结果
    // - interrupted：这次命令是否因为 Ctrl-C/自动中断结束
    pub request: BridgeRequest,
    pub exchange: BridgeExchange,
    pub follow_up: Option<BridgeExchange>,
    pub interrupted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandExecution {
    // 这个枚举是宿主“统一命令结果模型”的核心。
    //
    // 为什么要分成多个变体？
    // - 有些命令是 Rust -> Node bridge 的远程调用
    // - 有些命令完全在 Rust workflow 里完成
    // - 有些命令只是输出模板信息
    // 三者虽然最终都能转成 CLI 输出，但中间保留的信息并不一样。
    Bridge {
        context: CommandContext,
        request: BridgeRequest,
        exchange: BridgeExchange,
        follow_up: Option<BridgeExchange>,
        host_state: serde_json::Value,
        exit_code: i32,
    },
    Workflow {
        context: CommandContext,
        execution: WorkflowExecution,
        host_state: serde_json::Value,
        exit_code: i32,
    },
    TemplateInfo {
        context: CommandContext,
        #[serde(flatten)]
        output: serde_json::Value,
        host_state: serde_json::Value,
        exit_code: i32,
    },
}

impl CommandExecution {
    pub fn exit_code(&self) -> i32 {
        // 输出层/CLI main 只关心“最终退出码”，因此这里提供一个统一提取口。
        match self {
            Self::Bridge { exit_code, .. }
            | Self::Workflow { exit_code, .. }
            | Self::TemplateInfo { exit_code, .. } => *exit_code,
        }
    }

    pub fn with_host_state(self, host_state: serde_json::Value) -> Self {
        // `host_state` 不是命令业务结果本身，而是宿主运行时的附加快照，
        // 例如任务、进度、secret_fields、capabilities 等调试/输出信息。
        //
        // 这里返回一个新枚举值，而不是就地修改，是因为枚举变体字段不方便统一可变借用，
        // 直接 match 后重建反而最清晰。
        match self {
            Self::Bridge {
                context,
                request,
                exchange,
                follow_up,
                exit_code,
                ..
            } => Self::Bridge {
                context,
                request,
                exchange,
                follow_up,
                host_state,
                exit_code,
            },
            Self::Workflow {
                context,
                execution,
                exit_code,
                ..
            } => Self::Workflow {
                context,
                execution,
                host_state,
                exit_code,
            },
            Self::TemplateInfo {
                context,
                output,
                exit_code,
                ..
            } => Self::TemplateInfo {
                context,
                output,
                host_state,
                exit_code,
            },
        }
    }
}
