use std::collections::BTreeMap;

use super::*;

#[test]
fn renders_plain_and_ansi_text() {
    let text = StyledText::concat([
        StyledText::styled("Error: ", TextStyle::colored(TextColor::Red).bold()),
        StyledText::plain("missing config"),
    ]);

    let plain = PlainTextRenderer.render(&text);
    let ansi = AnsiTextRenderer.render(&text);

    assert_eq!(plain, "Error: missing config");
    assert!(ansi.contains("missing config"));
}

#[test]
fn supports_prefix_and_suffix() {
    let text = StyledText::styled("content", TextStyle::default().prefix("[").suffix("]"));
    let plain = PlainTextRenderer.render(&text);
    assert_eq!(plain, "[content]");
}

#[test]
fn supports_dim_level() {
    let text = StyledText::styled("dim text", TextStyle::default().dim_level(5));
    let ansi = AnsiTextRenderer.render(&text);
    assert!(ansi.contains("dim text"));
}

#[test]
fn supports_all_styles() {
    let text = StyledText::styled(
        "styled text",
        TextStyle::default()
            .bold()
            .italic()
            .underline()
            .strikethrough()
            .inverse(),
    );
    let ansi = AnsiTextRenderer.render(&text);
    assert!(ansi.contains("styled text"));
}

#[test]
fn translates_messages_with_replacements() {
    let i18n = I18nService::new("zh");
    i18n.load(
        "zh",
        BTreeMap::from([("help.command".into(), "命令 {name}".into())]),
    );

    assert_eq!(
        i18n.translate("help.command", [("name", "build")]),
        "命令 build"
    );
}

#[test]
fn builds_styled_localized_text() {
    let i18n = I18nService::new("en");
    i18n.load(
        "en",
        BTreeMap::from([("error.not_found".into(), "Missing {target}".into())]),
    );
    let styled = i18n.localized(
        "error.not_found",
        [("target", "package.json")],
        TextStyle::colored(TextColor::Yellow),
    );

    assert_eq!(PlainTextRenderer.render(&styled), "Missing package.json");
}

#[test]
fn defines_cli_localization_policy() {
    let policy = default_localization_policy(true, "zh");
    assert_eq!(policy.mode, LocaleMode::Multilingual);
    assert_eq!(policy.target_locales[&LocaleTarget::Help], "zh");
    assert_eq!(policy.target_locales[&LocaleTarget::Errors], "zh");
}

#[test]
fn terminal_renderer_can_disable_ansi() {
    let text = StyledText::styled("Warn", TextStyle::colored(TextColor::Yellow).bold());
    let rendered = TerminalTextRenderer::new(false).render(&text);
    assert_eq!(rendered, "Warn");
    assert!(!TerminalTextRenderer::new(false).ansi_enabled());
}
