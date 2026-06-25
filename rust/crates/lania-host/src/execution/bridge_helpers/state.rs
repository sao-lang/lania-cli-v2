//! 执行结果附带的 `host_state` 打包逻辑。

use lania_logger::LogLevel;

use super::super::{context::CommandExecutionContext, utils::redact_secret_fields};

impl<'a> CommandExecutionContext<'a> {
    pub fn host_state(&self) -> serde_json::Value {
        let secret_fields = self.prompt.secret_fields();
        // `host_state` 是“执行期快照打包器”：
        // 它把策略、日志、进度、任务、hook 事件统一收集起来，作为命令结果的附加上下文。
        serde_json::json!({
            "policy": {
                "timeoutMs": self.policy.timeout_ms,
                "retryAttempts": self.policy.retry_attempts,
                "autoInterruptAfterMs": self.policy.auto_interrupt_after_ms,
            },
            "logs": self.logger.entries().into_iter().map(|entry| {
                serde_json::json!({
                    "sequence": entry.sequence,
                    "level": match entry.level {
                        LogLevel::Trace => "trace",
                        LogLevel::Debug => "debug",
                        LogLevel::Info => "info",
                        LogLevel::Warn => "warn",
                        LogLevel::Error => "error",
                    },
                    "target": entry.target,
                    "message": entry.message,
                    "traceId": entry.trace_id,
                    "phase": entry.phase,
                    "operation": entry.operation,
                })
            }).collect::<Vec<_>>(),
            "progress": self.progress.snapshot().into_iter().map(|item| {
                serde_json::json!({
                    "id": item.id,
                    "current": item.current,
                    "total": item.total,
                    "message": item.message,
                    "percent": item.percent(),
                })
            }).collect::<Vec<_>>(),
            "tasks": self.tasks.snapshot().into_iter().map(|task| {
                serde_json::json!({
                    "id": task.id,
                    "title": task.title,
                    "state": format!("{:?}", task.state).to_lowercase(),
                    "detail": task.detail,
                })
            }).collect::<Vec<_>>(),
            "hooks": self.hooks.snapshot().events.into_iter().map(|event| {
                serde_json::json!({
                    "sequence": event.sequence,
                    "name": event.key,
                    "source": event.source,
                    "payload": redact_secret_fields(event.payload, &secret_fields),
                })
            }).collect::<Vec<_>>(),
        })
    }
}
