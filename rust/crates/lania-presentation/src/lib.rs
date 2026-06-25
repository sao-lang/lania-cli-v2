//! CLI 输出模型与面向终端展示的辅助结构。
//!
//! 主要导出：colored、bold、italic、underline、dim、bg_colored、plain、styled。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
//! - 包含并发共享状态或消息通道
use std::{
    collections::BTreeMap,
    io::IsTerminal,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

// ============================================
// 类型定义（保持 API 兼容）
// ============================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextColor {
    Default,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    #[serde(rename = "rgb")]
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TextStyle {
    pub color: Option<TextColor>,
    pub bg_color: Option<TextColor>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub inverse: bool,
    pub hidden: bool,
    // 新增对旧版功能的支持
    pub dim_level: Option<u8>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

impl TextStyle {
    pub fn colored(color: TextColor) -> Self {
        Self {
            color: Some(color),
            ..Self::default()
        }
    }

    pub fn bg_colored(mut self, color: TextColor) -> Self {
        self.bg_color = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub fn dim_level(mut self, level: u8) -> Self {
        self.dim_level = Some(level);
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    pub fn inverse(mut self) -> Self {
        self.inverse = true;
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    // 转换为 anstyle 的 Style
    fn to_anstyle(&self) -> anstyle::Style {
        let mut style = anstyle::Style::new();

        // 处理 dimLevel 灰度（优先级最高）
        if let Some(level) = self.dim_level {
            let level = level.clamp(1, 10);
            let gray = 255 - ((level as u16 * 255) / 10) as u8;
            style = style.fg_color(Some(anstyle::Color::Rgb(anstyle::RgbColor(
                gray, gray, gray,
            ))));
        } else if let Some(color) = &self.color {
            style = style.fg_color(text_color_to_anstyle(color));
        }

        if let Some(bg_color) = &self.bg_color {
            style = style.bg_color(text_color_to_anstyle(bg_color));
        }

        if self.bold {
            style = style.bold();
        }
        if self.dim {
            style = style.dimmed();
        }
        if self.italic {
            style = style.italic();
        }
        if self.underline {
            style = style.underline();
        }
        if self.strikethrough {
            style = style.strikethrough();
        }
        if self.inverse {
            style = style.invert();
        }
        if self.hidden {
            style = style.hidden();
        }

        style
    }
}

// 辅助函数：TextColor 到 anstyle::Color
fn text_color_to_anstyle(color: &TextColor) -> Option<anstyle::Color> {
    match color {
        TextColor::Default => None,
        TextColor::Red => Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red)),
        TextColor::Green => Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green)),
        TextColor::Yellow => Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow)),
        TextColor::Blue => Some(anstyle::Color::Ansi(anstyle::AnsiColor::Blue)),
        TextColor::Magenta => Some(anstyle::Color::Ansi(anstyle::AnsiColor::Magenta)),
        TextColor::Cyan => Some(anstyle::Color::Ansi(anstyle::AnsiColor::Cyan)),
        TextColor::White => Some(anstyle::Color::Ansi(anstyle::AnsiColor::White)),
        TextColor::Rgb { r, g, b } => Some(anstyle::Color::Rgb(anstyle::RgbColor(*r, *g, *b))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledSegment {
    pub text: String,
    pub style: TextStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StyledText {
    pub segments: Vec<StyledSegment>,
}

impl StyledText {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            segments: vec![StyledSegment {
                text: text.into(),
                style: TextStyle::default(),
            }],
        }
    }

    pub fn styled(text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            segments: vec![StyledSegment {
                text: text.into(),
                style,
            }],
        }
    }

    pub fn push(&mut self, text: impl Into<String>, style: TextStyle) {
        self.segments.push(StyledSegment {
            text: text.into(),
            style,
        });
    }

    pub fn concat(parts: impl IntoIterator<Item = StyledText>) -> Self {
        let mut result = StyledText::default();
        for part in parts {
            result.segments.extend(part.segments);
        }
        result
    }
}

pub trait StyledTextRenderer {
    fn render(&self, text: &StyledText) -> String;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlainTextRenderer;

impl StyledTextRenderer for PlainTextRenderer {
    fn render(&self, text: &StyledText) -> String {
        text.segments
            .iter()
            .map(|segment| {
                let prefix = segment.style.prefix.as_deref().unwrap_or("");
                let suffix = segment.style.suffix.as_deref().unwrap_or("");
                format!("{prefix}{}{suffix}", segment.text)
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AnsiTextRenderer;

impl StyledTextRenderer for AnsiTextRenderer {
    fn render(&self, text: &StyledText) -> String {
        text.segments
            .iter()
            .map(|segment| {
                let prefix = segment.style.prefix.as_deref().unwrap_or("");
                let suffix = segment.style.suffix.as_deref().unwrap_or("");
                let content = format!("{prefix}{}{suffix}", segment.text);
                let style = segment.style.to_anstyle();
                format!("{style}{content}{style:#}")
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalTextRenderer {
    emit_ansi: bool,
}

impl TerminalTextRenderer {
    pub fn new(emit_ansi: bool) -> Self {
        Self { emit_ansi }
    }

    pub fn auto() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let emit_ansi = std::io::stderr().is_terminal() && !no_color;
        Self { emit_ansi }
    }

    pub fn ansi_enabled(&self) -> bool {
        self.emit_ansi
    }
}

impl Default for TerminalTextRenderer {
    fn default() -> Self {
        Self::auto()
    }
}

impl StyledTextRenderer for TerminalTextRenderer {
    fn render(&self, text: &StyledText) -> String {
        if self.emit_ansi {
            AnsiTextRenderer.render(text)
        } else {
            PlainTextRenderer.render(text)
        }
    }
}

// ============================================
// 国际化支持（保持原样）
// ============================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocaleMode {
    Monolingual,
    Multilingual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocaleTarget {
    Logs,
    Errors,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizationPolicy {
    pub mode: LocaleMode,
    pub default_locale: String,
    pub target_locales: BTreeMap<LocaleTarget, String>,
}

impl Default for LocalizationPolicy {
    fn default() -> Self {
        Self {
            mode: LocaleMode::Monolingual,
            default_locale: "en".into(),
            target_locales: BTreeMap::from([
                (LocaleTarget::Logs, "en".into()),
                (LocaleTarget::Errors, "en".into()),
                (LocaleTarget::Help, "en".into()),
            ]),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct I18nService {
    // 当前 locale 会被 CLI/profile/运行时动态切换，因此需要可变共享。
    locale: Arc<Mutex<String>>,
    // `messages` 的结构是：locale -> (key -> message)。
    // 放进 `Arc<Mutex<_>>` 后，多个 renderer/service 可以共享同一份翻译表，
    // 同时又允许在启动阶段按 locale 批量 load。
    messages: Arc<Mutex<BTreeMap<String, BTreeMap<String, String>>>>,
}

impl I18nService {
    pub fn new(default_locale: impl Into<String>) -> Self {
        Self {
            locale: Arc::new(Mutex::new(default_locale.into())),
            messages: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn load(&self, locale: impl Into<String>, messages: BTreeMap<String, String>) {
        self.messages
            .lock()
            .expect("i18n storage poisoned")
            .insert(locale.into(), messages);
    }

    pub fn set_locale(&self, locale: impl Into<String>) {
        *self.locale.lock().expect("i18n locale poisoned") = locale.into();
    }

    pub fn locale(&self) -> String {
        self.locale.lock().expect("i18n locale poisoned").clone()
    }

    pub fn translate(
        &self,
        key: &str,
        replacements: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> String {
        let locale = self.locale();
        // 查找顺序很直接：
        // 1) 先读当前 locale
        // 2) 再在该 locale 对应字典里按 key 找文案
        // 3) 找不到就退回 key 本身
        //
        // 这里退回 key 而不是报错，是为了让“缺失翻译”表现成可见的降级，而不是运行时崩溃。
        let message = self
            .messages
            .lock()
            .expect("i18n storage poisoned")
            .get(&locale)
            .and_then(|messages| messages.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string());
        replacements
            .into_iter()
            .fold(message, |acc, (name, value)| {
                acc.replace(&format!("{{{}}}", name.into()), &value.into())
            })
    }

    pub fn localized(
        &self,
        key: &str,
        replacements: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
        style: TextStyle,
    ) -> StyledText {
        StyledText::styled(self.translate(key, replacements), style)
    }
}

pub fn default_localization_policy(multilingual: bool, locale: &str) -> LocalizationPolicy {
    LocalizationPolicy {
        mode: if multilingual {
            LocaleMode::Multilingual
        } else {
            LocaleMode::Monolingual
        },
        default_locale: locale.into(),
        target_locales: BTreeMap::from([
            (LocaleTarget::Logs, locale.into()),
            (LocaleTarget::Errors, locale.into()),
            (LocaleTarget::Help, locale.into()),
        ]),
    }
}

#[cfg(test)]
mod tests;
