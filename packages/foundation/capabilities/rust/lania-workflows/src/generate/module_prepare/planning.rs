//! `generate module` 的计划收敛层。
//!
//! 这个模块负责把配置文件、CLI 过滤条件和输出目录规则整理成一份
//! `PreparedGenerateModulePlan`。后面的执行阶段只消费这份计划，不再重复读配置、
//! 重跑筛选或重新推导注入目标。
use std::path::Path;

use anyhow::{anyhow, Context, Result};

use crate::generate_module_inject::{
    prepare_main_go_injection, resolve_against_cwd, should_inject_modules,
};
use crate::generate_module_manifest::{
    load_module_manifest, resolve_module_config_path, resolve_module_manifest_path,
    validate_module_config,
};
use crate::generate_module_render::{
    is_single_grpc_output_entry, is_single_http_output_entry, render_module_entry,
    render_module_registry_file,
};
use crate::generate_schema::{normalize_source_filter, normalize_target_filter, normalize_target_kind};
use crate::generate_types::{
    CompiledModuleEntry, ModuleConfig, ModuleOutputConfig, ModuleOutputPaths,
    PreparedGenerateModulePlan,
};
use crate::models::GenerateModuleWorkflowInput;

use super::compile::compile_module_entry;

// 规划阶段负责把“模块生成配置 + CLI 输入”收敛成一份 PreparedGenerateModulePlan。
// 后续真正的写文件/执行流程只消费这份计划，不再重复读取配置、筛选 entry
// 或解析目标目录。
pub(crate) fn prepare_generate_module_plan(
    input: &GenerateModuleWorkflowInput,
) -> Result<PreparedGenerateModulePlan> {
    // 主入口会同时完成：
    // - 读取/校验模块配置
    // - 确定 output 与 manifest 路径
    // - 编译全部 entry，保留全局视图
    // - 只为命中的 entry 渲染业务输出计划
    // - 视情况追加 registry/main.go 注入计划
    let config_path = resolve_module_config_path(input);
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input.cwd.clone());
    let config_content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read module config {}", config_path.display()))?;
    let config: ModuleConfig = serde_yaml::from_str(&config_content)
        .with_context(|| format!("failed to parse module config {}", config_path.display()))?;
    validate_module_config(&config)?;

    // framework 允许被 CLI 参数覆盖，但当前只支持 `lania-g`。
    // 将来如果扩展更多 framework，这里仍然是最合适的总入口校验点。
    let framework = input
        .framework
        .clone()
        .unwrap_or_else(|| config.framework.name.clone());
    if framework != "lania-g" {
        return Err(anyhow!("unsupported module framework: {framework}"));
    }

    let output = config.output.clone().unwrap_or_default();
    let module_output = resolve_module_output(&config_dir, &output);
    let manifest_path = resolve_module_manifest_path(input, &config_dir, &output);
    let manifest = load_module_manifest(&manifest_path, &config_path, &framework)?;
    let effective_targets = resolve_module_targets(&config, input)?;
    let input_override = input
        .input_path
        .as_ref()
        .map(|raw| resolve_against_cwd(&input.cwd, raw));

    // 先编译所有 entry，再做筛选。
    // 这样 registry 文件、main.go 注入等“全局视图”逻辑仍然能看到完整 entry 集合，
    // 不会被当前命令的局部 filter 误伤。
    let all_entries = config
        .inputs
        .iter()
        .map(|entry| {
            compile_module_entry(
                entry,
                &config_dir,
                &effective_targets,
                config.overrides.as_ref(),
                input.module_name.as_deref(),
                input.entry_filter.len(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let selected_entries = all_entries
        .iter()
        .filter(|entry| matches_generate_module_filters(entry, input, input_override.as_deref()))
        .cloned()
        .collect::<Vec<_>>();
    if selected_entries.is_empty() {
        return Err(anyhow!("no module inputs matched the requested filters"));
    }

    // 只为最终选中的 entry 渲染业务输出，但 registry 文件基于 all_entries 生成，
    // 目的是让生成结果保留完整注册视图，而不是只看到本次局部命中的片段。
    let mut generated_plans = Vec::new();
    for entry in &selected_entries {
        generated_plans.extend(render_module_entry(entry, &module_output)?);
    }
    let needs_module_registry = selected_entries
        .iter()
        .any(|entry| !is_single_http_output_entry(entry) && !is_single_grpc_output_entry(entry));
    if needs_module_registry {
        generated_plans.push(render_module_registry_file(
            &all_entries,
            &module_output.module_dir.join("generated_modules.gen.go"),
        ));
    }

    let mut planning_notes = Vec::new();
    // main.go 注入属于“可选增强”，因此即便缺少 marker block 或目标文件，
    // 也只记录说明，不让整个生成流程失败。
    if should_inject_modules(&config, input) && needs_module_registry {
        match prepare_main_go_injection(
            &config,
            input,
            &config_dir,
            &module_output.module_dir,
            &all_entries,
        )? {
            Some((main_go_plan, inject_helper_plan)) => {
                generated_plans.push(main_go_plan);
                generated_plans.push(inject_helper_plan);
            }
            None => {
                planning_notes.push(
                    "main.go injection skipped: marker block or target file missing".into(),
                )
            }
        }
    } else if should_inject_modules(&config, input) && !needs_module_registry {
        planning_notes.push("main.go injection skipped: single-target http/grpc output mode".into());
    } else {
        planning_notes.push("main.go injection disabled".into());
    }

    Ok(PreparedGenerateModulePlan {
        config_path: config_path.clone(),
        config_dir,
        manifest_path: manifest_path.clone(),
        manifest,
        all_entries,
        selected_entries,
        generated_plans,
        language: config
            .framework
            .language
            .clone()
            .unwrap_or_else(|| "go".into()),
        framework,
        planning_notes,
    })
}

// 输出目录的默认值与相对路径解析集中放在这里，
// 避免 compile/render 阶段重复推导 contracts/adapters/modules 三组位置。
pub(crate) fn resolve_module_output(
    config_dir: &Path,
    output: &ModuleOutputConfig,
) -> ModuleOutputPaths {
    let root = config_dir.join(
        output
            .root
            .clone()
            .unwrap_or_else(|| "generated/lania".into()),
    );
    ModuleOutputPaths {
        module_dir: output
            .module_dir
            .clone()
            .map(|value| config_dir.join(value))
            .unwrap_or_else(|| root.join("modules")),
        adapter_dir: output
            .adapter_dir
            .clone()
            .map(|value| config_dir.join(value))
            .unwrap_or_else(|| root.join("adapters")),
        contract_dir: output
            .contract_dir
            .clone()
            .map(|value| config_dir.join(value))
            .unwrap_or_else(|| root.join("contracts")),
        http_root_dir: output
            .http_root_dir
            .clone()
            .map(|value| config_dir.join(value)),
        http_root_import: output
            .http_root_dir
            .clone()
            .map(|value| value.replace('\\', "/")),
        grpc_root_dir: output
            .grpc_root_dir
            .clone()
            .map(|value| config_dir.join(value)),
        grpc_root_import: output
            .grpc_root_dir
            .clone()
            .map(|value| value.replace('\\', "/")),
    }
}

// 有效 target 的来源有两层：
// - 配置里全局启用的 targets / inputs[].targets
// - 当前命令通过 `--target` 传入的显式过滤
// 这里统一求交并去重，得到后续 compile 阶段真正允许输出的目标集合。
fn resolve_module_targets(
    config: &ModuleConfig,
    input: &GenerateModuleWorkflowInput,
) -> Result<Vec<String>> {
    let configured = if config.targets.is_empty() {
        config
            .inputs
            .iter()
            .flat_map(|input| input.targets.iter())
            .map(|target| normalize_target_kind(target))
            .collect::<Result<Vec<_>>>()?
    } else {
        config
            .targets
            .iter()
            .filter(|target| target.enabled.unwrap_or(true))
            .map(|target| normalize_target_kind(&target.kind))
            .collect::<Result<Vec<_>>>()?
    };
    // CLI 显式 `--target` 一旦给出，就视为最终过滤目标；
    // 否则使用配置文件声明的可用 target 集合。
    let mut effective = if input.target_filter.is_empty() {
        configured
    } else {
        input
            .target_filter
            .iter()
            .map(|target| normalize_target_kind(target))
            .collect::<Result<Vec<_>>>()?
    };
    effective.sort();
    effective.dedup();
    if effective.is_empty() {
        return Err(anyhow!(
            "module config must enable at least one target (via targets or inputs[].targets)"
        ));
    }
    Ok(effective)
}

// 统一封装 entry/source/target/input-path 四类过滤条件，
// 保证“哪些 entry 会参与本次生成”的判定逻辑只维护一处。
fn matches_generate_module_filters(
    entry: &CompiledModuleEntry,
    input: &GenerateModuleWorkflowInput,
    input_override: Option<&Path>,
) -> bool {
    // 这里把 entry/source/target/input-path 四类过滤统一合并，
    // 保证“本次命中了哪些 entry”只有这一处判定逻辑。
    let entry_match =
        input.entry_filter.is_empty() || input.entry_filter.iter().any(|value| value == &entry.name);
    let source_match = input.source_filter.is_empty()
        || input
            .source_filter
            .iter()
            .any(|value| normalize_source_filter(value) == entry.source_kind);
    let target_match = input.target_filter.is_empty()
        || entry.targets.iter().any(|target| {
            input
                .target_filter
                .iter()
                .any(|value| normalize_target_filter(value) == *target)
        });
    let input_match = input_override.is_none_or(|override_path| {
        entry.input_paths.iter().any(|path| {
            if override_path.is_file() {
                path == override_path
            } else {
                path.starts_with(override_path)
            }
        })
    });
    entry_match && source_match && target_match && input_match
}
