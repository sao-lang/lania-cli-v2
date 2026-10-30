//! `runex` CLI / TUI 的人类语言（locale）。
//!
//! 只做 **en / zh** 两种；未识别的值回落到英文。读取顺序：
//! `RUNEX_LOCALE` → `LC_ALL` → `LC_MESSAGES` → `LANG` → `LANGUAGE`。
//!
//! 这不是运行时语言（node/python/...）的列表，后者见 `runex::api`。

use std::ffi::OsString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Zh,
}

impl Locale {
    pub fn parse(value: &str) -> Self {
        let value = value.trim().to_ascii_lowercase();
        if value.starts_with("zh") {
            Self::Zh
        } else {
            Self::En
        }
    }

    /// 按与 lania / 终端约定一致的顺序探测。
    pub fn detect() -> Self {
        for key in ["RUNEX_LOCALE", "LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Some(value) = std::env::var_os(key) {
                let value = value.to_string_lossy();
                if !value.trim().is_empty() {
                    return Self::parse(&value);
                }
            }
        }
        Self::En
    }

    /// 从 `--locale` 或环境变量得到最终 locale。
    pub fn resolve(explicit: Option<&str>) -> Self {
        explicit.map(Self::parse).unwrap_or_else(Self::detect)
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
        }
    }

    pub fn is_zh(self) -> bool {
        self == Self::Zh
    }

    /// 选择当前 locale 的文案。
    pub fn tr(self, en: &'static str, zh: &'static str) -> &'static str {
        if self.is_zh() {
            zh
        } else {
            en
        }
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::En
    }
}

/// 供需要把 locale 交给子进程/命令行的场景使用。
pub fn locale_from_env() -> OsString {
    OsString::from(Locale::detect().code())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_locale_shapes() {
        assert_eq!(Locale::parse("zh-CN"), Locale::Zh);
        assert_eq!(Locale::parse("zh_CN.UTF-8"), Locale::Zh);
        assert_eq!(Locale::parse("en-US"), Locale::En);
        assert_eq!(Locale::parse("C"), Locale::En);
    }

    #[test]
    fn falls_back_to_english() {
        assert_eq!(Locale::En.tr("hello", "你好"), "hello");
        assert_eq!(Locale::Zh.tr("hello", "你好"), "你好");
    }
}
