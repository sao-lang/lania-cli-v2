//! 原 create 包的 `template` 子命令迁入 workspace（命令上移为 `lan workspace template`）。
//!
//! 本模块承载原 `lania-plugins-command-template` / create 内 `template.rs` 的
//! spec 与 handler：handler 代码零改动，仅注册点迁到 workspace 根命令下。

use std::io::{stderr, stdin, IsTerminal, Write};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use lania_command::{ArgSpec, CommandContext, CommandSpec, Example};
use lania_host::{
    execution::{CommandExecution, CommandExecutionContext, CommandHandler, ExecutionError},
    EXIT_RUNTIME_ERROR, EXIT_SUCCESS,
};
use lania_node_bridge::TemplateBridgeCapability;
use lania_prompt::{
    PromptFallbackStrategy, PromptFlow, PromptRunOptions, PromptStep, PromptStepKind,
};

pub const TEMPLATE_HANDLER_ID: &str = "command.template";

#[derive(Debug, Default)]
pub struct TemplateCommandSpec;

pub struct TemplateCommandHandler;

impl TemplateCommandSpec {
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
            TEMPLATE_HANDLER_ID,
        );
        spec.args = vec![ArgSpec {
            name: "name".into(),
            required: false,
            multiple: false,
            help: "Template name".into(),
        }];
        spec.examples = vec![
            Example {
                command: "lan workspace template".into(),
                description: "Inspect available templates in an interactive selector".into(),
            },
            Example {
                command: "lan workspace template toolkit".into(),
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
        template_name.is_none() && ctx.command().handler_id == TEMPLATE_HANDLER_ID && stdin().is_terminal()
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
        // 展示文案统一由 cli-kit 提供，避免 human 输出层再写一份。
        lania_cli_kit::template_summary(name, locale)
    }

    fn template_highlights(name: &str, locale: &str) -> Vec<String> {
        lania_cli_kit::template_highlights(name, locale)
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

#[async_trait(?Send)]
impl CommandHandler for TemplateCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let requested_template_name = TemplateCommandSpec::resolve_template_name(ctx.command());
        if requested_template_name.as_deref() == Some("info") {
            return Err(ExecutionError {
                exit_code: EXIT_RUNTIME_ERROR,
                message: if ctx.locale() == "zh" {
                    "未知命令：`lan workspace template info`；请使用 `lan workspace template` 或 `lan workspace template <name>`".into()
                } else {
                    "unknown command: `lan workspace template info`; use `lan workspace template` or `lan workspace template <name>`".into()
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
                TemplateCommandSpec::localized(
                    ctx.locale(),
                    "template.list returned no payload",
                    "template.list 未返回有效结果",
                )
            )
        })?;

        let templates = result["templates"].clone();
        let metadata = result["metadata"].as_array().cloned().unwrap_or_default();
        let interactive_info = TemplateCommandSpec::is_interactive_template_info(
            ctx,
            requested_template_name.as_deref(),
        );
        let template_name = if interactive_info {
            TemplateCommandSpec::pick_template_interactively(ctx, &metadata)?
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
                        if TemplateCommandSpec::is_zh_locale(ctx.locale()) {
                            format!("未知模板：{name}")
                        } else {
                            format!("unknown template: {name}")
                        }
                    )
                })?;
            if interactive_info {
                TemplateCommandSpec::maybe_print_interactive_detail(ctx, &detail)?;
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
                    "list": "lan workspace template",
                    "detail": "lan workspace template <name>"
                }
            })
        };

        Ok(ctx.complete_template_info(output, EXIT_SUCCESS))
    }
}

#[cfg(test)]
mod tests {
    use super::TemplateCommandSpec;

    // ── is_zh_locale / localized ─────────────────────────────

    #[test]
    fn is_zh_locale_returns_true_for_zh() {
        assert!(TemplateCommandSpec::is_zh_locale("zh"));
    }

    #[test]
    fn is_zh_locale_returns_false_for_en() {
        assert!(!TemplateCommandSpec::is_zh_locale("en"));
    }

    #[test]
    fn localized_returns_zh_when_locale_is_zh() {
        assert_eq!(
            TemplateCommandSpec::localized("zh", "hello", "你好"),
            "你好"
        );
    }

    // ── localized_label ──────────────────────────────────────

    #[test]
    fn localized_label_returns_english_by_default() {
        assert_eq!(
            TemplateCommandSpec::localized_label("template", "en"),
            "Template"
        );
        assert_eq!(
            TemplateCommandSpec::localized_label("summary", "en"),
            "Summary"
        );
        assert_eq!(
            TemplateCommandSpec::localized_label("highlights", "en"),
            "Highlights"
        );
    }

    #[test]
    fn localized_label_returns_chinese_when_locale_is_zh() {
        assert_eq!(
            TemplateCommandSpec::localized_label("template", "zh"),
            "模板"
        );
        assert_eq!(
            TemplateCommandSpec::localized_label("summary", "zh"),
            "简介"
        );
        assert_eq!(
            TemplateCommandSpec::localized_label("highlights", "zh"),
            "关键特征"
        );
    }

    // ── template_summary ─────────────────────────────────────

    #[test]
    fn template_summary_returns_english_for_spa_react() {
        let summary = TemplateCommandSpec::template_summary("spa-react", "en");
        assert!(summary.contains("React"));
    }

    #[test]
    fn template_summary_returns_chinese_for_spa_react() {
        let summary = TemplateCommandSpec::template_summary("spa-react", "zh");
        assert!(summary.contains("React"));
    }

    #[test]
    fn template_summary_returns_chinese_for_toolkit_monorepo() {
        let summary = TemplateCommandSpec::template_summary("toolkit-monorepo", "zh");
        assert!(summary.contains("monorepo"));
    }

    #[test]
    fn template_summary_falls_back_to_name_for_unknown() {
        let summary = TemplateCommandSpec::template_summary("custom", "en");
        assert_eq!(summary, "custom");
    }

    // ── template_highlights ──────────────────────────────────

    #[test]
    fn template_highlights_returns_four_items_for_spa_react() {
        let highlights = TemplateCommandSpec::template_highlights("spa-react", "en");
        assert_eq!(highlights.len(), 4);
        assert!(highlights[0].contains("React"));
    }

    #[test]
    fn template_highlights_returns_chinese_for_zh_locale() {
        let highlights = TemplateCommandSpec::template_highlights("spa-react", "zh");
        assert_eq!(highlights.len(), 4);
        assert!(highlights[0].contains("React"));
    }

    #[test]
    fn template_highlights_returns_empty_for_unknown() {
        let highlights = TemplateCommandSpec::template_highlights("unknown", "en");
        assert!(highlights.is_empty());
    }

    // ── render_template_detail ───────────────────────────────

    #[test]
    fn render_template_detail_contains_name_and_summary() {
        let detail = serde_json::json!({
            "name": "spa-react",
            "summary": "React SPA"
        });
        let rendered = TemplateCommandSpec::render_template_detail(&detail, "en");
        assert!(rendered.contains("spa-react"));
        assert!(rendered.contains("Summary"));
        assert!(rendered.contains("React"));
    }

    #[test]
    fn render_template_detail_renders_highlights_section() {
        let detail = serde_json::json!({
            "name": "toolkit",
        });
        let rendered = TemplateCommandSpec::render_template_detail(&detail, "zh");
        assert!(rendered.contains("关键特征"));
        assert!(rendered.contains("工具库"));
    }

    // ── template_choice_label ────────────────────────────────

    #[test]
    fn template_choice_label_extracts_name() {
        let meta = serde_json::json!({"name": "spa-react", "version": "1.0"});
        assert_eq!(
            TemplateCommandSpec::template_choice_label(&meta, "en"),
            "spa-react"
        );
    }

    #[test]
    fn template_choice_label_falls_back_to_unknown() {
        let meta = serde_json::json!({"version": "1.0"});
        assert_eq!(
            TemplateCommandSpec::template_choice_label(&meta, "en"),
            "unknown"
        );
    }

    // ── resolve_template_name ────────────────────────────────

    #[test]
    fn resolve_template_name_returns_none_when_no_name() {
        let context = lania_command::CommandContext {
            name: String::new(),
            handler_id: "command.template".into(),
            argv: lania_command::ParsedArgv {
                args: std::collections::BTreeMap::new(),
                options: std::collections::BTreeMap::new(),
            },
            cwd: "/tmp".into(),
            trace_id: "test".into(),
        };
        assert_eq!(TemplateCommandSpec::resolve_template_name(&context), None);
    }

    #[test]
    fn resolve_template_name_reads_from_args() {
        let mut args = std::collections::BTreeMap::new();
        args.insert("name".into(), serde_json::json!("toolkit"));
        let context = lania_command::CommandContext {
            name: String::new(),
            handler_id: "command.template".into(),
            argv: lania_command::ParsedArgv {
                args,
                options: std::collections::BTreeMap::new(),
            },
            cwd: "/tmp".into(),
            trace_id: "test".into(),
        };
        assert_eq!(
            TemplateCommandSpec::resolve_template_name(&context),
            Some("toolkit".into())
        );
    }
}
