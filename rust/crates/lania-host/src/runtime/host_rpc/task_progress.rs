use anyhow::{anyhow, Result};
use lania_progress::{
    IndicatifProgressRenderer, JsonProgressRenderer, ProgressKind, ProgressSummary,
};
use lania_task::{TaskEvent, TaskEventKind, TaskPriority, TaskRecord};
use serde_json::{json, Value};

use super::{payload_required_str, HostPayload, HostRpcAdapter, HostRpcResponse};

/// task 与 progress 的 handler 放在同一个模块里，是因为它们有相同的运行时特征：
/// - 都会修改“短生命周期的编排状态”（任务/进度条状态）
/// - 都需要把快照序列化回 node-bridge，供 JS 侧展示/聚合
///
/// 把这两类 handler 放在一起，可以在不改变任何 RPC surface 的前提下，显著缩短 `host_rpc.rs`。
pub(super) fn handle_tasks_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<HostRpcResponse> {
    match method {
        "host.tasks.register" => {
            let id = payload_required_str(payload, "id", method)?;
            let title = payload_required_str(payload, "title", method)?;
            let group = payload
                .get("group")
                .and_then(Value::as_str)
                .unwrap_or("default");
            let priority = match payload
                .get("priority")
                .and_then(Value::as_str)
                .unwrap_or("medium")
            {
                "high" => TaskPriority::High,
                "low" => TaskPriority::Low,
                _ => TaskPriority::Medium,
            };
            adapter.tasks.register(&id, &title, group, priority);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.tasks.start" => {
            let id = payload_required_str(payload, "id", method)?;
            let title = payload.get("title").and_then(Value::as_str).unwrap_or(&id);
            adapter.tasks.start(&id, title);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.tasks.update" => {
            let id = payload_required_str(payload, "id", method)?;
            let detail = payload
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or_default();
            adapter.tasks.update(&id, detail);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.tasks.complete" => {
            let id = payload_required_str(payload, "id", method)?;
            let detail = payload
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("done");
            adapter.tasks.complete(&id, detail);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.tasks.fail" => {
            let id = payload_required_str(payload, "id", method)?;
            let detail = payload
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("failed");
            adapter.tasks.fail(&id, detail);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.tasks.cancel" => {
            let id = payload_required_str(payload, "id", method)?;
            let detail = payload
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("cancelled");
            adapter.tasks.cancel(&id, detail);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.tasks.snapshot" => {
            let items = adapter
                .tasks
                .snapshot()
                .into_iter()
                .map(task_record_to_json)
                .collect::<Vec<_>>();
            Ok((json!({ "tasks": items }), Vec::new()))
        }
        "host.tasks.events" => {
            let events = adapter
                .tasks
                .events()
                .into_iter()
                .map(task_event_to_json)
                .collect::<Vec<_>>();
            Ok((json!({ "events": events }), Vec::new()))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}

pub(super) fn handle_progress_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<HostRpcResponse> {
    match method {
        "host.progress.begin" => {
            let id = payload_required_str(payload, "id", method)?;
            let total = payload.get("total").and_then(Value::as_u64);
            adapter.progress.begin(id, total);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.beginGroup" => {
            let id = payload_required_str(payload, "id", method)?;
            let total = payload.get("total").and_then(Value::as_u64);
            let kind = parse_progress_kind(payload.get("kind").and_then(Value::as_str));
            adapter.progress.begin_group(id, total, kind);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.beginStep" => {
            let id = payload_required_str(payload, "id", method)?;
            let parent_id = payload_required_str(payload, "parentId", method)?;
            let total = payload.get("total").and_then(Value::as_u64);
            let kind = parse_progress_kind(payload.get("kind").and_then(Value::as_str));
            adapter.progress.begin_step(id, parent_id, total, kind);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.beginItem" => {
            let id = payload_required_str(payload, "id", method)?;
            let parent_id = payload_required_str(payload, "parentId", method)?;
            let kind = parse_progress_kind(payload.get("kind").and_then(Value::as_str));
            adapter.progress.begin_item(id, parent_id, None, kind);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.advance" => {
            let id = payload_required_str(payload, "id", method)?;
            let delta = payload.get("delta").and_then(Value::as_u64).unwrap_or(1);
            adapter.progress.advance(&id, delta);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.updateTotal" => {
            let id = payload_required_str(payload, "id", method)?;
            let total = payload.get("total").and_then(Value::as_u64);
            adapter.progress.update_total(&id, total);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.message" => {
            let id = payload_required_str(payload, "id", method)?;
            let message = payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            adapter.progress.message(&id, message);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.detail" => {
            let id = payload_required_str(payload, "id", method)?;
            let detail = payload
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or_default();
            adapter.progress.detail(&id, detail);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.linkTask" => {
            let id = payload_required_str(payload, "id", method)?;
            let task_id = payload_required_str(payload, "taskId", method)?;
            adapter.progress.link_task(&id, task_id);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.finish" => {
            let id = payload_required_str(payload, "id", method)?;
            adapter.progress.finish(&id);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.fail" => {
            let id = payload_required_str(payload, "id", method)?;
            let detail = payload
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("failed");
            adapter.progress.fail(&id, detail);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.cancel" => {
            let id = payload_required_str(payload, "id", method)?;
            let detail = payload
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("cancelled");
            adapter.progress.cancel(&id, detail);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.reset" => {
            let id = payload_required_str(payload, "id", method)?;
            adapter.progress.reset(&id);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.resetAll" => {
            adapter.progress.reset_all();
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.completeAll" => {
            adapter.progress.complete_all();
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.failAll" => {
            let detail = payload
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("failed");
            adapter.progress.fail_all(detail);
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.snapshot" => {
            Ok((json!({ "items": adapter.progress.snapshot() }), Vec::new()))
        }
        "host.progress.events" => Ok((json!({ "events": adapter.progress.events() }), Vec::new())),
        "host.progress.summary" => {
            let summary: ProgressSummary = adapter.progress.summary();
            Ok((serde_json::to_value(summary)?, Vec::new()))
        }
        "host.progress.render" => {
            let mode = payload
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("indicatif");
            let lines = match mode {
                "json" => adapter.progress.render(&JsonProgressRenderer),
                _ => adapter
                    .progress
                    .render(&IndicatifProgressRenderer::default()),
            };
            Ok((json!({ "lines": lines }), Vec::new()))
        }
        "host.progress.contains" => {
            let id = payload_required_str(payload, "id", method)?;
            Ok((
                json!({ "contains": adapter.progress.contains(&id) }),
                Vec::new(),
            ))
        }
        "host.progress.suspendTerminal" => {
            adapter.progress.suspend_terminal();
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.progress.resumeTerminal" => {
            adapter.progress.resume_terminal();
            Ok((json!({ "ok": true }), Vec::new()))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}

fn parse_progress_kind(kind: Option<&str>) -> ProgressKind {
    match kind.unwrap_or("spinner") {
        "progress_bar" | "progressBar" => ProgressKind::ProgressBar,
        "static_step" | "staticStep" => ProgressKind::StaticStep,
        _ => ProgressKind::Spinner,
    }
}

fn task_record_to_json(record: TaskRecord) -> Value {
    json!({
        "id": record.id,
        "title": record.title,
        "group": record.group,
        "priority": match record.priority {
            TaskPriority::High => "high",
            TaskPriority::Medium => "medium",
            TaskPriority::Low => "low",
        },
        "state": match record.state {
            lania_task::TaskState::Pending => "pending",
            lania_task::TaskState::Running => "running",
            lania_task::TaskState::Completed => "completed",
            lania_task::TaskState::Failed => "failed",
            lania_task::TaskState::Cancelled => "cancelled",
        },
        "detail": record.detail,
        "attempts": record.attempts,
    })
}

fn task_event_to_json(event: TaskEvent) -> Value {
    json!({
        "sequence": event.sequence,
        "taskId": event.task_id,
        "kind": match event.kind {
            TaskEventKind::Registered => "registered",
            TaskEventKind::Started => "started",
            TaskEventKind::Updated => "updated",
            TaskEventKind::Completed => "completed",
            TaskEventKind::Failed => "failed",
            TaskEventKind::Cancelled => "cancelled",
            TaskEventKind::RollbackStarted => "rollback_started",
            TaskEventKind::RollbackCompleted => "rollback_completed",
            TaskEventKind::RollbackFailed => "rollback_failed",
        },
        "detail": event.detail,
    })
}
