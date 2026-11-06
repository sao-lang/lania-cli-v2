use lania_presentation::I18nService;
use serde_json::Value;

use super::{
    render_ascii_banner, AutoStyledLogRenderer, CliMessageLevel, CliMessageRenderer,
    JsonLogRenderer, LocalizedLogRenderer, LogLevel, LoggerService, PlainTextLogRenderer,
    StyledLogRenderer,
};
use lania_presentation::TerminalTextRenderer;

#[test]
fn renders_ascii_banner_lines() {
    let lines = render_ascii_banner("lania");
    assert!(!lines.is_empty());
    // Figlet output is font-dependent; just assert we rendered something meaningful.
    assert!(lines.iter().any(|line| line
        .chars()
        .any(|ch| ch.is_ascii_graphic() && !ch.is_ascii_whitespace())));
}

#[test]
fn records_messages_at_or_above_threshold() {
    let logger = LoggerService::new("lania").with_min_level(LogLevel::Info);
    logger.log(LogLevel::Debug, "skip");
    logger.log(LogLevel::Warn, "keep");

    let entries = logger.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "keep");
    assert_eq!(entries[0].target, "lania");
    assert_eq!(entries[0].sequence, 1);
}

#[test]
fn shares_sink_between_scopes() {
    let logger = LoggerService::default();
    let child = logger.scoped("dev");
    child.log(LogLevel::Info, "boot");

    let entries = logger.entries();
    assert_eq!(entries[0].target, "host.dev");
}

#[test]
fn records_context_metadata() {
    let logger = LoggerService::default().with_min_level(LogLevel::Trace);
    logger.log_with_context(
        LogLevel::Debug,
        "host.runtime",
        "debugging",
        Some("trace-1".into()),
        Some("setup".into()),
        Some("command.dev".into()),
    );

    let entries = logger.entries();
    assert_eq!(entries[0].trace_id.as_deref(), Some("trace-1"));
    assert_eq!(entries[0].phase.as_deref(), Some("setup"));
    assert_eq!(entries[0].operation.as_deref(), Some("command.dev"));
}

#[test]
fn renders_entries_for_terminal_presentation() {
    let logger = LoggerService::default().with_min_level(LogLevel::Trace);
    logger.log_with_context(
        LogLevel::Info,
        "host.runtime",
        "server started",
        Some("trace-2".into()),
        Some("runtime_start".into()),
        Some("command.dev".into()),
    );

    let rendered = logger.render_latest(&PlainTextLogRenderer::default());
    assert_eq!(
        rendered.as_deref(),
        Some("[1] INFO host.runtime (trace=trace-2, phase=runtime_start, op=command.dev): server started")
    );
}

#[test]
fn renders_entries_as_json_lines() {
    let logger = LoggerService::default();
    logger.log(LogLevel::Warn, "watcher stalled");

    let rendered = logger.render(&JsonLogRenderer);
    let entry: Value = serde_json::from_str(&rendered[0]).expect("valid json log line");
    assert_eq!(entry["level"], "warn");
    assert_eq!(entry["message"], "watcher stalled");
}

#[test]
fn renders_entries_with_styled_text_renderer() {
    let logger = LoggerService::default();
    logger.log(LogLevel::Error, "boom");

    let rendered = logger.render(&StyledLogRenderer::default());
    assert!(rendered[0].contains("\u{1b}[31m"));
    assert!(rendered[0].contains("boom"));
}

#[test]
fn renders_entries_with_i18n_layer() {
    let logger = LoggerService::default();
    logger.log(LogLevel::Info, "started");

    let i18n = I18nService::new("zh");
    i18n.load(
        "zh",
        std::collections::BTreeMap::from([(
            "log.entry".into(),
            "[{sequence}] {level} {target}: {message}".into(),
        )]),
    );
    let rendered = logger.render(&LocalizedLogRenderer::new(i18n));
    assert_eq!(rendered[0], "[1] INFO host: started");
}

#[test]
fn renders_cli_messages_with_level_prefix() {
    let renderer = CliMessageRenderer::new(TerminalTextRenderer::new(false));
    assert_eq!(
        renderer.render(CliMessageLevel::Warn, "be careful"),
        "Warn: be careful"
    );
    assert_eq!(
        renderer.render(CliMessageLevel::Success, "done"),
        "Success: done"
    );
}

#[test]
fn auto_styled_renderer_can_fallback_to_plain_text() {
    let logger = LoggerService::default();
    logger.log(LogLevel::Error, "boom");

    let rendered = logger.render(&AutoStyledLogRenderer {
        terminal: TerminalTextRenderer::new(false),
        use_timestamp: false,
        locale: "en".into(),
    });
    assert_eq!(rendered[0], "[1] ERROR host: boom");
}

#[test]
fn format_iso8601_works_for_specific_dates() {
    // 测试 Unix epoch 开始
    assert_eq!(super::format_iso8601(0), "1970-01-01T00:00:00Z");

    // 测试简单日期
    assert_eq!(super::format_iso8601(3600), "1970-01-01T01:00:00Z"); // 1小时后
    assert_eq!(super::format_iso8601(86400), "1970-01-02T00:00:00Z"); // 1天后

    // 测试闰年相关
    assert_eq!(super::format_iso8601(31536000), "1971-01-01T00:00:00Z"); // 1970年（非闰年）后
                                                                         // 测试 1972 年（闰年）
                                                                         // 1970 年 + 1971 年 = 365 + 365 = 730 天
                                                                         // 730 * 86400 = 63072000 秒
                                                                         // 1972 年 1 月 1 日
    assert_eq!(super::format_iso8601(63072000), "1972-01-01T00:00:00Z");
    // 1972 年 2 月 29 日（闰年）
    // 1972-02-29 = 31（1月） + 29（2月） - 1 = 59 天
    // 59 * 86400 = 5097600 秒
    // 总计 63072000 + 5097600 = 68169600
    assert_eq!(super::format_iso8601(68169600), "1972-02-29T00:00:00Z");
}

#[test]
fn cli_message_renderer_with_timestamp() {
    let renderer = CliMessageRenderer::new(TerminalTextRenderer::new(false)).with_timestamps(true);
    let output = renderer.render(CliMessageLevel::Info, "test message");
    // 验证输出包含简洁时间戳
    assert!(output.starts_with('['));
    assert!(output.contains("] Info: test message"));
    assert!(!output.contains('T'));
}
