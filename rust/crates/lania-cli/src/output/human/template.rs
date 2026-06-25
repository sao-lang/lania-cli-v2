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
    // 对“内置模板”给一个更短、更像产品说明的简介；
    // 其它未知模板回退为原名，避免误导。
    match (template_info_is_zh(locale), name) {
        (true, "spa-react") => "基于 React 的单页应用项目模板，适合快速启动前端业务项目。".into(),
        (true, "spa-vue") => "基于 Vue 3 的单文件组件项目模板，适合快速启动前端业务项目。".into(),
        (true, "toolkit") => "面向工具库或 SDK 的单包项目模板，适合构建可复用的前端工具模块。".into(),
        (true, "toolkit-monorepo") => {
            "面向多包协作场景的 monorepo 模板，适合组件库、工具集或多模块工程。".into()
        }
        (_, "spa-react") => {
            "A React single-page application template for quickly bootstrapping frontend projects."
                .into()
        }
        (_, "spa-vue") => {
            "A Vue 3 single-file component template for quickly bootstrapping frontend projects."
                .into()
        }
        (_, "toolkit") => "A single-package toolkit template for building reusable utilities or SDKs."
            .into(),
        (_, "toolkit-monorepo") => {
            "A monorepo template for multi-package toolkits, component libraries, or modular projects."
                .into()
        }
        _ => name.to_string(),
    }
}

fn template_info_highlights(name: &str, locale: &str) -> Vec<String> {
    // highlights 是用户最关心的“这个模板能干什么”，这里保持短句且可扫描。
    match (template_info_is_zh(locale), name) {
        (true, "spa-react") => vec![
            "使用 React 组件结构，默认包含 `App` 与入口文件".into(),
            "支持 TypeScript 或 JavaScript".into(),
            "支持 Vite 或 Webpack 构建".into(),
            "可选 CSS 预处理器、Tailwind 和常见 lint 工具".into(),
        ],
        (true, "spa-vue") => vec![
            "使用 Vue 3 + 单文件组件（`App.vue`）结构".into(),
            "支持 TypeScript 或 JavaScript".into(),
            "支持 Vite 或 Webpack 构建".into(),
            "可选 CSS 预处理器、Tailwind 和常见 lint 工具".into(),
        ],
        (true, "toolkit") => vec![
            "单包目录结构，适合工具库或 SDK".into(),
            "默认基于 Vite 构建".into(),
            "支持 TypeScript，可选 Vitest".into(),
            "内置常见工程化与发布配置".into(),
        ],
        (true, "toolkit-monorepo") => vec![
            "多包目录结构，默认包含 `packages/core`".into(),
            "支持 pnpm workspace 与 changesets 发布流程".into(),
            "适合组件库、工具集或多模块协作".into(),
            "内置常见工程化与提交规范配置".into(),
        ],
        (_, "spa-react") => vec![
            "Uses a React component structure with an `App` entry".into(),
            "Supports TypeScript or JavaScript".into(),
            "Supports Vite or Webpack".into(),
            "Optional CSS preprocessors, Tailwind, and common lint tools".into(),
        ],
        (_, "spa-vue") => vec![
            "Uses Vue 3 with a single-file component (`App.vue`) structure".into(),
            "Supports TypeScript or JavaScript".into(),
            "Supports Vite or Webpack".into(),
            "Optional CSS preprocessors, Tailwind, and common lint tools".into(),
        ],
        (_, "toolkit") => vec![
            "Single-package structure for utilities or SDKs".into(),
            "Built around Vite by default".into(),
            "Supports TypeScript with optional Vitest".into(),
            "Includes common engineering and release setup".into(),
        ],
        (_, "toolkit-monorepo") => vec![
            "Multi-package structure with a default `packages/core` workspace".into(),
            "Supports pnpm workspace and changesets release flow".into(),
            "Suitable for component libraries, toolkits, or modular repos".into(),
            "Includes common engineering and commit tooling".into(),
        ],
        _ => Vec::new(),
    }
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
