//! `add` 工作流：在现有项目里追加一个模板产物，而不是初始化整套新项目。
//!
//! 它和 `create` 很像，但目标更聚焦：
//! - 询问要添加什么模板、写到哪里
//! - 从 bridge 渲染出单个文件或少量文件
//! - 走格式化、冲突检查、hook、写盘
//!
//! 可以把它理解成“create 的轻量版增量写入流程”。

use std::path::Path;

use anyhow::{anyhow, Result};
use lania_format::{FormatMode, FormatOptions, FormatService};
use lania_fs::PlannedFile;
use serde_json::{json, Value};

use crate::models::{
    redact_prompt_answers, AddWorkflow, AddWorkflowInput, WorkflowExecution, WorkflowServices,
    WorkflowState,
};
use crate::workflow_hooks::{call_files_prepare, write_files_with_hooks};

use super::capability::AddTemplateCapability;
use super::helpers::*;
use super::prompts::run_add_prompt;

impl AddWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: AddWorkflowInput,
    ) -> Result<WorkflowExecution> {
        services.progress.begin("add", Some(4));
        let capability = AddTemplateCapability::new(&services.bridge);
        let mut bridge_steps = Vec::new();
        let prompt_state = {
            let _progress_guard = services.progress.suspend_terminal_guard();
            run_add_prompt(&services.prompt, services.locale.as_str(), &input)?
        };
        let template_name = prompt_state["template"]
            .as_str()
            .ok_or_else(|| anyhow!("add prompt did not resolve template"))?
            .to_string();
        let target = prompt_state["target"]
            .as_str()
            .ok_or_else(|| anyhow!("add prompt did not resolve target"))?
            .to_string();
        let normalized_target = normalize_add_target(&target);
        validate_relative_target(&normalized_target)?;
        let config = load_add_template_context(services, &input.cwd).await?;
        let (rendered_file, render_step) = capability
            .render(
                &template_name,
                json!({
                    "projectName": config.project_name,
                    "language": config.language,
                    "cssProcessor": config.css_processor,
                }),
            )
            .await?;
        bridge_steps.push(render_step);
        services.progress.advance("add", 1);

        let output_path = resolve_add_output_path(
            &input.cwd,
            &normalized_target,
            prompt_state.get("name").and_then(Value::as_str),
            &rendered_file,
        )?;
        let target_dir = output_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| input.cwd.clone());
        let mut files = vec![PlannedFile {
            path: output_path,
            content: rendered_file.content.clone(),
        }];
        let formatter = FormatService;
        let format_report = formatter.format_planned_files(
            &services.exec,
            &mut files,
            &FormatOptions {
                enabled: true,
                mode: FormatMode::BestEffort,
                root_dir: Some(target_dir.clone()),
            },
        )?;
        let conflicts = services.fs.find_conflicts(&files);
        call_files_prepare(services, "add", &target_dir, &mut files).await?;
        let report =
            write_files_with_hooks(services, "add", &target_dir, &files, input.force).await?;
        services.progress.advance("add", 2);
        services.progress.finish("add");

        let mut notes = vec![
            "dedicated add template render applied".into(),
            format!(
                "add template {} rendered via node bridge assets",
                rendered_file.template
            ),
            format!("normalized target: {}", normalized_target),
            format!("language: {}", config.language),
            format!("css processor: {}", config.css_processor),
        ];
        if format_report.formatted_count() > 0 {
            notes.push(format!(
                "formatted {} generated files",
                format_report.formatted_count()
            ));
        }
        if format_report.failed_count() > 0 {
            notes.push(format!(
                "formatters failed for {} files (best-effort: kept original content)",
                format_report.failed_count()
            ));
        }

        Ok(WorkflowExecution {
            workflow: "add".into(),
            state: WorkflowState::Completed,
            target_dir: target_dir.display().to_string(),
            prompts: redact_prompt_answers(&prompt_state, &services.prompt.secret_fields()),
            bridge_steps,
            written_files: report
                .written
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
            conflicts: conflicts
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
            command_plans: Vec::new(),
            git_status: None,
            notes,
            interactive_rendered: false,
        })
    }
}
