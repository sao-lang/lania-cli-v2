//! 基于 `indicatif` 的终端进度渲染器。
//!
//! `ProgressService` 只保存状态，不直接决定怎么显示；
//! 真正把状态渲染成 spinner/progress bar 的，是这个 sink。
//!
//! 这里最值得关注的是：
//! - `TerminalProgressMode` 如何决定渲染风格
//! - `bars: Arc<Mutex<HashMap<...>>>` 如何把业务 id 映射到具体终端条目
//! - `suspend/resume` 如何和 prompt/日志输出协作，避免终端互相抢占

use std::{
    collections::HashMap,
    io::IsTerminal,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    ProgressEvent, ProgressEventKind, ProgressKind, ProgressSink, ProgressSnapshot, ProgressStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalProgressMode {
    Auto,
    Spinner,
    Bar,
}

#[derive(Clone)]
pub struct IndicatifTerminalProgressSink {
    interactive: bool,
    mode: TerminalProgressMode,
    multi: indicatif::MultiProgress,
    // `bars` 记录“业务 id -> 终端进度条对象”的映射。
    // 这里必须共享，因为：
    // - ProgressService 可能从多个地方把事件推给同一个 sink；
    // - 每次事件到来都要找到对应的 bar 做增量更新；
    // - 因此需要一个可共享、可变、线程安全的映射表。
    bars: Arc<Mutex<HashMap<String, indicatif::ProgressBar>>>,
    bar_style: indicatif::ProgressStyle,
    spinner_style: indicatif::ProgressStyle,
    // `suspended` 用原子布尔值，而不是 Mutex<bool>，因为这里只需要极轻量的
    // “当前是否暂停”状态切换，不需要保护复杂数据结构。
    suspended: Arc<AtomicBool>,
}

impl std::fmt::Debug for IndicatifTerminalProgressSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndicatifTerminalProgressSink")
            .field("interactive", &self.interactive)
            .field(
                "bars",
                &self
                    .bars
                    .lock()
                    .map(|value| value.len())
                    .unwrap_or_default(),
            )
            .finish()
    }
}

impl Default for IndicatifTerminalProgressSink {
    fn default() -> Self {
        let interactive = std::io::stderr().is_terminal();
        let multi =
            indicatif::MultiProgress::with_draw_target(indicatif::ProgressDrawTarget::stderr());
        let bar_style =
            indicatif::ProgressStyle::with_template("{msg} [{wide_bar}] {pos}/{len} {percent}%")
                .unwrap();
        let spinner_style = indicatif::ProgressStyle::with_template("{spinner} {msg}").unwrap();
        Self {
            interactive,
            mode: TerminalProgressMode::Auto,
            multi,
            bars: Arc::new(Mutex::new(HashMap::new())),
            bar_style,
            spinner_style,
            suspended: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl IndicatifTerminalProgressSink {
    pub fn with_mode(mode: TerminalProgressMode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    pub fn suspend(&self) {
        if !self.interactive {
            return;
        }
        if self.suspended.swap(true, Ordering::SeqCst) {
            return;
        }
        // suspend 不是销毁 bar，而只是把 draw target 暂时隐藏。
        // 这样 prompt/log 输出结束后还能原地恢复，不需要重建整套终端 UI。
        self.multi
            .set_draw_target(indicatif::ProgressDrawTarget::hidden());
    }

    pub fn resume(&self) {
        if !self.interactive {
            return;
        }
        if !self.suspended.swap(false, Ordering::SeqCst) {
            return;
        }
        // 与 suspend 成对出现：恢复的是“渲染能力”，不是业务状态。
        self.multi
            .set_draw_target(indicatif::ProgressDrawTarget::stderr());
    }

    fn ensure_bar(&self, snapshot: &ProgressSnapshot) -> indicatif::ProgressBar {
        if let Some(existing) = self
            .bars
            .lock()
            .expect("progress sink store poisoned")
            .get(&snapshot.id)
            .cloned()
        {
            return existing;
        }

        let effective_kind = match self.mode {
            TerminalProgressMode::Spinner => ProgressKind::Spinner,
            TerminalProgressMode::Bar if snapshot.total.is_some() => ProgressKind::ProgressBar,
            TerminalProgressMode::Bar => ProgressKind::Spinner,
            TerminalProgressMode::Auto => snapshot.kind,
        };
        // `Auto` 模式尊重业务上报的 kind；
        // 手动指定 Spinner / Bar 时，则由 CLI 侧强制覆盖渲染风格。

        let bar = match effective_kind {
            ProgressKind::ProgressBar => {
                let total = snapshot.total.unwrap_or(0);
                if total > 0 {
                    let bar = indicatif::ProgressBar::new(total);
                    bar.set_style(self.bar_style.clone());
                    bar
                } else {
                    let bar = indicatif::ProgressBar::new_spinner();
                    bar.set_style(self.spinner_style.clone());
                    bar
                }
            }
            ProgressKind::Spinner | ProgressKind::StaticStep => {
                let bar = indicatif::ProgressBar::new_spinner();
                bar.set_style(self.spinner_style.clone());
                bar
            }
        };

        if matches!(
            effective_kind,
            ProgressKind::Spinner | ProgressKind::StaticStep
        ) {
            bar.enable_steady_tick(Duration::from_millis(80));
        }

        let bar = self.multi.add(bar);
        // 先挂到 MultiProgress，再写入 bars map。
        // 这样后续事件一旦马上到来，就能通过业务 id 找到已经注册好的终端对象。
        self.bars
            .lock()
            .expect("progress sink store poisoned")
            .insert(snapshot.id.clone(), bar.clone());
        bar
    }

    fn render_message(snapshot: &ProgressSnapshot) -> String {
        // message 优先表达“当前做什么”，detail 更像补充说明；
        // 两者同时存在时拼成一行，方便终端短文本展示。
        match (snapshot.message.as_deref(), snapshot.detail.as_deref()) {
            (Some(message), Some(detail)) if !detail.trim().is_empty() => {
                format!("{message} ({detail})")
            }
            (Some(message), _) => message.to_string(),
            (_, Some(detail)) => detail.to_string(),
            _ => snapshot.id.clone(),
        }
    }
}

impl ProgressSink for IndicatifTerminalProgressSink {
    fn on_event(&self, snapshot: &ProgressSnapshot, event: &ProgressEvent) {
        if !self.interactive {
            return;
        }

        // Reset is a management operation: remove any active bar without creating new UI.
        if matches!(event.kind, ProgressEventKind::Reset) {
            if let Ok(mut bars) = self.bars.lock() {
                bars.remove(&snapshot.id);
            }
            // Reset 只清理终端侧对象，不去改业务 snapshot；
            // 业务状态仍应由 ProgressService 统一维护。
            return;
        }

        let bar = self.ensure_bar(snapshot);
        if let Some(total) = snapshot.total {
            if total > 0 {
                bar.set_length(total);
            }
        }
        // 这里每次事件都按 snapshot 覆盖 bar 的当前展示状态，
        // 说明终端 sink 是“幂等渲染器”：它不依赖自己记住增量历史，只吃最新快照。
        bar.set_position(snapshot.current);
        bar.set_message(Self::render_message(snapshot));
        match snapshot.status {
            ProgressStatus::Completed => {
                // 完成用 `finish_with_message`：保留最终文案并把条目标记为成功结束。
                bar.finish_with_message(Self::render_message(snapshot));
                self.bars
                    .lock()
                    .expect("progress sink store poisoned")
                    .remove(&snapshot.id);
            }
            ProgressStatus::Failed => {
                // 失败/取消都用 `abandon_with_message`，因为它们语义上是“中途结束”，
                // 不应该伪装成正常完成的进度条。
                bar.abandon_with_message(Self::render_message(snapshot));
                self.bars
                    .lock()
                    .expect("progress sink store poisoned")
                    .remove(&snapshot.id);
            }
            ProgressStatus::Cancelled => {
                bar.abandon_with_message(Self::render_message(snapshot));
                self.bars
                    .lock()
                    .expect("progress sink store poisoned")
                    .remove(&snapshot.id);
            }
            ProgressStatus::Pending | ProgressStatus::Running => {}
        }
    }
}
