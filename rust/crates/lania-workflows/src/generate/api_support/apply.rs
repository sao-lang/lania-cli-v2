use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use lania_format::{FormatMode, FormatOptions, FormatService};
use lania_fs::PlannedFile;

use crate::generate_types::{ContractWriteOutcome, GeneratedContractPlan};
use crate::models::WorkflowServices;

pub(crate) async fn apply_contract_generation(
    services: &WorkflowServices,
    workflow: &str,
    target_dir: &Path,
    plans: &[GeneratedContractPlan],
    previous_managed_paths: &BTreeSet<PathBuf>,
    force: bool,
) -> Result<ContractWriteOutcome> {
    let mut outcome = ContractWriteOutcome::default();
    let formatter = FormatService;
    let mut to_write_files = Vec::new();
    for plan in plans {
        if plan.path.exists() {
            let current = std::fs::read_to_string(&plan.path).unwrap_or_default();
            if current == plan.content {
                outcome.skipped.push(plan.path.clone());
                continue;
            }
            // “冲突”的判断逻辑：
            // - 如果文件已存在，并且旧 manifest 没认领过它
            // - 那么默认认为这是用户自管文件，不能直接覆盖
            // - 只有 `--force` 才允许覆盖
            if !force && !previous_managed_paths.contains(&plan.path) {
                outcome.conflicts.push(plan.path.clone());
                continue;
            }
        }

        // 先在内存里格式化，再决定最终写入内容。
        // 注意：format 可能失败，但 Mode=BestEffort 会尽量不影响整体生成流程。
        let mut content = plan.content.clone();
        let mut files = vec![PlannedFile {
            path: plan.path.clone(),
            content,
        }];
        let root_dir = plan.path.parent().map(Path::to_path_buf);
        let _format_report = formatter.format_planned_files(
            &services.exec,
            &mut files,
            &FormatOptions {
                enabled: true,
                mode: FormatMode::BestEffort,
                root_dir,
            },
        )?;
        content = files
            .pop()
            .map(|file| file.content)
            .unwrap_or_else(|| plan.content.clone());
        to_write_files.push(PlannedFile {
            path: plan.path.clone(),
            content,
        });
    }
    // v2.1 hooks: allow rewriting planned outputs and emit onFileWrite events.
    // 如果没有任何需要写入的文件，就直接返回（这也是 check 模式常见结果）。
    if to_write_files.is_empty() {
        return Ok(outcome);
    }
    crate::workflow_hooks::call_files_prepare(services, workflow, target_dir, &mut to_write_files)
        .await?;
    let report = crate::workflow_hooks::write_files_with_hooks(
        services,
        workflow,
        target_dir,
        &to_write_files,
        true,
    )
    .await?;
    outcome.written.extend(report.written);
    Ok(outcome)
}

pub(crate) async fn remove_stale_generated_files(
    services: &WorkflowServices,
    workflow: &str,
    target_dir: &Path,
    paths: &[PathBuf],
) -> Result<()> {
    for path in paths {
        if path.is_dir() {
            continue;
        }
        if path.exists() {
            services
                .hooks
                .call_parallel(
                    format!("workflow:{workflow}"),
                    lania_hooks::hook_keys::ON_FILE_WRITE.to_string(),
                    crate::workflow_hooks::merge(
                        crate::workflow_hooks::base_payload(services, workflow),
                        serde_json::json!({
                            "file": {
                                "path": path.display().to_string(),
                                "relativePath": path.strip_prefix(target_dir).ok().and_then(|p| p.to_str()).map(|s| s.to_string()),
                                "stage": "remove_before"
                            }
                        }),
                    ),
                )
                .await
                .ok();
            std::fs::remove_file(path).with_context(|| {
                format!("failed to remove stale generated file {}", path.display())
            })?;
            services
                .hooks
                .call_parallel(
                    format!("workflow:{workflow}"),
                    lania_hooks::hook_keys::ON_FILE_WRITE.to_string(),
                    crate::workflow_hooks::merge(
                        crate::workflow_hooks::base_payload(services, workflow),
                        serde_json::json!({
                            "file": {
                                "path": path.display().to_string(),
                                "relativePath": path.strip_prefix(target_dir).ok().and_then(|p| p.to_str()).map(|s| s.to_string()),
                                "stage": "remove_after",
                                "ok": true
                            }
                        }),
                    ),
                )
                .await
                .ok();
        }
    }
    Ok(())
}
