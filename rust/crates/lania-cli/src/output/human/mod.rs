//! 人类可读输出渲染：把统一输出 envelope 渲染成更适合人阅读的文本。
//!
//! 渲染顺序：
//! 1) 一行命令摘要（`lan <name>` + args/options）
//! 2) 按 `result.kind` 复用专用渲染器（template/system/tool/product/config 等）
//! 3) 没有专用渲染器时只 pretty-print `result`
//!
//! 失败时只输出 `错误: <message>`；不会 dump envelope、host_state 或 exchange。
//!
//! 拆分说明：
//! - `common.rs`：渲染共用工具（pretty json、block、内部 flag 清理、本地化等）
//! - `template.rs`：template_info 专用渲染
//! - `product.rs`：product_inspect 专用渲染（比较长，单独拆出）
//! - `system.rs`：system_commands* 专用渲染
//! - `tools.rs`：tool_* 专用渲染

mod common;
mod product;
mod system;
mod template;
mod tools;

use anyhow::Result;
use serde_json::Value;

use crate::{cli_text, OutputMode, OutputProfile};

use crate::profile::localized_user_message;

use common::{
    human_block, is_config_value, is_locale_value, localize_json_strings, render_pretty_json,
    render_raw_value, should_suppress_rendered_output, strip_internal_output_flags,
};

pub(crate) fn render_output_value(mut value: Value, profile: &OutputProfile) -> Result<String> {
    if should_suppress_rendered_output(&value, profile) {
        return Ok(String::new());
    }

    // 输出前统一清理内部标记、做本地化，避免 downstream 分支漏处理。
    strip_internal_output_flags(&mut value);
    localize_json_strings(&mut value, profile.locale.as_str());

    // locale/config_value 属于“纯文本回显”类：脚本兼容要求输出裸标量。
    let payload = value.get("result").cloned().unwrap_or(Value::Null);
    if is_locale_value(&payload) || is_config_value(&payload) {
        return Ok(format!("{}\n", render_raw_value(&payload)));
    }

    let rendered = match profile.mode {
        OutputMode::Json => render_pretty_json(&value)?,
        OutputMode::Human => render_human_output_with_locale(&value, &profile.locale),
    };

    Ok(format!("{rendered}\n"))
}

fn render_human_output_with_locale(value: &Value, locale: &str) -> String {
    if value.get("kind").and_then(Value::as_str) == Some("error") {
        let message = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(|message| localized_user_message(locale, message))
            .unwrap_or_else(|| cli_text(locale, "unknown error", "未知错误").to_string());
        let line = format!("{}: {message}", cli_text(locale, "Error", "错误"));
        return match value.get("command") {
            Some(command) => human_block(&render_command_summary(Some(command)), line),
            None => line,
        };
    }

    let payload = value.get("result").cloned().unwrap_or(Value::Null);
    let kind = payload.get("kind").and_then(Value::as_str);
    let body = match kind {
        Some("system_commands") => system::render_system_commands_human(&payload, locale),
        Some("system_command_groups") => {
            system::render_system_command_groups_human(&payload, locale)
        }
        Some("system_command_names") => system::render_system_command_names_human(&payload, locale),
        Some("system_command_plain") => system::render_system_command_plain_human(&payload),
        Some("tool_run_result") => tools::render_tool_run_result_human(&payload, locale),
        Some("tool_view_result") => tools::render_tool_view_result_human(&payload, locale),
        Some("tool_open_result") => tools::render_tool_open_result_human(&payload, locale),
        Some("template_info") => template::render_template_info_human(&payload, locale),
        Some("config") => render_config_human(&payload, locale),
        Some("product_inspect") => product::render_product_inspect_human(&payload, locale),
        _ => render_pretty_json(&payload).unwrap_or_else(|_| payload.to_string()),
    };

    human_block(&render_command_summary(value.get("command")), body)
}

fn render_command_summary(command: Option<&Value>) -> String {
    let Some(command) = command else {
        return "lan".to_string();
    };
    let name = command.get("name").and_then(Value::as_str).unwrap_or("");
    let mut line = if name.is_empty() {
        "lan".to_string()
    } else {
        format!("lan {name}")
    };

    let mut positional = Vec::new();
    if let Some(args) = command.get("args").and_then(Value::as_object) {
        for (key, value) in args {
            line.push(' ');
            line.push_str(&render_argument(value));
            positional.push(key.as_str());
        }
    }
    if let Some(options) = command.get("options").and_then(Value::as_object) {
        for (key, value) in options {
            // 位置参数在解析结果里会同时出现在 args/options，跳过重复项。
            if positional.contains(&key.as_str()) {
                continue;
            }
            match value {
                Value::Bool(true) => line.push_str(&format!(" --{key}")),
                Value::Bool(false) => {}
                other => line.push_str(&format!(" --{key}={}", render_argument(other))),
            }
        }
    }

    line
}

fn render_argument(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn render_config_human(value: &Value, locale: &str) -> String {
    let config = value.get("config").cloned().unwrap_or(Value::Null);
    let title = cli_text(locale, "Global CLI Config", "全局 CLI 配置");
    human_block(
        title,
        render_pretty_json(&config).unwrap_or_else(|_| config.to_string()),
    )
}
