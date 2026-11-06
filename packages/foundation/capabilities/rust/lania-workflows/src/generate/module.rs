//! 模块生成主流程，串联准备、渲染、注入与 manifest 更新。
//!
//! 主要导出：run。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::json;

use crate::generate_api_support::{
    apply_contract_generation, remove_stale_generated_files, summarize_contract_plan,
};
use crate::generate_module_manifest::{
    append_generate_module_notes, has_generate_module_filters, module_manifest_managed_paths,
    module_safe_overwrite_paths, render_generate_module_command_plan, update_module_manifest,
    write_module_manifest,
};
use crate::generate_module_prepare::{initialize_generate_module, prepare_generate_module_plan};
use crate::generate_types::ContractWriteOutcome;
use crate::models::{
    GenerateModuleMode, GenerateModuleWorkflow, GenerateModuleWorkflowInput, WorkflowExecution,
    WorkflowServices, WorkflowState,
};

impl GenerateModuleWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: GenerateModuleWorkflowInput,
    ) -> Result<WorkflowExecution> {
        // generate-module 和 generate-api 结构很像，但会多出一层“框架注入/manifest 管理”语义。
        // 因此这里也复用了很多 generate-api_support 的通用能力。
        services.progress.begin("generate-module", Some(5));
        if matches!(input.mode, GenerateModuleMode::Init) {
            let execution = initialize_generate_module(services, &input).await?;
            services.progress.finish("generate-module");
            return Ok(execution);
        }

        let mut prepared = prepare_generate_module_plan(&input)?;
        services.progress.advance("generate-module", 2);
        let mut scoped_previous_paths = module_manifest_managed_paths(
            &prepared.manifest,
            &prepared.selected_entries,
            has_generate_module_filters(&input),
        );
        scoped_previous_paths.extend(module_safe_overwrite_paths(&prepared.generated_plans));
        let summary = summarize_contract_plan(
            &prepared.generated_plans,
            &scoped_previous_paths,
            input.force,
        )?;
        services.progress.advance("generate-module", 1);

        let state = match input.mode {
            GenerateModuleMode::Plan | GenerateModuleMode::Diff => WorkflowState::Planned,
            GenerateModuleMode::Apply if input.dry_run || input.check => WorkflowState::Planned,
            _ => WorkflowState::Completed,
        };

        // safe_overwrite_paths 的存在说明：
        // 不是所有“已存在文件”都算冲突；
        // 有些文件本来就是生成器管理的、允许幂等覆盖的。
        let write_outcome = if matches!(
            input.mode,
            GenerateModuleMode::Plan | GenerateModuleMode::Diff
        ) || input.dry_run
            || input.check
        {
            ContractWriteOutcome {
                written: summary.to_write.clone(),
                conflicts: summary.conflicts.clone(),
                skipped: summary.unchanged.clone(),
                removed: if input.clean {
                    summary.stale.clone()
                } else {
                    Vec::new()
                },
            }
        } else if !summary.conflicts.is_empty() {
            ContractWriteOutcome {
                written: Vec::new(),
                conflicts: summary.conflicts.clone(),
                skipped: summary.unchanged.clone(),
                removed: Vec::new(),
            }
        } else {
            let outcome = apply_contract_generation(
                services,
                "generate-module",
                &prepared.config_dir,
                &prepared.generated_plans,
                &scoped_previous_paths,
                input.force,
            )
            .await?;
            if input.clean {
                remove_stale_generated_files(
                    services,
                    "generate-module",
                    &prepared.config_dir,
                    &summary.stale,
                )
                .await?;
            }
            update_module_manifest(
                &mut prepared.manifest,
                &prepared.selected_entries,
                &prepared.generated_plans,
                &prepared.config_path,
                &prepared.framework,
            );
            write_module_manifest(
                services,
                "generate-module",
                &prepared.config_dir,
                &prepared.manifest_path,
                &prepared.manifest,
            )
            .await?;
            ContractWriteOutcome {
                removed: if input.clean {
                    summary.stale.clone()
                } else {
                    Vec::new()
                },
                ..outcome
            }
        };
        services.progress.advance("generate-module", 1);

        let selected_names = prepared
            .selected_entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        let mut notes = vec![
            format!("module config version: {}", prepared.manifest.version),
            format!("language: {}", prepared.language),
            format!("framework: {}", prepared.framework),
            format!("selected entries: {}", selected_names.join(", ")),
            format!("manifest: {}", prepared.manifest_path.display()),
        ];
        notes.extend(prepared.planning_notes);
        append_generate_module_notes(&mut notes, &summary, &input, prepared.all_entries.len());
        services.progress.finish("generate-module");
        services
            .tasks
            .complete("generate-module", "Generate module workflow completed");

        Ok(WorkflowExecution {
            // `notes` 在 generate-module 里尤其重要，因为它常会解释：
            // - 本次选中了哪些 entry
            // - 发生了哪些注入/清理
            // - 为什么某些文件被视为安全覆盖
            workflow: "generate-module".into(),
            state,
            target_dir: prepared.config_dir.display().to_string(),
            prompts: BTreeMap::from([
                (
                    "config".into(),
                    json!(prepared.config_path.display().to_string()),
                ),
                ("entries".into(), json!(selected_names)),
                ("sources".into(), json!(input.source_filter)),
                ("targets".into(), json!(input.target_filter)),
                ("framework".into(), json!(prepared.framework)),
                ("dryRun".into(), json!(input.dry_run)),
                ("check".into(), json!(input.check)),
                ("clean".into(), json!(input.clean)),
                ("noInject".into(), json!(input.no_inject)),
                (
                    "mode".into(),
                    json!(match input.mode {
                        GenerateModuleMode::Apply => "apply",
                        GenerateModuleMode::Plan => "plan",
                        GenerateModuleMode::Diff => "diff",
                        GenerateModuleMode::Init => "init",
                    }),
                ),
            ]),
            bridge_steps: Vec::new(),
            written_files: write_outcome
                .written
                .into_iter()
                .chain(write_outcome.removed.into_iter())
                .map(|path| path.display().to_string())
                .collect(),
            conflicts: write_outcome
                .conflicts
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
            command_plans: vec![render_generate_module_command_plan(
                &prepared.config_path,
                &input,
            )],
            git_status: None,
            notes,
            interactive_rendered: false,
        })
    }
}
