//! 人类可读输出：`system_commands` 相关展示。
//!
//! 这些输出通常来自“系统命令探测/枚举”能力（PATH、shell 内建、alias、function 等）。
//! 目标是把结构化结果用更紧凑的方式展示在终端里。

use crate::cli_text;

use super::common::human_block;

pub(super) fn render_system_commands_human(value: &serde_json::Value, locale: &str) -> String {
    // 这里默认只展示命令名列表，避免输出过多字段影响可读性。
    let names = value
        .get("commands")
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name").and_then(|name| name.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    human_block(
        cli_text(locale, "System Commands", "终端命令"),
        names.join("\n"),
    )
}

pub(super) fn render_system_command_groups_human(value: &serde_json::Value, locale: &str) -> String {
    // groups 结构会按来源分类（PATH / 内建命令 / 别名 / 函数）。
    // 人类可读输出把每个分组单独成块，并用空行分隔，方便用户扫一眼。
    let mut sections = Vec::new();
    for (key, title_en, title_zh) in [
        ("path", "PATH", "PATH"),
        ("shell_builtin", "Shell Builtins", "Shell 内建命令"),
        ("shell_alias", "Shell Aliases", "Shell 别名"),
        ("shell_function", "Shell Functions", "Shell 函数"),
    ] {
        let names = value
            .get("groups")
            .and_then(|groups| groups.get(key))
            .and_then(|items| items.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("name").and_then(|name| name.as_str()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if names.is_empty() {
            continue;
        }
        sections.push(human_block(
            cli_text(locale, title_en, title_zh),
            names.join("\n"),
        ));
    }
    sections.join("\n\n")
}

pub(super) fn render_system_command_names_human(value: &serde_json::Value, locale: &str) -> String {
    // 某些场景只返回“名字列表”，这时直接逐行输出即可。
    let names = value
        .get("names")
        .and_then(|item| item.as_array())
        .map(|items| items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    human_block(
        cli_text(locale, "Command Names", "命令名列表"),
        names.join("\n"),
    )
}

pub(super) fn render_system_command_plain_human(value: &serde_json::Value) -> String {
    // plain 模式：桥接侧已经给了最终文本（例如直接执行 `help`/`man` 的输出片段）。
    value
        .get("text")
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .to_string()
}
