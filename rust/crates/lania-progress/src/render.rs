//! 进度快照到文本输出的渲染层。
//!
//! 注意这里的“render”不是终端实时刷新逻辑，而是：
//! - 给 JSON 模式输出结构化文本
//! - 给 human 模式输出一行可读摘要
//! - 给 summary 输出生成统一格式
//!
//! 真正的动态 spinner/progress bar 在 `terminal.rs`；这里更偏“静态字符串渲染”。

use indicatif::HumanDuration;

use crate::{ProgressKind, ProgressSnapshot, ProgressStatus, ProgressSummary};

pub trait ProgressRenderer {
    fn render_snapshot(&self, snapshot: &ProgressSnapshot) -> String;

    fn render_summary(&self, summary: &ProgressSummary) -> Vec<String> {
        summary
            .items
            .iter()
            .map(|snapshot| self.render_snapshot(snapshot))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct JsonProgressRenderer;

impl ProgressRenderer for JsonProgressRenderer {
    fn render_snapshot(&self, snapshot: &ProgressSnapshot) -> String {
        serde_json::to_string(snapshot).expect("progress snapshot serializes")
    }
}

#[derive(Debug, Clone)]
pub struct IndicatifProgressRenderer {
    bar_template: String,
    spinner_template: String,
    step_template: String,
}

impl Default for IndicatifProgressRenderer {
    fn default() -> Self {
        Self {
            bar_template: "{msg} [{wide_bar}] {pos}/{len} {percent}%".into(),
            spinner_template: "{spinner} {msg}".into(),
            step_template: "{msg}".into(),
        }
    }
}

impl ProgressRenderer for IndicatifProgressRenderer {
    fn render_snapshot(&self, snapshot: &ProgressSnapshot) -> String {
        let status = match snapshot.status {
            ProgressStatus::Pending => "pending",
            ProgressStatus::Running => "running",
            ProgressStatus::Completed => "completed",
            ProgressStatus::Failed => "failed",
            ProgressStatus::Cancelled => "cancelled",
        };
        let message = snapshot
            .message
            .as_deref()
            .or(snapshot.detail.as_deref())
            .unwrap_or(&snapshot.id);
        let task_link = snapshot
            .task_id
            .as_deref()
            .map(|task_id| format!(" task={task_id}"))
            .unwrap_or_default();
        let duration = snapshot
            .duration_ms()
            .map(|ms| {
                format!(
                    " duration={}",
                    HumanDuration(std::time::Duration::from_millis(ms))
                )
            })
            .unwrap_or_default();
        let rate = snapshot
            .rate_per_sec()
            .map(|rate| format!(" rate={rate:.1}/s"))
            .unwrap_or_default();
        let eta = snapshot
            .eta_ms()
            .map(|ms| {
                format!(
                    " eta={}",
                    HumanDuration(std::time::Duration::from_millis(ms))
                )
            })
            .unwrap_or_default();

        match snapshot.kind {
            ProgressKind::ProgressBar => format!(
                "{} | {} {}/{} {}% status={}{}{}{}",
                self.bar_template,
                message,
                snapshot.current,
                snapshot.total.unwrap_or(0),
                snapshot.percent().unwrap_or(0),
                status,
                task_link,
                duration,
                if rate.is_empty() && eta.is_empty() {
                    String::new()
                } else {
                    format!("{rate}{eta}")
                }
            ),
            ProgressKind::Spinner => format!(
                "{} | {} status={}{}{}",
                self.spinner_template, message, status, task_link, duration
            ),
            ProgressKind::StaticStep => format!(
                "{} | {} status={}{}{}",
                self.step_template, message, status, task_link, duration
            ),
        }
    }
}
