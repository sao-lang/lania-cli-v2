use std::{collections::BTreeSet, path::{Path, PathBuf}};

use anyhow::{anyhow, Context, Result};

use crate::generate_schema::{
    exported_name, normalize_source_kind, parse_graphql_schema_contract,
    parse_json_schema_contract, parse_proto_contract, parse_proto_contract_from_files,
    parse_thrift_contract,
    parse_thrift_contract_from_path, slugify,
};
use crate::generate_types::{
    CompiledModuleEntry, ContractIr, ModuleInputConfig, ModuleOverridesConfig,
};

use super::overrides::apply_module_overrides;
use super::source_scan::collect_source_files;

// compile 阶段面向“单个 module input”，把源文件、目标平台、合同 IR 和模块名
// 编译成统一的 `CompiledModuleEntry`。
// 上层 planning 不需要了解不同 schema 格式如何解析，也不需要关心 include 扫描细节。
pub(super) fn compile_module_entry(
    entry: &ModuleInputConfig,
    config_dir: &Path,
    effective_targets: &[String],
    overrides: Option<&ModuleOverridesConfig>,
    module_name_override: Option<&str>,
    entry_filter_count: usize,
) -> Result<CompiledModuleEntry> {
    let source_kind = normalize_source_kind(&entry.source)?;

    // entry 可以声明自己的 targets，但最终不能超出全局/CLI 允许范围。
    // 这里通过 allowed_targets 做一次交集裁剪，保证后续渲染只针对真正启用的目标。
    let allowed_targets = effective_targets.iter().cloned().collect::<BTreeSet<_>>();
    let mut entry_targets = if entry.targets.is_empty() {
        effective_targets.to_vec()
    } else {
        entry
            .targets
            .iter()
            .map(|target| crate::generate_schema::normalize_target_kind(target))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|target| allowed_targets.contains(target))
            .collect::<Vec<_>>()
    };
    entry_targets.sort();
    entry_targets.dedup();
    if entry_targets.is_empty() {
        return Err(anyhow!(
            "module input {} did not enable any targets (check input.targets / targets.enabled / --target)",
            entry.name
        ));
    }

    let base_path = config_dir.join(&entry.path);
    let include_patterns = if entry.include.is_empty() {
        match source_kind.as_str() {
            "proto" => vec!["**/*.proto".to_string()],
            "thrift" => vec!["**/*.thrift".to_string()],
            "json" => vec![
                "**/*.json".to_string(),
                "**/*.yaml".to_string(),
                "**/*.yml".to_string(),
            ],
            "graphql" => vec!["**/*.graphql".to_string()],
            _ => unreachable!(),
        }
    } else {
        entry.include.clone()
    };
    let input_paths = collect_source_files(&base_path, &include_patterns)?;
    if input_paths.is_empty() {
        return Err(anyhow!(
            "module input {} did not resolve any schema files under {}",
            entry.name,
            base_path.display()
        ));
    }

    // 同一个 entry 可以由多个源文件共同组成，这里先全部解析再汇总成一个合同 IR。
    let mut ir = ContractIr::default();
    let mut visited_thrift_paths = BTreeSet::<PathBuf>::new();
    if source_kind == "proto" {
        ir = parse_proto_contract_from_files(&input_paths, &[base_path.clone()])?;
    } else {
    for input_path in &input_paths {
        let content = std::fs::read_to_string(input_path)
            .with_context(|| format!("failed to read source schema {}", input_path.display()))?;
        let parsed = parse_source_contract(
            &source_kind,
            &content,
            Some(input_path),
            &input_paths,
            &base_path,
            &mut visited_thrift_paths,
        )?;
        merge_contract_ir(&mut ir, parsed);
    }
    }
    // overrides 统一在 IR 层应用，避免 render 阶段再感知“原始 schema 与覆盖规则”的双重来源。
    apply_module_overrides(&mut ir, overrides);
    if source_kind == "json" && ir.services.is_empty() {
        return Err(anyhow!(
            "json schema input {} requires overrides.operations to describe service methods",
            entry.name
        ));
    }

    if module_name_override.is_some() && entry_filter_count > 1 {
        return Err(anyhow!(
            "--module-name only works when a single module entry is selected"
        ));
    }

    Ok(CompiledModuleEntry {
        name: entry.name.clone(),
        slug: slugify(&entry.name),
        source_kind,
        targets: entry_targets,
        input_paths,
        ir,
        module_name: module_name_override
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{}Module", exported_name(&entry.name))),
    })
}

// 各种 schema 格式的解析入口统一收口到这里。
// JSON 额外支持“先按 JSON 解析，失败后再按 YAML 读取并转 JSON”的兼容路径，
// 这样配置层只需要声明 `json`，不用区分文件扩展名到底是 json 还是 yaml。
fn parse_source_contract(
    source_kind: &str,
    content: &str,
    source_path: Option<&Path>,
    all_input_paths: &[PathBuf],
    base_path: &Path,
    visited_thrift_paths: &mut BTreeSet<PathBuf>,
) -> Result<ContractIr> {
    match source_kind {
        "proto" => {
            if let Some(path) = source_path {
                let mut include_paths = vec![base_path.to_path_buf()];
                if let Some(parent) = path.parent() {
                    include_paths.push(parent.to_path_buf());
                }
                parse_proto_contract_from_files(all_input_paths, &include_paths)
            } else {
                parse_proto_contract(content)
            }
        }
        "thrift" => {
            if let Some(path) = source_path {
                parse_thrift_contract_from_path(path, content, visited_thrift_paths)
            } else {
                parse_thrift_contract(content)
            }
        }
        "json" => match parse_json_schema_contract(content) {
            Ok(ir) => Ok(ir),
            Err(json_err) => {
                let yaml_value: serde_yaml::Value =
                    serde_yaml::from_str(content).context("invalid yaml schema")?;
                let json_value =
                    serde_json::to_value(yaml_value).context("failed to convert yaml to json")?;
                let json_content = serde_json::to_string(&json_value)
                    .context("failed to serialize yaml-as-json")?;
                parse_json_schema_contract(&json_content)
                    .map_err(|e| anyhow!("invalid json/yaml schema: {e} (json error: {json_err})"))
            }
        },
        "graphql" => parse_graphql_schema_contract(content),
        _ => Err(anyhow!("unsupported source kind: {source_kind}")),
    }
}

fn merge_contract_ir(target: &mut ContractIr, parsed: ContractIr) {
    target.aliases.extend(parsed.aliases);
    target.consts.extend(parsed.consts);
    target.enums.extend(parsed.enums);
    target.types.extend(parsed.types);
    target.services.extend(parsed.services);
}
