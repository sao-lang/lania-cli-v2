//! `generate api` 使用的 manifest 读写与 managed path 管理。
//!
//! 这里的 manifest 不是配置副本，而是一份“生成器认领状态”：
//! - 哪些输出文件由生成器负责
//! - 输入和 IR 的快照 hash 是什么
//! - 之后 diff/check/clean 应该以哪些路径为准
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use lania_format::{FormatMode, FormatOptions, FormatService};
use lania_fs::PlannedFile;

use crate::generate_schema::stable_hash;
use crate::generate_types::{
    CompiledContractEntry, ContractManifest, ContractManifestEntry, ContractOutputConfig,
    GeneratedContractPlan,
};
use crate::models::{GenerateApiWorkflowInput, WorkflowServices};
use crate::workflow_hooks::{call_files_prepare, write_files_with_hooks};

pub(crate) fn resolve_manifest_path(
    input: &GenerateApiWorkflowInput,
    config_dir: &Path,
    output: &ContractOutputConfig,
) -> PathBuf {
    // manifest 路径优先级：
    // 1. CLI `--manifest`
    // 2. config.defaults.output.manifest
    // 3. 默认 `.lania/contracts.lock.json`
    let raw = input
        .manifest_path
        .as_deref()
        .or(output.manifest.as_deref())
        .unwrap_or(".lania/contracts.lock.json");
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        config_dir.join(path)
    }
}

pub(crate) fn load_contract_manifest(path: &Path, config_path: &Path) -> Result<ContractManifest> {
    // manifest 不存在不是错误：
    // - 首次运行生成器时本来就没有 manifest
    // - 此时返回一个空的默认 manifest，后续流程仍可统一处理
    if !path.exists() {
        return Ok(ContractManifest {
            version: 1,
            config_path: config_path.display().to_string(),
            module_file: None,
            entries: BTreeMap::new(),
        });
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read contract manifest {}", path.display()))?;
    let manifest: ContractManifest = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse contract manifest {}", path.display()))?;
    Ok(manifest)
}

pub(crate) async fn write_contract_manifest(
    services: &WorkflowServices,
    workflow: &str,
    target_dir: &Path,
    path: &Path,
    manifest: &ContractManifest,
) -> Result<()> {
    // manifest 写入也走“格式化 + hooks + write”统一管线，
    // 这样它和其它生成文件在 diff、日志、hook 观察上保持一致。
    let mut content = serde_json::to_string_pretty(manifest)?;
    content.push('\n');
    let formatter = FormatService;
    let mut files = vec![PlannedFile {
        path: path.to_path_buf(),
        content,
    }];
    let root_dir = path.parent().map(Path::to_path_buf);
    let _format_report = formatter.format_planned_files(
        &services.exec,
        &mut files,
        &FormatOptions {
            enabled: true,
            mode: FormatMode::BestEffort,
            root_dir,
        },
    )?;
    let content = files.pop().map(|file| file.content).unwrap_or_default();
    let mut planned = vec![PlannedFile {
        path: path.to_path_buf(),
        content,
    }];
    call_files_prepare(services, workflow, target_dir, &mut planned).await?;
    let _report = write_files_with_hooks(services, workflow, target_dir, &planned, true).await?;
    Ok(())
}

pub(crate) fn update_contract_manifest(
    manifest: &mut ContractManifest,
    entries: &[CompiledContractEntry],
    plans: &[GeneratedContractPlan],
    config_path: &Path,
) {
    // manifest 记录的是“这次 generation 之后，哪些输出由生成器认领”。
    // 它不是一份完整的业务配置副本，而是偏向增量生成/清理用的状态文件。
    manifest.version = 1;
    manifest.config_path = config_path.display().to_string();
    manifest.module_file = plans
        .iter()
        .find(|plan| plan.owner.is_none())
        .map(|plan| plan.path.display().to_string());

    for entry in entries {
        let outputs = plans
            .iter()
            .filter(|plan| plan.owner.as_deref() == Some(entry.name.as_str()))
            .map(|plan| plan.path.display().to_string())
            .collect::<Vec<_>>();
        // 这里用输入文件内容 + IR debug 文本做 hash，
        // 目的不是安全校验，而是为了快速判断“输入或解析结果是否变化”。
        let source_snapshot = entry
            .input_paths
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n--input--\n");
        let ir_snapshot = format!("{:#?}", entry.ir);
        manifest.entries.insert(
            entry.name.clone(),
            ContractManifestEntry {
                source_kind: entry.source_kind.clone(),
                targets: entry.targets.clone(),
                input_hash: stable_hash(&source_snapshot),
                ir_hash: stable_hash(&ir_snapshot),
                outputs,
            },
        );
    }
}

pub(crate) fn manifest_managed_paths(
    manifest: &ContractManifest,
    selected_entries: &[CompiledContractEntry],
    scoped: bool,
) -> BTreeSet<PathBuf> {
    // `scoped=false`：返回整个 manifest 中所有受生成器管理的路径
    // `scoped=true`：只返回当前 selected entries 对应的路径
    // 后者常用于“只生成/清理某几个 entry”时避免误伤其它产物。
    let mut paths = BTreeSet::new();
    if let Some(module_file) = &manifest.module_file {
        paths.insert(PathBuf::from(module_file));
    }
    let selected_names = selected_entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<BTreeSet<_>>();
    for (entry_name, entry) in &manifest.entries {
        if scoped && !selected_names.contains(entry_name.as_str()) {
            continue;
        }
        for output in &entry.outputs {
            paths.insert(PathBuf::from(output));
        }
    }
    paths
}
