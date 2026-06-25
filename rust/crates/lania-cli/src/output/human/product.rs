//! 人类可读输出：`product_inspect` 的专用渲染。
//!
//! 为什么需要专用渲染：
//! - product.inspect 的 payload 字段很多（config/schema/artifacts/compat/nextSteps...）
//! - 用户更关心“结论 + 下一步 + 哪些检查没通过”，而不是完整 JSON
//!
//! 渲染策略：
//! - 按段落输出（overview/compat/schema/artifacts/checks/warnings/next_steps）
//! - 每段落内部用 markdown-like 列表（`-` / `  -`）让终端可读、可复制

pub(super) fn render_product_inspect_human(value: &serde_json::Value, locale: &str) -> String {
    // 这块展示逻辑比较长，但核心思路是：
    // - 先做几个关键段落（overview/compat/schema/artifacts/...）
    // - 再把每个段落展开成 markdown-like 的列表，便于用户快速扫描
    let is_zh = locale == "zh";
    let mut lines = Vec::new();

    // doctor 模式会更强调“诊断”，并且通常默认包含 compat 检查。
    let doctor_mode = value
        .get("doctor")
        .and_then(|item| item.as_bool())
        .unwrap_or(false);
    lines.push(if doctor_mode {
        if is_zh {
            "产品 Doctor 诊断".to_string()
        } else {
            "Product Doctor Diagnostics".to_string()
        }
    } else if is_zh {
        "产品开发诊断".to_string()
    } else {
        "Product Development Diagnostics".to_string()
    });

    // mode 是 runtime 的视角：development / installed。
    let mode = value
        .get("mode")
        .and_then(|item| item.as_str())
        .unwrap_or("development");
    let config_path = value
        .get("configPath")
        .and_then(|item| item.as_str())
        .unwrap_or("<unknown>");
    // 为了让渲染逻辑更稳健：字段缺失时用 Null 兜底，避免 panic。
    let product = value.get("product").unwrap_or(&serde_json::Value::Null);
    let schema = value.get("schema").unwrap_or(&serde_json::Value::Null);
    let artifacts = value.get("artifacts").unwrap_or(&serde_json::Value::Null);
    let checks = value.get("checks").unwrap_or(&serde_json::Value::Null);
    let next_steps = value.get("nextSteps").unwrap_or(&serde_json::Value::Null);
    let compat = value.get("compat").unwrap_or(&serde_json::Value::Null);

    lines.push(section_title(locale, "overview"));
    lines.push(format!(
        "- {}: {}",
        locale_label(is_zh, "模式", "Mode"),
        mode
    ));
    lines.push(format!(
        "- {}: {}",
        locale_label(is_zh, "配置", "Config"),
        config_path
    ));
    if let Some(name) = product.get("name").and_then(|item| item.as_str()) {
        lines.push(format!(
            "- {}: {}",
            locale_label(is_zh, "产品包名", "Product Package"),
            name
        ));
    }
    if let Some(binary_name) = product.get("binaryName").and_then(|item| item.as_str()) {
        lines.push(format!(
            "- {}: {}",
            locale_label(is_zh, "命令名", "Binary"),
            binary_name
        ));
    }
    if let Some(templates_dir) = product.get("templatesDir").and_then(|item| item.as_str()) {
        lines.push(format!(
            "- {}: {}",
            locale_label(is_zh, "模板目录", "Templates Dir"),
            templates_dir
        ));
    }

    if compat.is_object() {
        // compat 段落：把“声明的 range”与“实际版本”并排展示，方便定位不兼容原因。
        lines.push(section_title(locale, "compat"));
        if let Some(verdict) = compat.get("verdict").and_then(|item| item.as_str()) {
            lines.push(format!(
                "- {}: {}",
                locale_label(is_zh, "状态", "Verdict"),
                verdict
            ));
        }
        if let Some(report_path) = compat.get("reportPath").and_then(|item| item.as_str()) {
            lines.push(format!(
                "- {}: {}",
                locale_label(is_zh, "报告", "Report"),
                report_path
            ));
        }
        if let Some(declared) = compat.get("declared").and_then(|item| item.as_object()) {
            let declared_items = [
                ("frameworkVersionRange", "Framework Range", "框架范围"),
                ("protocolVersionRange", "Protocol Range", "协议范围"),
                ("nodeBridgeVersionRange", "Node Bridge Range", "Node Bridge 范围"),
                ("productVersionRange", "Product Range", "产品范围"),
            ];
            let mut printed_declared = false;
            for (key, en_label, zh_label) in declared_items {
                if let Some(value) = declared.get(key).and_then(|item| item.as_str()) {
                    if !printed_declared {
                        lines.push(format!(
                            "- {}:",
                            locale_label(is_zh, "声明兼容范围", "Declared Ranges")
                        ));
                        printed_declared = true;
                    }
                    // 二级列表用于保持层级结构，但仍然是纯文本，可复制到 issue。
                    lines.push(format!(
                        "  - {}: {}",
                        locale_label(is_zh, zh_label, en_label),
                        value
                    ));
                }
            }
        }
        if let Some(actual) = compat.get("actual").and_then(|item| item.as_object()) {
            let actual_items = [
                ("hostVersion", "Host Version", "宿主版本"),
                ("protocolVersion", "Protocol Version", "协议版本"),
                ("nodeBridgeVersion", "Node Bridge Version", "Node Bridge 版本"),
            ];
            let mut printed_actual = false;
            for (key, en_label, zh_label) in actual_items {
                if let Some(value) = actual.get(key).and_then(|item| item.as_str()) {
                    if !printed_actual {
                        lines.push(format!(
                            "- {}:",
                            locale_label(is_zh, "实际版本", "Actual Versions")
                        ));
                        printed_actual = true;
                    }
                    lines.push(format!(
                        "  - {}: {}",
                        locale_label(is_zh, zh_label, en_label),
                        value
                    ));
                }
            }
        }
        if let Some(reasons) = compat.get("reasons").and_then(|item| item.as_array()) {
            let reasons = reasons
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>();
            if !reasons.is_empty() {
                lines.push(format!("- {}:", locale_label(is_zh, "提示", "Notes")));
                for reason in reasons {
                    lines.push(format!("  - {reason}"));
                }
            }
        }
    }

    lines.push(section_title(locale, "schema"));
    if let Some(entries) = schema.get("entries").and_then(|item| item.as_array()) {
        lines.push(format!(
            "- {}:",
            locale_label(is_zh, "Schema 入口", "Schema Entries")
        ));
        for entry in entries.iter().filter_map(|item| item.as_str()) {
            lines.push(format!("  - {entry}"));
        }
    }
    if let Some(roots) = schema.get("roots").and_then(|item| item.as_array()) {
        let roots = roots
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>();
        if !roots.is_empty() {
            lines.push("- Schema Roots:".to_string());
            for root in roots {
                lines.push(format!("  - {root}"));
            }
        }
    }

    lines.push(section_title(locale, "artifacts"));
    for (label_key, path_key, status_key) in [
        ("build", "buildDir", "hasBuildReport"),
        ("pack", "packDir", "hasPackReport"),
        ("publish", "publishDir", "hasPublishReport"),
    ] {
        // artifacts 段落：把“是否生成报告”与“目录路径”合并展示，节省行数。
        let path = artifacts
            .get(path_key)
            .and_then(|item| item.as_str())
            .unwrap_or("-");
        let ok = artifacts
            .get(status_key)
            .and_then(|item| item.as_bool())
            .unwrap_or(false);
        let state = if is_zh {
            if ok { "已就位" } else { "未生成" }
        } else if ok {
            "ready"
        } else {
            "missing"
        };
        lines.push(format!("- {label_key}: {state} ({path})"));
    }

    let mut check_items = Vec::new();
    for (key, label_zh, label_en) in [
        ("hasSchemaEntries", "已发现 schema 入口", "schema entries discovered"),
        ("hasTemplatesDir", "已发现模板目录", "templates directory detected"),
        ("hasBuildReport", "build report 已生成", "build report generated"),
        ("hasPackReport", "pack report 已生成", "pack report generated"),
        (
            "hasPublishReport",
            "publish report 已生成",
            "publish report generated",
        ),
    ] {
        if checks
            .get(key)
            .and_then(|item| item.as_bool())
            .unwrap_or(false)
        {
            check_items.push(if is_zh { label_zh } else { label_en });
        }
    }
    if !check_items.is_empty() {
        lines.push(section_title(locale, "checks"));
        for item in check_items {
            lines.push(format!("- {item}"));
        }
    }

    if let Some(warnings) = schema.get("warnings").and_then(|item| item.as_array()) {
        let warnings = warnings
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>();
        if !warnings.is_empty() {
            lines.push(section_title(locale, "warnings"));
            for warning in warnings {
                lines.push(format!("- {warning}"));
            }
        }
    }

    if let Some(items) = next_steps.as_array() {
        let items = items
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>();
        if !items.is_empty() {
            // nextSteps 是最关键的行动建议：一定放在末尾并保持短句。
            lines.push(section_title(locale, "next_steps"));
            for item in items {
                lines.push(format!("- {item}"));
            }
        }
    }

    lines.join("\n")
}

fn locale_label<'a>(is_zh: bool, zh: &'a str, en: &'a str) -> &'a str {
    if is_zh { zh } else { en }
}

fn section_title(locale: &str, key: &str) -> String {
    match (locale == "zh", key) {
        (true, "overview") => "概览".to_string(),
        (true, "compat") => "兼容性".to_string(),
        (true, "schema") => "Schema".to_string(),
        (true, "artifacts") => "产物状态".to_string(),
        (true, "checks") => "通过检查".to_string(),
        (true, "warnings") => "警告".to_string(),
        (true, "next_steps") => "下一步建议".to_string(),
        (false, "overview") => "Overview".to_string(),
        (false, "compat") => "Compatibility".to_string(),
        (false, "schema") => "Schema".to_string(),
        (false, "artifacts") => "Artifacts".to_string(),
        (false, "checks") => "Checks".to_string(),
        (false, "warnings") => "Warnings".to_string(),
        (false, "next_steps") => "Next Steps".to_string(),
        _ => key.to_string(),
    }
}
