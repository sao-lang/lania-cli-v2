//! 人类可读输出：`tool_*` 结果展示。
//!
//! tool 类结果通常包含：元信息（路径/类型/大小/范围/exitCode）+ 内容（stdout/文件内容）。
//! 展示原则是：
//! - 优先把“怎么理解这次执行/打开/查看”的关键信息放在前面
//! - 内容本身保持原样（不要二次格式化破坏可复制性）

use crate::cli_text;

use super::common::human_block;

pub(super) fn render_tool_run_result_human(value: &serde_json::Value, locale: &str) -> String {
    // tool.run：一般是执行脚本/二进制并捕获 stdout/stderr。
    // 人类可读输出先列出 file/runtime/exitCode，再附 stdout/stderr（如果不为空）。
    let file = value.get("file").and_then(|item| item.as_str()).unwrap_or_default();
    let runtime = value
        .get("runtime")
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    let exit_code = value
        .get("exitCode")
        .and_then(|item| item.as_i64())
        .unwrap_or_default();
    let stdout = value
        .get("stdout")
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    let stderr = value
        .get("stderr")
        .and_then(|item| item.as_str())
        .unwrap_or_default();

    let mut lines = vec![
        format!("{}: {file}", cli_text(locale, "File", "文件")),
        format!("{}: {runtime}", cli_text(locale, "Runtime", "运行时")),
        format!("{}: {exit_code}", cli_text(locale, "Exit Code", "退出码")),
    ];
    if !stdout.trim().is_empty() {
        lines.push(format!("{}\n{}", cli_text(locale, "Stdout", "标准输出"), stdout));
    }
    if !stderr.trim().is_empty() {
        lines.push(format!("{}\n{}", cli_text(locale, "Stderr", "标准错误"), stderr));
    }
    human_block(cli_text(locale, "Tool Run Result", "文件运行结果"), lines.join("\n"))
}

pub(super) fn render_tool_view_result_human(value: &serde_json::Value, locale: &str) -> String {
    // tool.view：查看文件内容（可能是文本、也可能是二进制摘要）。
    // 注意：content 可能很长，truncated=true 代表被行数限制截断。
    let path = value.get("path").and_then(|item| item.as_str()).unwrap_or_default();
    let content = value
        .get("content")
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    let truncated = value
        .get("truncated")
        .and_then(|item| item.as_bool())
        .unwrap_or(false);
    let start_line = value
        .get("startLine")
        .and_then(|item| item.as_u64())
        .unwrap_or_default();
    let end_line = value.get("endLine").and_then(|item| item.as_u64()).unwrap_or_default();
    let size_bytes = value
        .get("sizeBytes")
        .and_then(|item| item.as_u64())
        .unwrap_or_default();
    let mode = value.get("mode").and_then(|item| item.as_str()).unwrap_or("text");
    let media_type = value
        .get("mediaType")
        .and_then(|item| item.as_str())
        .unwrap_or("text");

    let mut lines = vec![
        format!("{}: {path}", cli_text(locale, "Path", "路径")),
        format!("{}: {mode}", cli_text(locale, "Mode", "模式")),
        format!("{}: {media_type}", cli_text(locale, "Media Type", "媒体类型")),
        format!("{}: {size_bytes}", cli_text(locale, "Size Bytes", "字节大小")),
    ];
    if start_line > 0 && end_line > 0 {
        lines.push(format!(
            "{}: {}-{}",
            cli_text(locale, "Line Range", "行范围"),
            start_line,
            end_line
        ));
    }
    lines.push(content.to_string());
    if truncated {
        // 这里追加提示语而不是截断内容本身，避免影响用户复制前面部分内容。
        lines.push(
            cli_text(
                locale,
                "(output truncated by line limit)",
                "（输出已按行数限制截断）",
            )
            .to_string(),
        );
    }
    human_block(cli_text(locale, "File Content", "文件内容"), lines.join("\n"))
}

pub(super) fn render_tool_open_result_human(value: &serde_json::Value, locale: &str) -> String {
    // tool.open：调用系统默认查看器打开文件（macOS: open / Windows: start / Linux: xdg-open）。
    // human 输出同时承担“打开成功与否”的反馈 + 文件元信息摘要。
    let path = value.get("path").and_then(|item| item.as_str()).unwrap_or_default();
    let viewer = value
        .get("viewer")
        .and_then(|item| item.as_str())
        .unwrap_or("system_default");
    let exit_code = value
        .get("exitCode")
        .and_then(|item| item.as_i64())
        .unwrap_or_default();
    let media_type = value
        .get("mediaType")
        .and_then(|item| item.as_str())
        .unwrap_or("binary");
    let size_bytes = value
        .get("sizeBytes")
        .and_then(|item| item.as_u64())
        .unwrap_or_default();
    let opened = value
        .get("opened")
        .and_then(|item| item.as_bool())
        .unwrap_or(false);
    let title = if opened {
        cli_text(locale, "Opened With System Viewer", "已调用系统查看器")
    } else {
        cli_text(locale, "File Metadata", "文件元信息")
    };
    human_block(
        title,
        format!(
            "{}: {path}\n{}: {media_type}\n{}: {size_bytes}\n{}: {viewer}\n{}: {exit_code}",
            cli_text(locale, "Path", "路径"),
            cli_text(locale, "Media Type", "媒体类型"),
            cli_text(locale, "Size Bytes", "字节大小"),
            cli_text(locale, "Viewer", "查看器"),
            cli_text(locale, "Exit Code", "退出码"),
        ),
    )
}
