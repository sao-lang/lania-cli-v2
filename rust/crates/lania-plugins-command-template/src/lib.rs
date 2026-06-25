//! 将 `template` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 主要导出：spec、TemplateCommandPlugin、HANDLER_ID、INFO_HANDLER_ID。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use std::io::{stderr, stdin, IsTerminal, Write};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use lania_command::{ArgSpec, CommandContext, CommandSpec, Example};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler, ExecutionError},
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
    EXIT_RUNTIME_ERROR, EXIT_SUCCESS,
};
use lania_node_bridge::TemplateBridgeCapability;
use lania_prompt::{
    PromptFallbackStrategy, PromptFlow, PromptRunOptions, PromptStep, PromptStepKind,
};

pub const HANDLER_ID: &str = "command.template";
#[derive(Debug, Default)]
pub struct TemplateCommandPlugin;

struct TemplateCommandHandler;

impl TemplateCommandPlugin {
    fn is_zh_locale(locale: &str) -> bool {
        locale == "zh"
    }

    fn localized<'a>(locale: &str, en: &'a str, zh: &'a str) -> &'a str {
        if Self::is_zh_locale(locale) {
            zh
        } else {
            en
        }
    }

    pub fn spec() -> CommandSpec {
        let mut spec = CommandSpec::new(
            "template",
            "List templates or inspect a template detail",
            HANDLER_ID,
        );
        spec.args = vec![ArgSpec {
            name: "name".into(),
            required: false,
            multiple: false,
            help: "Template name".into(),
        }];
        spec.examples = vec![
            Example {
                command: "lan template".into(),
                description: "Inspect available templates in an interactive selector".into(),
            },
            Example {
                command: "lan template toolkit".into(),
                description: "Show detail information for a specific template".into(),
            },
        ];
        spec
    }

    fn resolve_template_name(context: &CommandContext) -> Option<String> {
        context
            .argv
            .options
            .get("name")
            .or_else(|| context.argv.args.get("name"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
    }

    fn is_interactive_template_info(
        ctx: &CommandExecutionContext<'_>,
        template_name: Option<&str>,
    ) -> bool {
        template_name.is_none() && ctx.command().handler_id == HANDLER_ID && stdin().is_terminal()
    }

    fn template_choice_label(metadata: &serde_json::Value, _locale: &str) -> String {
        metadata["name"].as_str().unwrap_or("unknown").to_string()
    }

    fn pick_template_interactively(
        ctx: &CommandExecutionContext<'_>,
        metadata: &[serde_json::Value],
    ) -> Result<Option<String>> {
        let locale = ctx.locale();
        let zh = Self::is_zh_locale(locale);
        let mut step = PromptStep::new(
            "template",
            if zh {
                "选择模板"
            } else {
                "Choose a template"
            },
            "template",
        )
        .kind(PromptStepKind::FuzzySelect)
        .detail(if zh {
            "上下选择并回车查看模板详情"
        } else {
            "Use arrow keys and press Enter to inspect template details"
        });

        if let Some(default_name) = metadata
            .first()
            .and_then(|item| item["name"].as_str())
            .map(ToOwned::to_owned)
        {
            step = step.default_value(serde_json::json!(default_name));
        }

        for item in metadata {
            if let Some(name) = item["name"].as_str() {
                step = step.choice(
                    Self::template_choice_label(item, locale),
                    serde_json::json!(name),
                );
            }
        }

        let flow = PromptFlow::new().step(step);
        let state = ctx.prompt().run_cli_with_options(
            &flow,
            PromptRunOptions {
                fallback: Some(PromptFallbackStrategy::Error),
                ..PromptRunOptions::default()
            },
        )?;

        if state.interrupted {
            return Ok(None);
        }

        Ok(state
            .answers
            .get("template")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned))
    }

    fn should_render_interactive_detail_to_stderr(ctx: &CommandExecutionContext<'_>) -> bool {
        ctx.project_config()
            .map(|config| config.ui.output.mode != "human")
            .unwrap_or(true)
    }

    fn localized_label(key: &str, locale: &str) -> &'static str {
        match (Self::is_zh_locale(locale), key) {
            (true, "template") => "模板",
            (true, "summary") => "简介",
            (true, "highlights") => "关键特征",
            (_, "template") => "Template",
            (_, "summary") => "Summary",
            (_, "highlights") => "Highlights",
            _ => "Unknown",
        }
    }

    fn template_summary(name: &str, locale: &str) -> String {
        match (Self::is_zh_locale(locale), name) {
            (true, "spa-react") => "基于 React 的单页应用项目模板，适合快速启动前端业务项目。".into(),
            (true, "spa-vue") => "基于 Vue 3 的单文件组件项目模板，适合快速启动前端业务项目。".into(),
            (true, "toolkit") => "面向工具库或 SDK 的单包项目模板，适合构建可复用的前端工具模块。".into(),
            (true, "toolkit-monorepo") => {
                "面向多包协作场景的 monorepo 模板，适合组件库、工具集或多模块工程。".into()
            }
            (_, "spa-react") => {
                "A React single-page application template for quickly bootstrapping frontend projects.".into()
            }
            (_, "spa-vue") => {
                "A Vue 3 single-file component template for quickly bootstrapping frontend projects.".into()
            }
            (_, "toolkit") => {
                "A single-package toolkit template for building reusable utilities or SDKs.".into()
            }
            (_, "toolkit-monorepo") => {
                "A monorepo template for multi-package toolkits, component libraries, or modular projects.".into()
            }
            _ => name.to_string(),
        }
    }

    fn template_highlights(name: &str, locale: &str) -> Vec<String> {
        match (Self::is_zh_locale(locale), name) {
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

    fn render_template_detail(detail: &serde_json::Value, locale: &str) -> String {
        let mut lines = Vec::new();
        let name = detail["name"].as_str().unwrap_or("unknown");
        lines.push(format!(
            "{}: {}",
            Self::localized_label("template", locale),
            name
        ));
        lines.push(format!(
            "{}: {}",
            Self::localized_label("summary", locale),
            Self::template_summary(name, locale)
        ));
        let highlights = Self::template_highlights(name, locale);
        if !highlights.is_empty() {
            lines.push(Self::localized_label("highlights", locale).to_string());
            for item in highlights {
                lines.push(format!("- {item}"));
            }
        }

        lines.join("\n")
    }

    fn maybe_print_interactive_detail(
        ctx: &CommandExecutionContext<'_>,
        detail: &serde_json::Value,
    ) -> Result<()> {
        if !Self::should_render_interactive_detail_to_stderr(ctx) {
            return Ok(());
        }

        let mut handle = stderr().lock();
        writeln!(
            handle,
            "{}",
            Self::render_template_detail(detail, ctx.locale())
        )?;
        Ok(())
    }
}

impl Plugin for TemplateCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-template".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![CapabilityName::NodeBridge, CapabilityName::Prompt],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<TemplateCommandHandler, _>(
            ctx,
            "command-template",
            "template",
            Self::spec(),
            &[HANDLER_ID],
            || TemplateCommandHandler,
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for TemplateCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let requested_template_name = TemplateCommandPlugin::resolve_template_name(ctx.command());
        if requested_template_name.as_deref() == Some("info") {
            return Err(ExecutionError {
                exit_code: EXIT_RUNTIME_ERROR,
                message: if ctx.locale() == "zh" {
                    "未知命令：`lan template info`；请使用 `lan template` 或 `lan template <name>`".into()
                } else {
                    "unknown command: `lan template info`; use `lan template` or `lan template <name>`".into()
                },
            }
            .into());
        }
        let exchange = ctx
            .node_bridge()
            .list_templates(ctx.command().cwd.clone())
            .await?;
        let result = exchange.response.result.clone().ok_or_else(|| {
            anyhow!(
                "{}",
                TemplateCommandPlugin::localized(
                    ctx.locale(),
                    "template.list returned no payload",
                    "template.list 未返回有效结果",
                )
            )
        })?;

        let templates = result["templates"].clone();
        let metadata = result["metadata"].as_array().cloned().unwrap_or_default();
        let interactive_info = TemplateCommandPlugin::is_interactive_template_info(
            ctx,
            requested_template_name.as_deref(),
        );
        let template_name = if interactive_info {
            TemplateCommandPlugin::pick_template_interactively(ctx, &metadata)?
                .or(requested_template_name)
        } else {
            requested_template_name
        };

        let output = if let Some(name) = template_name {
            let detail = metadata
                .iter()
                .find(|item| item["name"].as_str() == Some(name.as_str()))
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "{}",
                        if TemplateCommandPlugin::is_zh_locale(ctx.locale()) {
                            format!("未知模板：{name}")
                        } else {
                            format!("unknown template: {name}")
                        }
                    )
                })?;
            if interactive_info {
                TemplateCommandPlugin::maybe_print_interactive_detail(ctx, &detail)?;
            }
            serde_json::json!({
                "template": name,
                "cwd": ctx.command().cwd.clone(),
                "metadata": detail,
                "availableTemplates": templates,
                "_interactiveRendered": interactive_info,
            })
        } else {
            serde_json::json!({
                "cwd": ctx.command().cwd.clone(),
                "templates": templates,
                "metadata": metadata,
                "usage": {
                    "list": "lan template",
                    "detail": "lan template <name>"
                }
            })
        };

        Ok(ctx.complete_template_info(output, EXIT_SUCCESS))
    }
}
