//! `lan env ui` 的 TUI 主题与图标。
//!
//! 配色参考 yazi 默认暗色主题：深灰底、青色主色、黄色强调、圆角边框。

use ratatui::style::{Color, Modifier, Style};

/// 语义色板。集中定义，便于整体换肤。
pub struct Theme {
    pub border: Color,
    pub border_focus: Color,
    pub title: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    pub selection_bg: Color,
    pub status_bg: Color,
}

impl Theme {
    /// yazi 风格暗色主题。
    pub const fn yazi() -> Self {
        Self {
            border: Color::Rgb(60, 66, 78),
            border_focus: Color::Rgb(0, 175, 175),
            title: Color::Rgb(0, 175, 175),
            text: Color::Rgb(204, 204, 204),
            muted: Color::Rgb(110, 118, 130),
            accent: Color::Rgb(230, 200, 90),
            ok: Color::Rgb(120, 200, 120),
            warn: Color::Rgb(230, 175, 80),
            err: Color::Rgb(230, 110, 110),
            selection_bg: Color::Rgb(38, 46, 58),
            status_bg: Color::Rgb(30, 34, 42),
        }
    }

    pub fn border_style(&self, focused: bool) -> Style {
        let color = if focused {
            self.border_focus
        } else {
            self.border
        };
        Style::default().fg(color)
    }

    pub fn title_style(&self, focused: bool) -> Style {
        let color = if focused { self.title } else { self.muted };
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }

    pub fn selected_style(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.text)
            .add_modifier(Modifier::BOLD)
    }
}

/// 图标集。使用 Nerd Font 私有区字符；不支持的终端会显示为方框，
/// 因此提供 ascii 回退，并可通过 `LANIA_ENV_ICONS` 强制指定。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconSet {
    Nerd,
    Ascii,
}

impl IconSet {
    /// 自动探测：
    /// - `LANIA_ENV_ICONS=ascii|nerd` 显式覆盖
    /// - Windows Terminal / VS Code / iTerm 等已知支持的环境默认启用
    /// - 其余保守回退 ascii
    pub fn detect() -> Self {
        match std::env::var("LANIA_ENV_ICONS")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "ascii" | "text" | "off" | "0" => return Self::Ascii,
            "nerd" | "icons" | "1" => return Self::Nerd,
            _ => {}
        }
        let known_terminal = std::env::var("WT_SESSION").is_ok()
            || std::env::var("TERM_PROGRAM").is_ok()
            || std::env::var("TERM").is_ok_and(|term| term.contains("xterm-256color"));
        if known_terminal {
            Self::Nerd
        } else {
            Self::Ascii
        }
    }

    /// 图标集需要覆盖全部受支持语言，否则新语言会显示成通用图标。
    pub fn language(self, id: &str) -> &'static str {
        match (self, id) {
            (Self::Ascii, "node") => "◈",
            (Self::Ascii, "deno") => "◈",
            (Self::Ascii, "bun") => "◈",
            (Self::Ascii, _) => "◈",
            (Self::Nerd, "node") => "\u{e718}",
            // 官方 Devicons：Deno U+E7C0、Bun U+E76F。
            (Self::Nerd, "deno") => "\u{e7c0}",
            (Self::Nerd, "bun") => "\u{e76f}",
            (Self::Nerd, "python") => "\u{e73c}",
            (Self::Nerd, "rust") => "\u{e7a8}",
            (Self::Nerd, "go") => "\u{e626}",
            (Self::Nerd, "zig") => "\u{e6a9}",
            (Self::Nerd, "dart") => "\u{e798}",
            (Self::Nerd, "kotlin") => "\u{e634}",
            (Self::Nerd, "java") => "\u{e738}",
            (Self::Nerd, "scala") => "\u{e737}",
            (Self::Nerd, "groovy") => "\u{e775}",
            (Self::Nerd, "c") => "\u{e61e}",
            (Self::Nerd, "cpp") => "\u{e61d}",
            (Self::Nerd, "csharp") => "\u{e7b2}",
            (Self::Nerd, "fsharp") => "\u{e7a7}",
            (Self::Nerd, "swift") => "\u{e755}",
            (Self::Nerd, "ruby") => "\u{e739}",
            (Self::Nerd, "php") => "\u{e73d}",
            (Self::Nerd, "perl") => "\u{e769}",
            (Self::Nerd, "lua") => "\u{e620}",
            (Self::Nerd, "elixir") => "\u{e62d}",
            (Self::Nerd, "erlang") => "\u{e7b1}",
            (Self::Nerd, "haskell") => "\u{e777}",
            (Self::Nerd, "julia") => "\u{e624}",
            (Self::Nerd, "r") => "\u{f25d}",
            (Self::Nerd, "nim") => "\u{e677}",
            (Self::Nerd, "crystal") => "\u{e62f}",
            (Self::Nerd, "ocaml") => "\u{e67a}",
            (Self::Nerd, "clojure") => "\u{e768}",
            // 新语言没有专属 Devicons 字形，复用最接近的同族图标，
            // 保证在任何 Nerd Font 下都能正常显示（宁可重复，不要方框）。
            (Self::Nerd, "moonbit") => "\u{e795}",
            (Self::Nerd, "cangjie") => "\u{e738}",
            (Self::Nerd, "arkts") => "\u{e70f}",
            (Self::Nerd, "solidity") => "\u{e7a8}",
            (Self::Nerd, "fortran") => "\u{e61e}",
            (Self::Nerd, "d") => "\u{e7af}",
            (Self::Nerd, "v") => "\u{e624}",
            (Self::Nerd, "gleam") => "\u{e62d}",
            (Self::Nerd, "haxe") => "\u{e677}",
            (Self::Nerd, "elm") => "\u{e755}",
            (Self::Nerd, _) => "\u{f0ad}",
        }
    }

    /// 当前选中项的行首指示符（yazi 风格 `❯`）。
    pub fn pointer(self) -> &'static str {
        "❯"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_icons_avoid_private_use_area() {
        let set = IconSet::Ascii;
        for id in [
            "node", "python", "rust", "go", "dart", "kotlin", "java", "c",
        ] {
            let icon = set.language(id);
            assert!(
                !icon.chars().any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c)),
                "{id} icon should stay ascii: {icon:?}"
            );
        }
    }

    #[test]
    fn deno_and_bun_use_official_devicons() {
        // Nerd Fonts 中官方 Devicons：denojs U+E7C0、bun U+E76F。
        // 不要用其它字形替代，否则会变成 TypeScript / 通用图标。
        let set = IconSet::Nerd;
        assert_eq!(set.language("deno"), "\u{e7c0}");
        assert_eq!(set.language("bun"), "\u{e76f}");
    }

    #[test]
    fn nerd_icons_are_distinct_per_language() {
        let set = IconSet::Nerd;
        // 核心语言必须有专属图标；新兴语言允许复用同族图标
        // （很多语言在 Nerd Font 里根本没有专属字形）。
        let core = [
            "node", "deno", "bun", "python", "rust", "go", "zig", "dart", "kotlin", "java",
            "scala", "groovy", "c", "cpp", "csharp", "fsharp", "swift", "ruby", "php", "perl",
            "lua", "elixir", "erlang", "haskell", "julia", "r", "nim", "crystal", "ocaml",
            "clojure",
        ];
        let mut seen = std::collections::HashSet::new();
        for id in core {
            assert!(seen.insert(set.language(id)), "duplicate icon for {id}");
        }
    }

    /// 图标必须落在各版本 Nerd Font 都包含的区段内，否则会渲染成方框。
    ///
    /// 之前 C#/F# 用的 `\u{f81a}` 属于 Material Design Icons 扩展区，
    /// 多数 Nerd Font 发行版并不包含，终端里显示为空白——用户直接看到
    /// 「图标有问题」。
    #[test]
    fn nerd_icons_stay_within_common_nerd_font_ranges() {
        let set = IconSet::Nerd;
        let ids = [
            "node", "deno", "bun", "python", "rust", "go", "zig", "dart", "kotlin", "java",
            "scala", "groovy", "c", "cpp", "csharp", "fsharp", "swift", "ruby", "php", "perl",
            "lua", "elixir", "erlang", "haskell", "julia", "r", "nim", "crystal", "ocaml",
            "clojure", "moonbit", "cangjie", "arkts", "solidity", "fortran", "d", "v", "gleam",
            "haxe", "elm",
        ];
        for id in ids {
            let icon = set.language(id);
            assert!(
                icon.chars().all(|c| {
                    // Devicons / Font Awesome / Powerline 等标准区段。
                    ('\u{e600}'..='\u{e7ff}').contains(&c)
                        || ('\u{f000}'..='\u{f2ff}').contains(&c)
                        || ('\u{f300}'..='\u{f3ff}').contains(&c)
                }),
                "{id} icon {icon:?} is outside common Nerd Font ranges"
            );
        }
    }

    #[test]
    fn pointer_is_stable() {
        assert_eq!(IconSet::Ascii.pointer(), "❯");
    }
}
