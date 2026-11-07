//! 人类可读输出：`template_info` 的专用渲染。
//!
//! template_info 是“帮助/介绍类”输出：用户通常希望看到少量结构化信息，而不是一整段 JSON。
//! 该渲染器会：
//! - 根据 locale 输出中英文 label
//! - 对常见模板提供 summary/highlights（更像产品文案）
//! - 输出“可用模板列表 / 详情命令”等 next action

pub(super) fn render_template_info_human(value: &serde_json::Value, locale: &str) -> String {
    // 两种形态：
    // 1) detail：存在 `template` 字段，表示正在展示某个模板的详情
    // 2) list：不存在 `template` 字段，表示展示可用模板列表
    if let Some(template) = value.get("template").and_then(|item| item.as_str()) {
        let mut lines = vec![format!(
            "{}: {template}",
            template_info_label("template", locale)
        )];
        lines.push(format!(
            "{}: {}",
            template_info_label("summary", locale),
            template_info_summary(template, locale)
        ));
        let highlights = template_info_highlights(template, locale);
        if !highlights.is_empty() {
            lines.push(template_info_label("highlights", locale).to_string());
            for item in highlights {
                lines.push(format!("- {item}"));
            }
        }
        if let Some(templates) = template_names(value.get("availableTemplates")) {
            lines.push(format!(
                "{}: {}",
                template_info_label("availableTemplates", locale),
                templates.join(", ")
            ));
        }
        return lines.join("\n");
    }

    let mut lines = vec![template_info_label("availableTemplates", locale).to_string()];
    if let Some(templates) = template_names(value.get("templates")) {
        // list 模式下：逐行输出，方便复制/筛选。
        for template in templates {
            lines.push(format!("- {template}"));
        }
    }
    if let Some(detail) = value
        .get("usage")
        .and_then(|item| item.get("detail"))
        .and_then(|item| item.as_str())
    {
        lines.push(format!(
            "{}: {detail}",
            template_info_label("detail", locale)
        ));
    }
    if let Some(legacy) = value
        .get("usage")
        .and_then(|item| item.get("legacyInfo"))
        .and_then(|item| item.as_str())
    {
        lines.push(format!(
            "{}: {legacy}",
            template_info_label("legacyDetail", locale)
        ));
    }
    lines.join("\n")
}

fn template_info_is_zh(locale: &str) -> bool {
    locale == "zh"
}

fn template_info_label(key: &str, locale: &str) -> &'static str {
    match (template_info_is_zh(locale), key) {
        (true, "template") => "模板",
        (true, "availableTemplates") => "可用模板",
        (true, "detail") => "详情命令",
        (true, "legacyDetail") => "旧版详情命令",
        (true, "summary") => "简介",
        (true, "highlights") => "关键特征",
        (_, "template") => "Template",
        (_, "availableTemplates") => "Available Templates",
        (_, "detail") => "Detail",
        (_, "legacyDetail") => "Legacy Detail",
        (_, "summary") => "Summary",
        (_, "highlights") => "Highlights",
        _ => "Unknown",
    }
}

fn template_info_summary(name: &str, locale: &str) -> String {
    // 与 `lan create template` 共用一份数据（cli-kit::template_catalog）。
    crate::template_catalog::template_summary(name, locale)
}

fn template_info_highlights(name: &str, locale: &str) -> Vec<String> {
    crate::template_catalog::template_highlights(name, locale)
}

fn template_names(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    Some(
        value?
            .as_array()?
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect(),
    )
}
