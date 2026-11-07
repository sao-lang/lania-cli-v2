//! 审批代理：让「宿主决定是否放行」与「谁负责画审批界面」解耦。
//!
//! 为什么需要它（这是与 omp 的结构性差异之一）：
//! - 授权判定必须在 `agent.tool.exec` 的同一次调用内完成（防 TOCTOU），所以判定只能留在宿主；
//! - 但「问用户」这件事，行式 REPL 用 stdin 就行，全屏 TUI 必须弹一个覆盖层，
//!   两者不可能都由宿主直接做终端 I/O。
//!
//! 因此宿主只暴露一个 [`ApprovalBroker`] 插槽：命令层在进入交互前装上自己的实现，
//! 退出时由 [`ApprovalHandle`] 自动摘掉。没有装 broker 时回落到宿主自带的终端提示。
//!
//! 单例的理由：一个 `lan` 进程同时只可能有一个交互式 agent 会话（TUI 独占终端），
//! 这与 `runtime::bridges::shared_registry()` 的取舍一致。`install` 返回的 handle
//! 在 drop 时清空插槽，避免下一条命令继承上一轮的 UI。

use std::sync::{Arc, Mutex, OnceLock};

/// 一次审批请求（给 UI 渲染用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    /// `read` / `write` / `exec`
    pub tier: String,
    /// 工具名，例如 `edit` / `exec`
    pub tool: String,
    /// 归一化后的目标：文件绝对路径 / 命令行 / `git diff <path>`
    pub target: String,
    /// 可选的预览正文（diff、命令输出等），由 UI 决定是否展示
    pub preview: Option<String>,
}

impl ApprovalRequest {
    pub fn new(tier: &str, tool: &str, target: &str) -> Self {
        Self {
            tier: tier.to_string(),
            tool: tool.to_string(),
            target: target.to_string(),
            preview: None,
        }
    }

    pub fn with_preview(mut self, preview: Option<String>) -> Self {
        self.preview = preview;
        self
    }
}

/// 用户决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// 只放行这一次
    AllowOnce,
    /// 放行并记住（写 `.lan/agent/approvals.json`）
    AllowAlways,
    Deny,
}

/// 由命令层实现：阻塞式询问用户。
///
/// 实现方必须保证**有界等待**：这里被调用时，bridge 的 reader 线程正在等，
/// 无限阻塞会让心跳超时并触发 bridge 重连。
pub trait ApprovalBroker: Send + Sync {
    fn ask(&self, request: &ApprovalRequest) -> ApprovalDecision;
}

fn slot() -> &'static Mutex<Option<Arc<dyn ApprovalBroker>>> {
    static SLOT: OnceLock<Mutex<Option<Arc<dyn ApprovalBroker>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 装上 broker；返回的 handle drop 时自动摘除。
pub fn install(broker: Arc<dyn ApprovalBroker>) -> ApprovalHandle {
    *slot().lock().expect("approval slot poisoned") = Some(broker);
    ApprovalHandle { active: true }
}

/// 询问当前 broker；没有安装时返回 `None`，由调用方回落。
pub fn ask(request: &ApprovalRequest) -> Option<ApprovalDecision> {
    let broker = slot()
        .lock()
        .expect("approval slot poisoned")
        .as_ref()
        .map(Arc::clone);
    broker.map(|broker| broker.ask(request))
}

/// 当前是否装了 broker（供测试与诊断）。
pub fn is_installed() -> bool {
    slot()
        .lock()
        .expect("approval slot poisoned")
        .as_ref()
        .is_some()
}

/// RAII 句柄：drop 时清空插槽。
#[derive(Debug)]
pub struct ApprovalHandle {
    active: bool,
}

impl Drop for ApprovalHandle {
    fn drop(&mut self) {
        if self.active {
            *slot().lock().expect("approval slot poisoned") = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysAllow;
    impl ApprovalBroker for AlwaysAllow {
        fn ask(&self, _request: &ApprovalRequest) -> ApprovalDecision {
            ApprovalDecision::AllowOnce
        }
    }

    struct AlwaysDeny;
    impl ApprovalBroker for AlwaysDeny {
        fn ask(&self, _request: &ApprovalRequest) -> ApprovalDecision {
            ApprovalDecision::Deny
        }
    }

    // 单例插槽：这一组断言必须串行，用同一把锁把测试自己串起来。
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn ask_returns_none_without_broker() {
        let _guard = serial();
        assert!(ask(&ApprovalRequest::new("exec", "exec", "ls")).is_none());
        assert!(!is_installed());
    }

    #[test]
    fn install_routes_requests_and_drop_clears() {
        let _guard = serial();
        let handle = install(Arc::new(AlwaysAllow));
        assert!(is_installed());
        assert_eq!(
            ask(&ApprovalRequest::new("write", "edit", "/repo/a.ts")),
            Some(ApprovalDecision::AllowOnce)
        );
        drop(handle);
        assert!(!is_installed());
        assert!(ask(&ApprovalRequest::new("write", "edit", "/repo/a.ts")).is_none());
    }

    #[test]
    fn replacing_broker_takes_effect_immediately() {
        let _guard = serial();
        let first = install(Arc::new(AlwaysAllow));
        let second = install(Arc::new(AlwaysDeny));
        assert_eq!(
            ask(&ApprovalRequest::new("exec", "exec", "rm -rf /")),
            Some(ApprovalDecision::Deny)
        );
        drop(second);
        drop(first);
        assert!(!is_installed());
    }
}
