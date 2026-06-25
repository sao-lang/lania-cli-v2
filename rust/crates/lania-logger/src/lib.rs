//! 结构化日志模型与内存日志服务，实现按级别和上下文记录日志。
//!
//! 主要导出：render_ascii_banner、as_str、new、compact、with_min_level、scoped。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
//! - 包含并发共享状态或消息通道
use std::sync::{Arc, Mutex};

use lania_presentation::{
    AnsiTextRenderer, I18nService, StyledText, StyledTextRenderer, TerminalTextRenderer, TextColor,
    TextStyle,
};
use serde::{Deserialize, Serialize};

pub fn render_ascii_banner(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let rendered = if let Ok(font) = figlet_rs::FIGfont::standard() {
        match font.convert(trimmed) {
            Some(figure) => figure.to_string(),
            None => trimmed.to_string(),
        }
    } else {
        trimmed.to_string()
    };
    rendered
        .lines()
        .map(|line| line.trim_end_matches(['\r', '\n']))
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub sequence: u64,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub trace_id: Option<String>,
    pub phase: Option<String>,
    pub operation: Option<String>,
}

pub trait LogRenderer: Send + Sync {
    fn render_entry(&self, entry: &LogEntry) -> String;

    fn render_entries(&self, entries: &[LogEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| self.render_entry(entry))
            .collect()
    }
}

pub trait LogSink: Send + Sync {
    fn on_entry(&self, entry: &LogEntry);
}

#[derive(Clone)]
pub struct StderrLogSink {
    // sink 不自己决定“日志长什么样”，而是依赖 renderer。
    // 这样同一个 sink 目标（stderr）可以切换不同渲染风格，
    // 比如纯文本、带颜色、带本地化文案等。
    renderer: Arc<dyn LogRenderer>,
}

impl StderrLogSink {
    pub fn new(renderer: Arc<dyn LogRenderer>) -> Self {
        Self { renderer }
    }
}

impl LogSink for StderrLogSink {
    fn on_entry(&self, entry: &LogEntry) {
        use std::io::Write;

        let rendered = self.renderer.render_entry(entry);
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "{rendered}");
        let _ = stderr.flush();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlainTextLogRenderer {
    include_context: bool,
}

impl PlainTextLogRenderer {
    pub fn new() -> Self {
        Self {
            include_context: true,
        }
    }

    pub fn compact(mut self) -> Self {
        self.include_context = false;
        self
    }
}

impl Default for PlainTextLogRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl LogRenderer for PlainTextLogRenderer {
    fn render_entry(&self, entry: &LogEntry) -> String {
        let mut output = format!(
            "[{}] {} {}",
            entry.sequence,
            entry.level.as_str().to_uppercase(),
            entry.target
        );
        if self.include_context {
            let mut context = Vec::new();
            if let Some(trace_id) = entry.trace_id.as_deref() {
                context.push(format!("trace={trace_id}"));
            }
            if let Some(phase) = entry.phase.as_deref() {
                context.push(format!("phase={phase}"));
            }
            if let Some(operation) = entry.operation.as_deref() {
                context.push(format!("op={operation}"));
            }
            if !context.is_empty() {
                output.push_str(&format!(" ({})", context.join(", ")));
            }
        }
        output.push_str(": ");
        output.push_str(&entry.message);
        output
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JsonLogRenderer;

impl LogRenderer for JsonLogRenderer {
    fn render_entry(&self, entry: &LogEntry) -> String {
        serde_json::to_string(entry).expect("log entry serializes")
    }
}

#[derive(Debug, Clone, Default)]
pub struct StyledLogRenderer {
    ansi: AnsiTextRenderer,
}

impl LogRenderer for StyledLogRenderer {
    fn render_entry(&self, entry: &LogEntry) -> String {
        let style = match entry.level {
            LogLevel::Trace => TextStyle::colored(TextColor::Cyan),
            LogLevel::Debug => TextStyle::colored(TextColor::Blue),
            LogLevel::Info => TextStyle::colored(TextColor::Green),
            LogLevel::Warn => TextStyle::colored(TextColor::Yellow).bold(),
            LogLevel::Error => TextStyle::colored(TextColor::Red).bold(),
        };
        let text = StyledText::concat([
            StyledText::styled(
                format!(
                    "[{}] {} ",
                    entry.sequence,
                    entry.level.as_str().to_uppercase()
                ),
                style,
            ),
            StyledText::plain(format!("{}: {}", entry.target, entry.message)),
        ]);
        self.ansi.render(&text)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AutoStyledLogRenderer {
    terminal: TerminalTextRenderer,
    use_timestamp: bool,
    locale: String,
}

impl AutoStyledLogRenderer {
    pub fn with_timestamps(mut self, use_timestamp: bool) -> Self {
        self.use_timestamp = use_timestamp;
        self
    }

    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = locale.into();
        self
    }
}

impl LogRenderer for AutoStyledLogRenderer {
    fn render_entry(&self, entry: &LogEntry) -> String {
        let style = match entry.level {
            LogLevel::Trace => TextStyle::colored(TextColor::Cyan),
            LogLevel::Debug => TextStyle::colored(TextColor::Blue),
            LogLevel::Info => TextStyle::colored(TextColor::Green),
            LogLevel::Warn => TextStyle::colored(TextColor::Yellow).bold(),
            LogLevel::Error => TextStyle::colored(TextColor::Red).bold(),
        };
        let timestamp = if self.use_timestamp {
            format!("[{}] ", cli_timestamp_compact())
        } else {
            String::new()
        };
        let text = StyledText::concat([
            StyledText::plain(timestamp),
            StyledText::styled(
                format!(
                    "[{}] {} ",
                    entry.sequence,
                    log_level_label(entry.level, self.locale.as_str())
                ),
                style,
            ),
            StyledText::plain(format!("{}: {}", entry.target, entry.message)),
        ]);
        self.terminal.render(&text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliMessageLevel {
    Log,
    Info,
    Warn,
    Error,
    Success,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliMessageRenderer {
    terminal: TerminalTextRenderer,
    use_timestamp: bool,
    locale: String,
}

impl CliMessageRenderer {
    pub fn new(terminal: TerminalTextRenderer) -> Self {
        Self {
            terminal,
            use_timestamp: false,
            locale: "en".into(),
        }
    }

    pub fn with_timestamps(mut self, use_timestamp: bool) -> Self {
        self.use_timestamp = use_timestamp;
        self
    }

    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = locale.into();
        self
    }

    pub fn render(&self, level: CliMessageLevel, message: impl AsRef<str>) -> String {
        let message = message.as_ref();
        let (prefix, style) = cli_level_style(level, self.locale.as_str());
        let prefix = StyledText::styled(prefix, style);
        let body = StyledText::plain(message.to_string());
        let timestamp = if self.use_timestamp {
            StyledText::plain(format!("[{}] ", cli_timestamp_compact()))
        } else {
            StyledText::default()
        };
        self.terminal
            .render(&StyledText::concat([timestamp, prefix, body]))
    }

    pub fn render_banner(&self, text: &str) -> Vec<String> {
        render_ascii_banner(text)
            .into_iter()
            .map(|line| {
                self.terminal.render(&StyledText::styled(
                    line,
                    TextStyle::colored(TextColor::Magenta).bold(),
                ))
            })
            .collect()
    }
}

impl Default for CliMessageRenderer {
    fn default() -> Self {
        Self::new(TerminalTextRenderer::default())
    }
}

/// 无第三方依赖的 ISO 8601 时间格式化函数
/// 格式：YYYY-MM-DDTHH:MM:SSZ（UTC 时区）
fn format_iso8601(epoch_secs: u64) -> String {
    const SECONDS_PER_MINUTE: u64 = 60;
    const SECONDS_PER_HOUR: u64 = 3600;
    const SECONDS_PER_DAY: u64 = 86400;
    const DAYS_PER_YEAR: u64 = 365;
    const DAYS_PER_LEAP_YEAR: u64 = 366;

    // 判断是否为闰年
    fn is_leap_year(year: u64) -> bool {
        (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
    }

    // 获取某年的天数
    fn days_in_year(year: u64) -> u64 {
        if is_leap_year(year) {
            DAYS_PER_LEAP_YEAR
        } else {
            DAYS_PER_YEAR
        }
    }

    // 每月的天数（非闰年）
    fn days_in_month(year: u64, month: u8) -> u8 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if is_leap_year(year) {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        }
    }

    let mut days = epoch_secs / SECONDS_PER_DAY;
    let secs_in_day = epoch_secs % SECONDS_PER_DAY;

    // 计算年份
    let mut year = 1970;
    while days >= days_in_year(year) {
        days -= days_in_year(year);
        year += 1;
    }

    // 计算月份
    let mut month = 1;
    while days >= days_in_month(year, month) as u64 {
        days -= days_in_month(year, month) as u64;
        month += 1;
    }

    // 计算日期
    let day = days as u8 + 1;

    // 计算时分秒
    let hours = (secs_in_day / SECONDS_PER_HOUR) as u8;
    let minutes = ((secs_in_day % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE) as u8;
    let seconds = (secs_in_day % SECONDS_PER_MINUTE) as u8;

    // 格式化为 ISO 8601
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn cli_timestamp_compact() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format_compact_timestamp(duration.as_secs()),
        Err(_) => "1970-01-01 00:00:00".into(),
    }
}

fn format_compact_timestamp(epoch_secs: u64) -> String {
    format_iso8601(epoch_secs)
        .replace('T', " ")
        .trim_end_matches('Z')
        .to_string()
}

fn cli_level_style(level: CliMessageLevel, locale: &str) -> (&'static str, TextStyle) {
    match level {
        CliMessageLevel::Log => ("", TextStyle::default()),
        CliMessageLevel::Info if locale == "zh" => ("信息: ", TextStyle::colored(TextColor::Blue)),
        CliMessageLevel::Warn if locale == "zh" => {
            ("警告: ", TextStyle::colored(TextColor::Yellow).bold())
        }
        CliMessageLevel::Error if locale == "zh" => {
            ("错误: ", TextStyle::colored(TextColor::Red).bold())
        }
        CliMessageLevel::Success if locale == "zh" => {
            ("成功: ", TextStyle::colored(TextColor::Green))
        }
        CliMessageLevel::Info => ("Info: ", TextStyle::colored(TextColor::Blue)),
        CliMessageLevel::Warn => ("Warn: ", TextStyle::colored(TextColor::Yellow).bold()),
        CliMessageLevel::Error => ("Error: ", TextStyle::colored(TextColor::Red).bold()),
        CliMessageLevel::Success => ("Success: ", TextStyle::colored(TextColor::Green)),
    }
}

fn log_level_label(level: LogLevel, locale: &str) -> &'static str {
    match (locale, level) {
        ("zh", LogLevel::Trace) => "跟踪",
        ("zh", LogLevel::Debug) => "调试",
        ("zh", LogLevel::Info) => "信息",
        ("zh", LogLevel::Warn) => "警告",
        ("zh", LogLevel::Error) => "错误",
        (_, _) => match level {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        },
    }
}

#[derive(Debug, Clone)]
pub struct LocalizedLogRenderer {
    i18n: I18nService,
}

impl LocalizedLogRenderer {
    pub fn new(i18n: I18nService) -> Self {
        Self { i18n }
    }
}

impl LogRenderer for LocalizedLogRenderer {
    fn render_entry(&self, entry: &LogEntry) -> String {
        self.i18n.translate(
            "log.entry",
            [
                ("sequence", entry.sequence.to_string()),
                ("level", entry.level.as_str().to_uppercase()),
                ("target", entry.target.clone()),
                ("message", entry.message.clone()),
            ],
        )
    }
}

#[derive(Debug, Clone)]
pub struct LoggerService {
    scope: String,
    min_level: LogLevel,
    state: Arc<Mutex<LoggerState>>,
}

#[derive(Default)]
struct LoggerState {
    next_sequence: u64,
    entries: Vec<LogEntry>,
    sinks: Vec<Arc<dyn LogSink>>,
}

impl std::fmt::Debug for LoggerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoggerState")
            .field("next_sequence", &self.next_sequence)
            .field("entries", &self.entries.len())
            .field("sinks", &self.sinks.len())
            .finish()
    }
}

impl LoggerService {
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            min_level: LogLevel::Info,
            state: Arc::new(Mutex::new(LoggerState::default())),
        }
    }

    pub fn with_min_level(mut self, min_level: LogLevel) -> Self {
        self.min_level = min_level;
        self
    }

    pub fn scoped(&self, suffix: impl Into<String>) -> Self {
        // `scoped()` 不创建新的日志存储，只是换一个 target 前缀继续共用同一份 state。
        // 因此不同子模块虽然 scope 不同，但最终仍会汇总到同一个日志时间线里。
        Self {
            scope: format!("{}.{}", self.scope, suffix.into()),
            min_level: self.min_level,
            state: Arc::clone(&self.state),
        }
    }

    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.log_with_target(level, self.scope.clone(), message);
    }

    pub fn log_with_context(
        &self,
        level: LogLevel,
        target: impl Into<String>,
        message: impl Into<String>,
        trace_id: Option<String>,
        phase: Option<String>,
        operation: Option<String>,
    ) {
        if level < self.min_level {
            return;
        }

        let (entry, sinks) = {
            let mut state = self.state.lock().expect("logger poisoned");
            state.next_sequence += 1;
            let sequence = state.next_sequence;
            let entry = LogEntry {
                sequence,
                level,
                target: target.into(),
                message: message.into(),
                trace_id,
                phase,
                operation,
            };
            state.entries.push(entry.clone());
            // 这里同样采用“锁内记账，锁外分发”：
            // - entries/sequence 属于内部状态，必须受 Mutex 保护
            // - sink 可能执行 IO、格式化、终端写入，不适合在持锁状态下运行
            (entry, state.sinks.clone())
        };

        for sink in sinks {
            sink.on_entry(&entry);
        }
    }

    pub fn log_with_target(
        &self,
        level: LogLevel,
        target: impl Into<String>,
        message: impl Into<String>,
    ) {
        if level < self.min_level {
            return;
        }

        self.log_with_context(level, target, message, None, None, None);
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.state.lock().expect("logger poisoned").entries.clone()
    }

    pub fn render<R: LogRenderer>(&self, renderer: &R) -> Vec<String> {
        // `render()` 是“按需把已记录日志渲染出来”；
        // 它和 sink 是两条独立路径：
        // - sink 偏实时推送
        // - render 偏事后查看/序列化/测试断言
        renderer.render_entries(&self.entries())
    }

    pub fn render_latest<R: LogRenderer>(&self, renderer: &R) -> Option<String> {
        self.state
            .lock()
            .expect("logger poisoned")
            .entries
            .last()
            .map(|entry| renderer.render_entry(entry))
    }

    pub fn add_sink(&self, sink: Arc<dyn LogSink>) {
        self.state.lock().expect("logger poisoned").sinks.push(sink);
    }

    pub fn sink_count(&self) -> usize {
        self.state.lock().expect("logger poisoned").sinks.len()
    }

    pub fn clear(&self) {
        self.state.lock().expect("logger poisoned").entries.clear();
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn min_level(&self) -> LogLevel {
        self.min_level
    }
}

impl Default for LoggerService {
    fn default() -> Self {
        Self::new("host")
    }
}

#[cfg(test)]
mod tests;
