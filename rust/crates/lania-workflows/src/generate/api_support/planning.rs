//! `generate api` 的计划准备与摘要生成逻辑。
//!
//! 这里把“配置 + manifest + 当前磁盘状态”三份信息揉在一起，
//! 产出后续执行阶段真正需要的 plan、summary 和附加说明文字。
use std::{collections::BTreeSet, path::Path};

use anyhow::{anyhow, Context, Result};

use crate::generate_api_support::{
    compile_contract_entry, load_contract_manifest, matches_generate_filters,
    render_contract_entry, render_module_file, resolve_contract_config_path, resolve_manifest_path,
};
use crate::generate_types::{
    ContractConfig, ContractPlanSummary, GeneratedContractPlan, PreparedGeneratePlan,
};
use crate::models::{GenerateApiMode, GenerateApiWorkflowInput};

pub(crate) fn prepare_generate_plan(
    input: &GenerateApiWorkflowInput,
) -> Result<PreparedGeneratePlan> {
    // “准备阶段”做的事情：
    // - 读配置文件并校验
    // - 计算 output/manifest 路径
    // - 编译 entries（把路径、filters、schema kind 都归一化）
    // - 产出 `generated_plans`（每个 plan 都是一个最终文件的 {path, content}）
    let config_path = resolve_contract_config_path(input);
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input.cwd.clone());
    let config_content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read contract config {}", config_path.display()))?;
    let config: ContractConfig = serde_yaml::from_str(&config_content)
        .with_context(|| format!("failed to parse contract config {}", config_path.display()))?;
    validate_contract_config(&config)?;

    let output = config
        .defaults
        .clone()
        .unwrap_or_default()
        .output
        .unwrap_or_default();
    let contract_dir = config_dir.join(
        output
            .contract_dir
            .clone()
            .unwrap_or_else(|| "contracts/generated".into()),
    );
    let transport_dir = config_dir.join(
        output
            .transport_dir
            .clone()
            .unwrap_or_else(|| "transport/generated".into()),
    );
    let module_file = config_dir.join(
        output
            .module_file
            .clone()
            .unwrap_or_else(|| "modules/generated_module.gen.go".into()),
    );
    let manifest_path = resolve_manifest_path(input, &config_dir, &output);
    // manifest 的作用：
    // - 记录“生成器认领的输出文件集合”（managed paths）
    // - 用于 diff/check/clean
    // - 用于判断冲突：如果文件存在但不在 managed paths 里，默认视为用户自管文件
    let manifest = load_contract_manifest(&manifest_path, &config_path)?;

    let all_entries = config
        .entries
        .iter()
        .map(|entry| compile_contract_entry(entry, &config_dir))
        .collect::<Result<Vec<_>>>()?;
    let selected_entries = all_entries
        .iter()
        .filter(|entry| matches_generate_filters(entry, input))
        .cloned()
        .collect::<Vec<_>>();
    if selected_entries.is_empty() {
        return Err(anyhow!("no contract entries matched the requested filters"));
    }

    let mut generated_plans = Vec::new();
    for entry in &selected_entries {
        generated_plans.extend(render_contract_entry(entry, &contract_dir, &transport_dir)?);
    }
    generated_plans.push(render_module_file(&all_entries, &module_file));

    Ok(PreparedGeneratePlan {
        config_path,
        config_dir,
        manifest_path,
        manifest,
        selected_entries,
        generated_plans,
        language: config
            .defaults
            .and_then(|defaults| defaults.language)
            .unwrap_or_else(|| "go".into()),
    })
}

pub(crate) fn summarize_contract_plan(
    plans: &[GeneratedContractPlan],
    previous_managed_paths: &BTreeSet<std::path::PathBuf>,
    force: bool,
) -> Result<ContractPlanSummary> {
    // 这一步把“生成计划”和“磁盘现状 + 旧 manifest”做对比，形成摘要：
    // - unchanged：磁盘内容已是最新
    // - to_write：需要写入（新文件或内容变更）
    // - conflicts：磁盘已有文件，但不在旧 manifest 的 managed paths 里
    // - stale：旧 manifest 里有，但新计划不再需要（可在 clean 模式删）
    let mut summary = ContractPlanSummary::default();
    let planned_paths = plans
        .iter()
        .map(|plan| plan.path.clone())
        .collect::<BTreeSet<_>>();
    for plan in plans {
        if plan.path.exists() {
            let current = std::fs::read_to_string(&plan.path).unwrap_or_default();
            if current == plan.content {
                summary.unchanged.push(plan.path.clone());
                continue;
            }
            if !force && !previous_managed_paths.contains(&plan.path) {
                summary.conflicts.push(plan.path.clone());
                continue;
            }
        }
        summary.to_write.push(plan.path.clone());
    }
    for stale in previous_managed_paths {
        if !planned_paths.contains(stale) && stale.exists() {
            summary.stale.push(stale.clone());
        }
    }
    Ok(summary)
}

pub(crate) fn has_generate_filters(input: &GenerateApiWorkflowInput) -> bool {
    // 只要出现任一过滤条件，就说明本次不是“全量生成”。
    !input.entry_filter.is_empty()
        || !input.source_filter.is_empty()
        || !input.target_filter.is_empty()
}

pub(crate) fn append_contract_plan_notes(
    notes: &mut Vec<String>,
    summary: &ContractPlanSummary,
    input: &GenerateApiWorkflowInput,
) {
    // 这些 note 主要服务于 plan/check/diff/clean 输出，让用户不用自己从 summary 反推模式语义。
    notes.push(format!(
        "plan summary: {} write, {} unchanged, {} conflicts, {} stale",
        summary.to_write.len(),
        summary.unchanged.len(),
        summary.conflicts.len(),
        summary.stale.len()
    ));
    if matches!(input.mode, GenerateApiMode::Plan) || input.dry_run {
        notes.push("dry-run: no files were written".into());
    }
    if matches!(input.mode, GenerateApiMode::Diff) {
        notes.push("diff mode: compared manifest and current generation plan".into());
    }
    if input.check {
        if !summary.to_write.is_empty()
            || !summary.stale.is_empty()
            || !summary.conflicts.is_empty()
        {
            notes.push("drift detected: generated outputs are not up to date".into());
        } else {
            notes.push("check passed: generated outputs are up to date".into());
        }
    }
    if !summary.unchanged.is_empty() {
        notes.push(format!(
            "incremental skip: {} unchanged files reused",
            summary.unchanged.len()
        ));
    }
    if !summary.conflicts.is_empty() {
        notes.push(format!(
            "conflicts detected: {} unmanaged files require --force",
            summary.conflicts.len()
        ));
    }
    if input.clean && !summary.stale.is_empty() {
        notes.push(format!(
            "clean mode: {} stale generated files will be removed",
            summary.stale.len()
        ));
    }
}

pub(crate) fn validate_contract_config(config: &ContractConfig) -> Result<()> {
    // 这里只做最基础、最刚性的结构校验。
    // 更细的字段级约束往往留给后面的 compile/render 阶段去给出上下文更强的报错。
    if config.version != 1 {
        return Err(anyhow!(
            "unsupported contract config version: {}",
            config.version
        ));
    }
    if config.entries.is_empty() {
        return Err(anyhow!("contract config must contain at least one entry"));
    }
    Ok(())
}
