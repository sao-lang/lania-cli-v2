//! `lan pi` 的 handler：把宿主上下文翻译成产品输入，再把结果翻译回 `CommandExecution`。
//!
//! 这里做三件产品不该知道的事：
//! 1. 从 `CommandExecutionContext` 取 argv / cwd / `custom.agent`；
//! 2. 把 `custom.agent.guard` 装进宿主门禁；
//! 3. 把宿主的审批请求接到产品的审批总线上（只在交互模式下）。

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use lania_agent::{
    run, AgentOptions, ApprovalBus, ApprovalDecision, ApprovalRequest, ApprovalSetup,
};
use lania_host::{
    approval::{ApprovalBroker, ApprovalRequest as HostRequest},
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
};
use lania_node_bridge::NodeBridgeClient;

use crate::bridge::bridge_client;
use crate::options::CliArgs;

pub struct PiCommandHandler;

#[async_trait(?Send)]
impl CommandHandler for PiCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let cwd = ctx.command().cwd.clone();
        let args = CliArgs::from_context(ctx)?;

        // 配置 → 产品选项（`custom.agent`），再让 CLI 参数覆盖。
        let config = lania_agent_core::AgentConfig::from_custom(
            &ctx.project_config()
                .map(|snapshot| snapshot.custom.clone())
                .unwrap_or_default(),
        );
        let mut options = AgentOptions::from_config(&cwd, &config);
        options.build_tool = ctx
            .project_config()
            .map(|snapshot| snapshot.build_tool.clone());
        args.apply_to(&mut options);
        options.validate()?;

        // 工具门禁：规则来自 `custom.agent.guard`，装在宿主内；
        // 命中 deny 时工具根本不会执行（Node 无法绕过）。drop 时自动摘除。
        let _guard = lania_host::guard::install(
            config
                .guard
                .iter()
                .filter_map(|rule| {
                    lania_host::guard::GuardRule::from_parts(
                        &rule.tool,
                        rule.pattern.as_deref(),
                        &rule.action,
                        rule.reason.as_deref(),
                    )
                })
                .collect(),
        );

        // 只有会进 TUI 的运行才装审批 UI；否则宿主回落到自己的终端提示。
        let approvals = if lania_agent::is_interactive(&options) {
            Some(ApprovalSetup::new())
        } else {
            None
        };
        let _approval_handle = approvals.as_ref().map(|setup| {
            lania_host::approval::install(Arc::new(HostApprovalBroker {
                bus: Arc::clone(&setup.bus),
            }))
        });

        let client = bridge_client(NodeBridgeClient::clone(ctx.node_bridge()));
        let outcome = run(options, client, approvals).await;

        match outcome {
            Ok(outcome) => Ok(ctx.complete_template_info(
                serde_json::json!({
                    "kind": "agent_session",
                    "status": "closed",
                    "sessionId": outcome.session_id,
                    "exitCode": outcome.exit_code,
                }),
                outcome.exit_code,
            )),
            Err(error) => {
                // 让 CLI 顶层打印统一格式（与其它命令一致）。
                Err(error)
            }
        }
    }
}

/// 把宿主审批请求转成产品审批请求，阻塞等待用户决定。
///
/// 调用方是宿主的 `agent.tool.exec`（bridge reader 线程），
/// 所以这里必须是**有界等待**——`ApprovalBus` 内部带超时。
struct HostApprovalBroker {
    bus: Arc<ApprovalBus>,
}

impl ApprovalBroker for HostApprovalBroker {
    fn ask(&self, request: &HostRequest) -> lania_host::approval::ApprovalDecision {
        let translated = ApprovalRequest {
            tier: request.tier.clone(),
            tool: request.tool.clone(),
            target: request.target.clone(),
            preview: request.preview.clone(),
        };
        match self.bus.ask(translated) {
            ApprovalDecision::AllowOnce => lania_host::approval::ApprovalDecision::AllowOnce,
            ApprovalDecision::AllowAlways => lania_host::approval::ApprovalDecision::AllowAlways,
            ApprovalDecision::Deny => lania_host::approval::ApprovalDecision::Deny,
        }
    }
}
