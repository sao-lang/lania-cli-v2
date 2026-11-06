use std::{collections::BTreeSet, path::{Path, PathBuf}};

use anyhow::{anyhow, Context, Result};

use crate::generate_schema::{
    normalize_source_filter, normalize_source_kind, normalize_target_filter, normalize_target_kind,
    parse_proto_contract, parse_proto_contract_from_files, parse_thrift_contract_from_path,
    slugify,
};
use crate::generate_types::{CompiledContractEntry, ContractEntryConfig, ContractIr};
use crate::models::GenerateApiWorkflowInput;

pub(crate) fn compile_contract_entry(
    entry: &ContractEntryConfig,
    config_dir: &Path,
) -> Result<CompiledContractEntry> {
    // compile 的目标是把“配置里的 entry”转换成“真正可渲染的编译结果”：
    // - source/target 做 canonical normalize
    // - 输入路径转成绝对路径
    // - 读取 schema 内容并合并为统一 IR
    let source_kind = normalize_source_kind(&entry.source.kind)?;
    let targets = entry
        .targets
        .iter()
        .map(|target| normalize_target_kind(target))
        .collect::<Result<Vec<_>>>()?;
    if targets.is_empty() {
        return Err(anyhow!(
            "entry {} must declare at least one target",
            entry.name
        ));
    }

    let input_paths = entry
        .source
        .inputs
        .iter()
        .map(|input| {
            let path = PathBuf::from(input);
            if path.is_absolute() {
                path
            } else {
                config_dir.join(path)
            }
        })
        .collect::<Vec<_>>();
    if input_paths.is_empty() {
        return Err(anyhow!(
            "entry {} must declare at least one input",
            entry.name
        ));
    }

    let mut ir = ContractIr::default();
    let mut visited_thrift_paths = BTreeSet::<PathBuf>::new();
    if source_kind == "proto" {
        let include_paths = input_paths
            .iter()
            .filter_map(|path| path.parent().map(|parent| parent.to_path_buf()))
            .collect::<Vec<_>>();
        ir = parse_proto_contract_from_files(&input_paths, &include_paths)?;
    } else {
        for input_path in &input_paths {
            let content = std::fs::read_to_string(input_path)
                .with_context(|| format!("failed to read source schema {}", input_path.display()))?;
            // 同一个 entry 允许多个输入文件，最后会把类型/服务合并到同一个 IR。
            let parsed = match source_kind.as_str() {
                "proto" => parse_proto_contract(&content)?,
                "thrift" => {
                    parse_thrift_contract_from_path(input_path, &content, &mut visited_thrift_paths)?
                }
                _ => unreachable!(),
            };
            merge_contract_ir(&mut ir, parsed);
        }
    }

    Ok(CompiledContractEntry {
        name: entry.name.clone(),
        slug: slugify(&entry.name),
        source_kind,
        targets,
        input_paths,
        ir,
    })
}

fn merge_contract_ir(target: &mut ContractIr, parsed: ContractIr) {
    target.aliases.extend(parsed.aliases);
    target.consts.extend(parsed.consts);
    target.enums.extend(parsed.enums);
    target.types.extend(parsed.types);
    target.services.extend(parsed.services);
}

pub(crate) fn matches_generate_filters(
    entry: &CompiledContractEntry,
    input: &GenerateApiWorkflowInput,
) -> bool {
    // 三类 filter 的组合逻辑是“分别判断、最后 AND”：
    // 只要用户提供了某类 filter，就必须至少命中这一类。
    let entry_match = input.entry_filter.is_empty()
        || input.entry_filter.iter().any(|value| value == &entry.name);
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
    entry_match && source_match && target_match
}
