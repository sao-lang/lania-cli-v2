//! API 生成主流程，负责组织 schema、渲染输出与结果汇总。
//!
//! 主要导出：run。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::json;

use crate::generate_api_support::{
    append_contract_plan_notes, apply_contract_generation, has_generate_filters,
    initialize_generate_api, manifest_managed_paths, prepare_generate_plan,
    remove_stale_generated_files, render_generate_command_plan, summarize_contract_plan,
    update_contract_manifest, write_contract_manifest,
};
use crate::generate_types::ContractWriteOutcome;
use crate::models::{
    GenerateApiMode, GenerateApiWorkflow, GenerateApiWorkflowInput, WorkflowExecution,
    WorkflowServices, WorkflowState,
};

impl GenerateApiWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: GenerateApiWorkflowInput,
    ) -> Result<WorkflowExecution> {
        // generate-api 的主流程和 create/release 一样，也是分阶段的：
        // 1. 先准备 generation plan
        // 2. 再算 diff / conflict / stale files
        // 3. 最后根据 mode 决定是只展示计划，还是实际写文件
        services.progress.begin("generate-api", Some(5));
        if matches!(input.mode, GenerateApiMode::Init) {
            let execution = initialize_generate_api(services, &input).await?;
            services.progress.finish("generate-api");
            return Ok(execution);
        }

        // `prepared` 里会聚合：
        // - 配置文件路径
        // - manifest
        // - 选中的条目
        // - 生成计划
        // 后续逻辑尽量只基于这一个“准备结果对象”往下走。
        let mut prepared = prepare_generate_plan(&input)?;
        services.progress.advance("generate-api", 2);
        let scoped_previous_paths = manifest_managed_paths(
            &prepared.manifest,
            &prepared.selected_entries,
            has_generate_filters(&input),
        );

        let summary = summarize_contract_plan(
            &prepared.generated_plans,
            &scoped_previous_paths,
            input.force,
        )?;
        services.progress.advance("generate-api", 1);

        let state = match input.mode {
            GenerateApiMode::Plan | GenerateApiMode::Diff => WorkflowState::Planned,
            GenerateApiMode::Apply if input.dry_run || input.check => WorkflowState::Planned,
            _ => WorkflowState::Completed,
        };

        // `write_outcome` 把“计划阶段”和“实际写入阶段”统一成同一份结果结构，
        // 这样最终输出层不必关心这次到底是 plan、diff 还是 apply。
        let write_outcome = if matches!(input.mode, GenerateApiMode::Plan | GenerateApiMode::Diff)
            || input.dry_run
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
                "generate-api",
                &prepared.config_dir,
                &prepared.generated_plans,
                &scoped_previous_paths,
                input.force,
            )
            .await?;
            if input.clean {
                remove_stale_generated_files(
                    services,
                    "generate-api",
                    &prepared.config_dir,
                    &summary.stale,
                )
                .await?;
            }
            update_contract_manifest(
                &mut prepared.manifest,
                &prepared.selected_entries,
                &prepared.generated_plans,
                &prepared.config_path,
            );
            write_contract_manifest(
                services,
                "generate-api",
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
        services.progress.advance("generate-api", 1);

        let selected_names = prepared
            .selected_entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        let mut notes = vec![
            format!("contract config version: {}", prepared.manifest.version),
            format!("language: {}", prepared.language),
            format!("selected entries: {}", selected_names.join(", ")),
            format!("manifest: {}", prepared.manifest_path.display()),
        ];
        append_contract_plan_notes(&mut notes, &summary, &input);
        services.progress.finish("generate-api");

        Ok(WorkflowExecution {
            // 和其它工作流一样，最终统一返回 `WorkflowExecution`，
            // 让 host/output 层可以用一致方式渲染结果。
            workflow: "generate-api".into(),
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
                ("dryRun".into(), json!(input.dry_run)),
                ("check".into(), json!(input.check)),
                ("clean".into(), json!(input.clean)),
                (
                    "mode".into(),
                    json!(match input.mode {
                        GenerateApiMode::Apply => "apply",
                        GenerateApiMode::Plan => "plan",
                        GenerateApiMode::Diff => "diff",
                        GenerateApiMode::Init => "init",
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
            command_plans: vec![render_generate_command_plan(&prepared.config_path, &input)],
            git_status: None,
            notes,
            interactive_rendered: false,
        })
    }
}
