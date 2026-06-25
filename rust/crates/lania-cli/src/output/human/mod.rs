//! 人类可读输出渲染：把输出 JSON value 渲染成更适合人阅读的文本。
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

use crate::{cli_text, OutputMode, OutputProfile};

use crate::profile::localized_user_message;

use common::{
    human_block, is_config_value, is_locale_value, localize_json_strings, render_pretty_json,
    render_raw_value, should_suppress_rendered_output, strip_internal_output_flags,
};

pub(crate) fn render_output_value(
    mut value: serde_json::Value,
    profile: &OutputProfile,
) -> Result<String> {
    if should_suppress_rendered_output(&value, profile) {
        return Ok(String::new());
    }

    // 输出前统一清理内部标记、做本地化，避免 downstream 分支漏处理。
    // 这里做“全局预处理”，可以减少每个 kind renderer 自己重复处理一遍。
    strip_internal_output_flags(&mut value);
    localize_json_strings(&mut value, profile.locale.as_str());

    // locale/config_value 属于“纯文本回显”类，直接输出其 raw 字段即可。
    if is_locale_value(&value) || is_config_value(&value) {
        return Ok(format!("{}\n", render_raw_value(&value)));
    }

    let rendered = match profile.mode {
        OutputMode::Json => render_pretty_json(&value)?,
        OutputMode::Jsonl => {
            // JSONL 约定：每行一个 result 包裹结构（envelope），便于流式消费。
            let line = serde_json::json!({
                "kind": "result",
                "payload": value,
            });
            format!("{}\n", serde_json::to_string(&line)?)
        }
        OutputMode::Human => render_human_output_with_locale(&value, &profile.locale),
    };

    Ok(
        if matches!(profile.mode, OutputMode::Json | OutputMode::Human) {
            format!("{rendered}\n")
        } else {
            rendered
        },
    )
}

fn render_human_output_with_locale(value: &serde_json::Value, locale: &str) -> String {
    let kind = value
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("summary");

    match kind {
        "bridge" => render_bridge_human(value, locale),
        "workflow" => human_block(
            cli_text(locale, "Workflow Result", "工作流结果"),
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
        ),
        "system_commands" => system::render_system_commands_human(value, locale),
        "system_command_groups" => system::render_system_command_groups_human(value, locale),
        "system_command_names" => system::render_system_command_names_human(value, locale),
        "system_command_plain" => system::render_system_command_plain_human(value),
        "tool_run_result" => tools::render_tool_run_result_human(value, locale),
        "tool_view_result" => tools::render_tool_view_result_human(value, locale),
        "tool_open_result" => tools::render_tool_open_result_human(value, locale),
        "template_info" => template::render_template_info_human(value, locale),
        "config" => render_config_human(value, locale),
        "error" => human_block(
            &format!(
                "{} (exitCode={})",
                cli_text(locale, "Error", "错误"),
                value
                    .get("exitCode")
                    .and_then(|item| item.as_i64())
                    .unwrap_or(2)
            ),
            value
                .get("message")
                .and_then(|item| item.as_str())
                .map(|message| localized_user_message(locale, message))
                .unwrap_or_else(|| cli_text(locale, "unknown error", "未知错误").to_string()),
        ),
        _ => human_block(
            cli_text(locale, "Lania Runtime Summary", "Lania 运行时摘要"),
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
        ),
    }
}

fn render_bridge_human(value: &serde_json::Value, locale: &str) -> String {
    let exit_code = value
        .get("exit_code")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);

    // 某些桥接结果有更友好的专用渲染：
    // - 比如 product.inspect 输出适合分段展示，而不是直接 dump 大 JSON。
    if let Some(rendered) = render_known_bridge_result_human(value, locale) {
        return rendered;
    }

    // 默认：展示 result（如果存在），否则回退到 exchange。
    let result = value.get("result").or_else(|| value.get("exchange"));
    if let Some(result) = result {
        human_block(
            &format!(
                "{} (exitCode={exit_code})",
                cli_text(locale, "Bridge Command Completed", "桥接命令执行完成")
            ),
            serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string()),
        )
    } else {
        format!(
            "{} (exitCode={exit_code})",
            cli_text(locale, "Bridge Command Completed", "桥接命令执行完成")
        )
    }
}

fn render_known_bridge_result_human(value: &serde_json::Value, locale: &str) -> Option<String> {
    let result = value
        .get("exchange")
        .and_then(|exchange| exchange.get("response"))
        .and_then(|response| response.get("result"))?;
    match result.get("kind").and_then(|kind| kind.as_str()) {
        Some("product_inspect") => Some(product::render_product_inspect_human(result, locale)),
        _ => None,
    }
}

fn render_config_human(value: &serde_json::Value, locale: &str) -> String {
    let config = value
        .get("config")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let title = cli_text(locale, "Global CLI Config", "全局 CLI 配置");
    human_block(
        title,
        render_pretty_json(&config).unwrap_or_else(|_| config.to_string()),
    )
}
