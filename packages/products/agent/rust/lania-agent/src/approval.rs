//! 审批通道（产品侧）。
//!
//! 谁能「问用户」，由产品定义；谁「决定能不能执行」，仍由工具执行方（宿主）决定。
//! 这样 `lania-agent` 不依赖 `lania-host`：宿主把 broker 装到自己那边，
//! 把请求转成 [`ApprovalRequest`] 丢进 [`ApprovalBus`]，由 TUI 弹窗回答。

use std::{
    sync::{mpsc, Arc},
    time::Duration,
};

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// 一次审批请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    /// `read` / `write` / `exec`
    pub tier: String,
    pub tool: String,
    /// 归一化目标：文件绝对路径 / 命令行
    pub target: String,
    pub preview: Option<String>,
}

impl ApprovalRequest {
    pub fn new(tier: impl Into<String>, tool: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            tier: tier.into(),
            tool: tool.into(),
            target: target.into(),
            preview: None,
        }
    }
}

/// 用户决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    AllowOnce,
    AllowAlways,
    Deny,
}

/// 交给 UI 的一条待审批请求（`reply` 由发起方阻塞等待）。
pub struct ApprovalPrompt {
    pub request: ApprovalRequest,
    pub reply: mpsc::Sender<ApprovalDecision>,
}

/// 审批总线：执行方 → UI。
pub struct ApprovalBus {
    tx: UnboundedSender<ApprovalPrompt>,
    timeout: Duration,
}

/// 创建总线后得到的两端：`bus` 交给执行方，`prompts` 交给 UI。
pub struct ApprovalSetup {
    pub bus: Arc<ApprovalBus>,
    pub prompts: UnboundedReceiver<ApprovalPrompt>,
}

impl ApprovalSetup {
    pub fn new() -> Self {
        let (tx, prompts) = unbounded_channel();
        let bus = Arc::new(ApprovalBus {
            tx,
            // 有界等待：调用方通常跑在 bridge 的 reader 线程上，无限等待会拖死事件流。
            timeout: Duration::from_secs(600),
        });
        Self { bus, prompts }
    }
}

impl Default for ApprovalSetup {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalBus {
    /// 阻塞询问。UI 已退出或超时均按拒绝处理（fail-closed）。
    pub fn ask(&self, request: ApprovalRequest) -> ApprovalDecision {
        let (reply, receiver) = mpsc::channel();
        if self
            .tx
            .send(ApprovalPrompt { request, reply })
            .is_err()
        {
            return ApprovalDecision::Deny;
        }
        receiver.recv_timeout(self.timeout).unwrap_or_else(|_| {
            eprintln!("[lan pi] approval timed out; denying");
            ApprovalDecision::Deny
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_roundtrips_a_decision() {
        let setup = ApprovalSetup::new();
        let bus = Arc::clone(&setup.bus);
        let mut prompts = setup.prompts;

        let handle = std::thread::spawn(move || {
            bus.ask(ApprovalRequest::new("write", "edit", "/repo/a.ts"))
        });

        let prompt = prompts.blocking_recv().expect("prompt");
        assert_eq!(prompt.request.tool, "edit");
        prompt.reply.send(ApprovalDecision::AllowAlways).unwrap();
        assert_eq!(handle.join().unwrap(), ApprovalDecision::AllowAlways);
    }

    #[test]
    fn closed_channel_denies() {
        let setup = ApprovalSetup::new();
        drop(setup.prompts);
        assert_eq!(
            setup.bus.ask(ApprovalRequest::new("exec", "exec", "ls")),
            ApprovalDecision::Deny
        );
    }
}
