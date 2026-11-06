//! 负责把 Thrift schema 解析成内部统一使用的 `ContractIr`。
//!
//! 与 protobuf 不同，这里主链路依赖 tree-sitter 提供 AST，再把 include、typedef、const、
//! service、annotation 等语义重新整理成内部模型。这个模块的难点不在于识别单个 token，
//! 而在于把 Thrift 的“注解 + include + 常量引用”组合规则稳定落到后续代码生成能消费的结构里。
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use tree_sitter::{Node, Parser, Tree};

use crate::generate_types::{
    ContractAlias, ContractConst, ContractEnum, ContractEnumVariant, ContractField,
    ContractHttpFieldBinding, ContractIr, ContractMethod, ContractService, ContractType,
    ContractTypeKind,
};

use super::shared::{apply_lania_comment_overrides, method_ir, split_thrift_comment};
use super::thrift_scalar_to_go;

pub(crate) fn parse_thrift_contract(content: &str) -> Result<ContractIr> {
    let mut ir = parse_thrift_document(content)?;
    resolve_thrift_constants(&mut ir);
    Ok(ir)
}

pub(crate) fn parse_thrift_contract_from_path(
    path: &Path,
    content: &str,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<ContractIr> {
    let mut ir = parse_thrift_contract_recursive(path, content, visited)?;
    resolve_thrift_constants(&mut ir);
    Ok(ir)
}

fn parse_thrift_contract_recursive(
    path: &Path,
    content: &str,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<ContractIr> {
    // 递归展开 include，并用 `visited` 防止环形依赖导致无限递归。
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical) {
        return Ok(ContractIr::default());
    }
    let mut ir = parse_thrift_document(content)?;
    for include_path in extract_thrift_include_paths(content)? {
        let resolved = resolve_thrift_include_path(path, &include_path)?;
        let include_content = std::fs::read_to_string(&resolved)
            .with_context(|| format!("failed to read thrift include {}", resolved.display()))?;
        let include_ir = parse_thrift_contract_recursive(&resolved, &include_content, visited)?;
        merge_contract_ir(&mut ir, include_ir);
    }
    Ok(ir)
}

fn parse_thrift_document(content: &str) -> Result<ContractIr> {
    let mut ir = ContractIr::default();
    let tree = parse_thrift_tree(content)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(anyhow!("invalid thrift syntax"));
    }

    visit_thrift_document_node(content, root, &mut ir);
    Ok(ir)
}

fn merge_contract_ir(target: &mut ContractIr, parsed: ContractIr) {
    target.aliases.extend(parsed.aliases);
    target.consts.extend(parsed.consts);
    target.enums.extend(parsed.enums);
    target.types.extend(parsed.types);
    target.services.extend(parsed.services);
}

#[allow(dead_code)]
pub(crate) fn parse_thrift_field(line: &str) -> Option<ContractField> {
    let segment = line.trim_end_matches(',').trim_end_matches(';').trim();
    let (signature, annotations) = split_trailing_annotation_block(segment);
    let after_colon = signature.split_once(':')?.1.trim();
    let mut rest = after_colon;
    let mut required = false;
    let mut optional = false;
    if let Some(value) = rest.strip_prefix("required ") {
        required = true;
        rest = value.trim();
    } else if let Some(value) = rest.strip_prefix("optional ") {
        optional = true;
        rest = value.trim();
    }
    let (ty, rest_after_type) = parse_type_token(rest)?;
    let rest_after_type = rest_after_type.trim();
    let (name, default_value) = parse_field_name_and_default(rest_after_type)?;
    let mut field = ContractField {
        name,
        ty: parse_thrift_type_to_go(&ty),
        required,
        optional,
        oneof_group: None,
        default_value,
        http_binding: None,
        validation_rules: Vec::new(),
    };
    apply_thrift_field_annotations(&mut field, annotations.as_deref());
    Some(field)
}

#[allow(dead_code)]
pub(crate) fn parse_thrift_method(line: &str) -> Option<crate::generate_types::ContractMethod> {
    let segment = line.trim_end_matches(',').trim_end_matches(';').trim();
    let open = segment.find('(')?;
    let close = find_matching_paren(segment, open)?;
    let signature = segment[..open].trim();
    let mut signature_tokens = signature.split_whitespace().collect::<Vec<_>>();
    let oneway = matches!(signature_tokens.first().copied(), Some("oneway"));
    if oneway {
        signature_tokens.remove(0);
    }
    if signature_tokens.len() < 2 {
        return None;
    }
    let method_name = signature_tokens.pop()?.to_string();
    let response_type = signature_tokens.pop()?.to_string();
    let params = parse_thrift_params(&segment[open + 1..close]);
    let request_name = derive_thrift_request_name(&params);
    let response_name = if response_type == "void" {
        "Empty".to_string()
    } else {
        parse_thrift_type_to_go(&response_type)
    };
    let mut method = method_ir(
        method_name.clone(),
        request_name,
        response_name,
        if oneway { "command" } else { "rpc" },
    );
    method.params = params;
    method.oneway = oneway;

    let mut rest = segment[close + 1..].trim();
    if let Some(value) = rest.strip_prefix("throws") {
        let value = value.trim();
        let throws_open = value.find('(')?;
        let throws_close = find_matching_paren(value, throws_open)?;
        method.throws = parse_thrift_params(&value[throws_open + 1..throws_close]);
        rest = value[throws_close + 1..].trim();
    }
    let (_, annotations) = split_trailing_annotation_block(rest);
    apply_thrift_method_annotations(&mut method, annotations.as_deref());
    Some(method)
}

#[allow(dead_code)]
fn find_matching_paren(segment: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in segment[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_thrift_tree(content: &str) -> Result<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_thrift::language())
        .map_err(|err| anyhow!("failed to load thrift tree-sitter grammar: {err}"))?;
    parser
        .parse(content, None)
        .ok_or_else(|| anyhow!("failed to parse thrift content"))
}

fn extract_thrift_include_paths(content: &str) -> Result<Vec<String>> {
    let tree = parse_thrift_tree(content)?;
    let root = tree.root_node();
    let mut includes = Vec::new();
    collect_thrift_include_nodes(content, root, &mut includes);
    Ok(includes)
}

fn first_named_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    for index in 0..node.named_child_count() {
        let child = node.named_child(index)?;
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

fn parse_thrift_type_ref_node(content: &str, node: Node) -> Option<String> {
    // 把 tree-sitter 的类型 AST 节点统一还原成内部使用的类型字符串。
    // 这里既处理基础类型，也处理 list/set/map 这类容器类型和自定义类型引用。
    match node.kind() {
        "field_type"
        | "definition_type"
        | "param_type"
        | "exception_param_type"
        | "container_type"
        | "custom_type" => {
            for index in 0..node.named_child_count() {
                let child = node.named_child(index)?;
                if let Some(value) = parse_thrift_type_ref_node(content, child) {
                    return Some(value);
                }
            }
            None
        }
        "return_type" => {
            let Some(child) = node.named_child(0) else {
                return Some("void".to_string());
            };
            parse_thrift_type_ref_node(content, child)
        }
        "list" | "set" => {
            let inner = first_named_child_by_kind(node, "field_type")?;
            Some(format!("[]{}", parse_thrift_type_ref_node(content, inner)?))
        }
        "map" => {
            let mut field_types = Vec::new();
            for index in 0..node.named_child_count() {
                let child = node.named_child(index)?;
                if child.kind() == "field_type" {
                    field_types.push(parse_thrift_type_ref_node(content, child)?);
                }
            }
            if field_types.len() == 2 {
                Some(format!("map[{}]{}", field_types[0], field_types[1]))
            } else {
                None
            }
        }
        "stream" | "sink" => Some(parse_thrift_type_to_go(node_text(content, node))),
        "primitive" | "identifier" | "type_identifier" => {
            let base = base_identifier(node_text(content, node));
            Some(thrift_scalar_to_go(&base))
        }
        _ => None,
    }
}

fn parse_thrift_annotation_definition(content: &str, node: Node) -> Option<String> {
    let mut parts = Vec::new();
    for index in 0..node.named_child_count() {
        let child = node.named_child(index)?;
        match child.kind() {
            "annotation_identifier" | "field_identifier" => {
                parts.push(node_text(content, child).trim().to_string());
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

fn parse_thrift_annotation_value(content: &str, node: Node) -> Option<String> {
    let value = node_text(content, node).trim();
    if value.is_empty() {
        None
    } else if node.kind() == "string" {
        Some(trim_quoted(value).to_string())
    } else {
        Some(value.to_string())
    }
}

fn parse_thrift_annotation_pairs_from_node(content: &str, node: Node) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut pending_key = None::<String>;
    for index in 0..node.named_child_count() {
        let Some(child) = node.named_child(index) else {
            continue;
        };
        match child.kind() {
            "annotation_definition" => {
                if let Some(previous_key) = pending_key
                    .replace(parse_thrift_annotation_definition(content, child).unwrap_or_default())
                {
                    if !previous_key.is_empty() {
                        pairs.push((previous_key, "true".to_string()));
                    }
                }
            }
            "annotation_value" => {
                if let Some(key) = pending_key.take().filter(|value| !value.is_empty()) {
                    if let Some(value_node) = child.named_child(0) {
                        if let Some(value) = parse_thrift_annotation_value(content, value_node) {
                            pairs.push((key, value));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(key) = pending_key.filter(|value| !value.is_empty()) {
        pairs.push((key, "true".to_string()));
    }
    pairs
}

fn parse_thrift_field_node(
    content: &str,
    node: Node,
    type_kind: &str,
    name_kind: &str,
) -> Option<ContractField> {
    let mut required = false;
    let mut optional = false;
    let mut ty = None::<String>;
    let mut name = None::<String>;
    let mut default_value = None::<String>;
    let mut annotation_pairs = Vec::new();
    let mut name_seen = false;
    for index in 0..node.named_child_count() {
        let child = node.named_child(index)?;
        match child.kind() {
            "field_modifier" => match node_text(content, child).trim() {
                "required" => required = true,
                "optional" => optional = true,
                _ => {}
            },
            kind if kind == type_kind => ty = parse_thrift_type_ref_node(content, child),
            kind if kind == name_kind => {
                name = Some(base_identifier(node_text(content, child)));
                name_seen = true;
            }
            "const_value" if name_seen => {
                default_value = Some(node_text(content, child).trim().to_string());
            }
            "annotation" => {
                annotation_pairs.extend(parse_thrift_annotation_pairs_from_node(content, child));
            }
            _ => {}
        }
    }
    let mut field = ContractField {
        name: name?,
        ty: ty?,
        required,
        optional,
        oneof_group: None,
        default_value,
        http_binding: None,
        validation_rules: Vec::new(),
    };
    apply_thrift_field_annotation_pairs(&mut field, annotation_pairs);
    Some(field)
}

fn parse_thrift_function_parameters_node(content: &str, node: Node) -> Vec<ContractField> {
    let mut fields = Vec::new();
    for index in 0..node.named_child_count() {
        let Some(child) = node.named_child(index) else {
            continue;
        };
        if child.kind() == "function_parameter" {
            if let Some(field) =
                parse_thrift_field_node(content, child, "param_type", "param_identifier")
            {
                fields.push(field);
            }
        }
    }
    fields
}

fn parse_thrift_throws_node(content: &str, node: Node) -> Vec<ContractField> {
    let mut fields = Vec::new();
    for index in 0..node.named_child_count() {
        let Some(child) = node.named_child(index) else {
            continue;
        };
        if child.kind() == "exception_parameters" {
            for param_index in 0..child.named_child_count() {
                let Some(param) = child.named_child(param_index) else {
                    continue;
                };
                if param.kind() == "exception_parameter" {
                    if let Some(field) = parse_thrift_field_node(
                        content,
                        param,
                        "exception_param_type",
                        "exception_param_identifier",
                    ) {
                        fields.push(field);
                    }
                }
            }
        }
    }
    fields
}

fn parse_thrift_include_node(content: &str, node: Node) -> Option<String> {
    let path_node = first_named_child_by_kind(node, "include_path")?;
    let value = trim_quoted(node_text(content, path_node)).trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_thrift_enum_node(content: &str, node: Node) -> Option<ContractEnum> {
    let name = base_identifier(node_text(
        content,
        first_named_child_by_kind(node, "enum_identifier")?,
    ));
    let mut variants = Vec::new();
    let mut children = Vec::new();
    for index in 0..node.named_child_count() {
        let child = node.named_child(index)?;
        children.push(child);
    }
    let mut index = 0usize;
    while index < children.len() {
        if children[index].kind() == "enum_member" {
            let mut value = None;
            if let Some(next) = children.get(index + 1) {
                if next.kind() == "number" {
                    value = Some(node_text(content, *next).trim().to_string());
                    index += 1;
                }
            }
            variants.push(ContractEnumVariant {
                name: base_identifier(node_text(content, children[index])),
                value,
            });
        }
        index += 1;
    }
    Some(ContractEnum { name, variants })
}

fn parse_thrift_type_node(
    content: &str,
    node: Node,
    kind: ContractTypeKind,
) -> Option<ContractType> {
    let name_kind = match kind {
        ContractTypeKind::Struct | ContractTypeKind::Union => "type_identifier",
        ContractTypeKind::Exception => "exception_identifier",
    };
    let name = base_identifier(node_text(
        content,
        first_named_child_by_kind(node, name_kind)?,
    ));
    let mut fields = Vec::new();
    for index in 0..node.named_child_count() {
        let child = node.named_child(index)?;
        if matches!(child.kind(), "field" | "recursive_field") {
            if let Some(field) =
                parse_thrift_field_node(content, child, "field_type", "field_identifier")
            {
                fields.push(field);
            }
        }
    }
    Some(ContractType { name, kind, fields })
}

fn parse_thrift_service_node(content: &str, node: Node) -> Option<ContractService> {
    let mut type_names = Vec::new();
    let mut methods = Vec::new();
    for index in 0..node.named_child_count() {
        let child = node.named_child(index)?;
        match child.kind() {
            "type_identifier" => type_names.push(base_identifier(node_text(content, child))),
            "function" => {
                if let Some(mut method) = parse_thrift_method_node(content, child) {
                    apply_lania_comment_overrides(
                        &mut method,
                        trailing_thrift_comment(content, child).as_deref(),
                    );
                    methods.push(method);
                }
            }
            _ => {}
        }
    }
    Some(ContractService {
        name: type_names.first()?.clone(),
        extends: type_names.get(1).cloned(),
        grpc_metadata: Default::default(),
        methods,
    })
}

fn parse_thrift_typedef_node(content: &str, node: Node) -> Option<ContractAlias> {
    Some(ContractAlias {
        name: base_identifier(node_text(
            content,
            first_named_child_by_kind(node, "typedef_definition")?,
        )),
        target: parse_thrift_type_ref_node(
            content,
            first_named_child_by_kind(node, "definition_type")?,
        )?,
    })
}

fn parse_thrift_const_node(content: &str, node: Node) -> Option<ContractConst> {
    let value_node = first_named_child_by_kind(node, "const_value")?;
    Some(ContractConst {
        name: base_identifier(node_text(
            content,
            first_named_child_by_kind(node, "const_identifier")?,
        )),
        ty: parse_thrift_type_ref_node(content, first_named_child_by_kind(node, "field_type")?)?,
        value: trim_quoted(node_text(content, value_node).trim()).to_string(),
    })
}

fn parse_thrift_method_node(content: &str, node: Node) -> Option<ContractMethod> {
    // 从 AST function 节点构造内部方法定义，并把 oneway / throws / annotation 一并吸收进来。
    // 注解不会直接保留原文本，而是会在这里转换成 HTTP 等上层协议字段。
    let mut oneway = false;
    let mut annotation_pairs = Vec::new();
    for index in 0..node.named_child_count() {
        let child = node.named_child(index)?;
        match child.kind() {
            "function_modifier" if node_text(content, child).trim() == "oneway" => oneway = true,
            "annotation" => {
                annotation_pairs.extend(parse_thrift_annotation_pairs_from_node(content, child));
            }
            _ => {}
        }
    }
    let method_name = base_identifier(node_text(
        content,
        first_named_child_by_kind(node, "function_identifier")?,
    ));
    let response_type =
        parse_thrift_type_ref_node(content, first_named_child_by_kind(node, "return_type")?)?;
    let params = first_named_child_by_kind(node, "function_parameters")
        .map(|child| parse_thrift_function_parameters_node(content, child))
        .unwrap_or_default();
    let throws = first_named_child_by_kind(node, "throws")
        .map(|child| parse_thrift_throws_node(content, child))
        .unwrap_or_default();
    let mut method = method_ir(
        method_name.clone(),
        derive_thrift_request_name(&params),
        if response_type == "void" {
            "Empty".to_string()
        } else {
            response_type
        },
        if oneway { "command" } else { "rpc" },
    );
    method.params = params;
    method.throws = throws;
    method.oneway = oneway;
    apply_thrift_method_annotation_pairs(&mut method, annotation_pairs);
    Some(method)
}

fn node_text<'a>(content: &'a str, node: Node) -> &'a str {
    &content[node.start_byte()..node.end_byte()]
}

fn visit_thrift_document_node(content: &str, node: Node, ir: &mut ContractIr) {
    // 统一遍历文档树，把不同 definition 节点分发到对应的 IR 收集逻辑。
    // 这里相当于 tree-sitter AST 到 `ContractIr` 的主调度器。
    match node.kind() {
        "document" | "header" | "definition" => {
            for index in 0..node.named_child_count() {
                let Some(child) = node.named_child(index) else {
                    continue;
                };
                visit_thrift_document_node(content, child, ir);
            }
        }
        "typedef" => {
            if let Some(alias) = parse_thrift_typedef_node(content, node) {
                ir.aliases.push(alias);
            }
        }
        "const" => {
            if let Some(constant) = parse_thrift_const_node(content, node) {
                ir.consts.push(constant);
            }
        }
        "enum" => {
            if let Some(enum_type) = parse_thrift_enum_node(content, node) {
                ir.enums.push(enum_type);
            }
        }
        "struct" => {
            if let Some(ty) = parse_thrift_type_node(content, node, ContractTypeKind::Struct) {
                ir.types.push(ty);
            }
        }
        "union" => {
            if let Some(ty) = parse_thrift_type_node(content, node, ContractTypeKind::Union) {
                ir.types.push(ty);
            }
        }
        "exception" => {
            if let Some(ty) = parse_thrift_type_node(content, node, ContractTypeKind::Exception) {
                ir.types.push(ty);
            }
        }
        "service" => {
            if let Some(service) = parse_thrift_service_node(content, node) {
                ir.services.push(service);
            }
        }
        _ => {}
    }
}

fn collect_thrift_include_nodes(content: &str, node: Node, includes: &mut Vec<String>) {
    // include 提取独立做一遍，是为了让递归解析可以先拿到依赖边，而不和正文实体解析耦在一起。
    match node.kind() {
        "document" | "header" => {
            for index in 0..node.named_child_count() {
                let Some(child) = node.named_child(index) else {
                    continue;
                };
                collect_thrift_include_nodes(content, child, includes);
            }
        }
        "include" => {
            if let Some(path) = parse_thrift_include_node(content, node) {
                includes.push(path);
            }
        }
        _ => {}
    }
}

fn trailing_thrift_comment(content: &str, node: Node) -> Option<String> {
    let tail = &content[node.end_byte()..];
    let line = tail.lines().next().unwrap_or("");
    let (_, comment) = split_thrift_comment(line);
    comment.map(|value| value.trim().to_string())
}

fn resolve_thrift_include_path(current_path: &Path, include_path: &str) -> Result<PathBuf> {
    let base_dir = current_path
        .parent()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| PathBuf::from("."));
    let resolved = base_dir.join(include_path);
    if resolved.exists() {
        return Ok(resolved);
    }
    Err(anyhow!(
        "failed to resolve thrift include {} from {}",
        include_path,
        current_path.display()
    ))
}

#[allow(dead_code)]
fn parse_thrift_params(input: &str) -> Vec<ContractField> {
    split_thrift_items(input)
        .into_iter()
        .filter_map(|item| parse_thrift_field(&item))
        .collect()
}

fn derive_thrift_request_name(params: &[ContractField]) -> String {
    if params.len() == 1 {
        params[0].ty.clone()
    } else {
        "Empty".to_string()
    }
}

#[allow(dead_code)]
fn split_trailing_annotation_block(segment: &str) -> (&str, Option<String>) {
    let trimmed = segment.trim();
    if !trimmed.ends_with(')') {
        return (trimmed, None);
    }
    let mut depth = 0usize;
    for (idx, ch) in trimmed.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let prefix = trimmed[..idx].trim();
                    let body = trimmed[idx + 1..trimmed.len() - 1].trim().to_string();
                    return (prefix, Some(body));
                }
            }
            _ => {}
        }
    }
    (trimmed, None)
}

fn apply_thrift_field_annotation_pairs(
    field: &mut ContractField,
    pairs: impl IntoIterator<Item = (String, String)>,
) {
    // 把 Thrift 注解里的 `api.*` 约定映射为内部 HTTP 绑定信息。
    // 这里不是通用注解透传，而是只吸收后续代码生成真正理解的那一小部分语义。
    for (key, value) in pairs {
        let Some(binding_source) = key.strip_prefix("api.") else {
            continue;
        };
        let normalized_source = match binding_source {
            "body" => "body",
            "query" => "query",
            "path" | "param" => "param",
            "header" => "header",
            "form" => "form",
            _ => continue,
        };
        let (name, rules) = parse_binding_value(&value);
        field.http_binding = Some(ContractHttpFieldBinding {
            source: normalized_source.to_string(),
            name: if name.is_empty() {
                field.name.clone()
            } else {
                name
            },
        });
        for rule in rules {
            if rule == "required" {
                field.required = true;
            }
            if !field.validation_rules.contains(&rule) {
                field.validation_rules.push(rule);
            }
        }
    }
}

#[allow(dead_code)]
fn apply_thrift_field_annotations(field: &mut ContractField, annotations: Option<&str>) {
    let Some(annotations) = annotations else {
        return;
    };
    apply_thrift_field_annotation_pairs(field, parse_annotation_pairs(annotations));
}

fn apply_thrift_method_annotation_pairs(
    method: &mut ContractMethod,
    pairs: impl IntoIterator<Item = (String, String)>,
) {
    for (key, value) in pairs {
        match key.as_str() {
            "api.get" => {
                method.http_method = Some("GET".into());
                method.http_path = Some(value);
                method.kind = "query".into();
            }
            "api.post" => {
                method.http_method = Some("POST".into());
                method.http_path = Some(value);
                method.kind = "command".into();
            }
            "api.put" => {
                method.http_method = Some("PUT".into());
                method.http_path = Some(value);
                method.kind = "command".into();
            }
            "api.delete" => {
                method.http_method = Some("DELETE".into());
                method.http_path = Some(value);
                method.kind = "command".into();
            }
            "api.patch" => {
                method.http_method = Some("PATCH".into());
                method.http_path = Some(value);
                method.kind = "command".into();
            }
            "api.head" => {
                method.http_method = Some("HEAD".into());
                method.http_path = Some(value);
                method.kind = "query".into();
            }
            "api.options" => {
                method.http_method = Some("OPTIONS".into());
                method.http_path = Some(value);
                method.kind = "query".into();
            }
            "api.handler_path" => method.http_handler_path = Some(value),
            "api.category" => method.http_category = Some(value),
            _ => {}
        }
    }
}

#[allow(dead_code)]
fn apply_thrift_method_annotations(method: &mut ContractMethod, annotations: Option<&str>) {
    let Some(annotations) = annotations else {
        return;
    };
    apply_thrift_method_annotation_pairs(method, parse_annotation_pairs(annotations));
}

#[allow(dead_code)]
fn parse_annotation_pairs(input: &str) -> Vec<(String, String)> {
    split_annotation_items(input)
        .into_iter()
        .filter_map(|item| {
            let (key, value) = item.split_once('=')?;
            Some((
                key.trim().to_string(),
                trim_quoted(value.trim()).to_string(),
            ))
        })
        .collect()
}

#[allow(dead_code)]
fn split_annotation_items(input: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                let item = current.trim();
                if !item.is_empty() {
                    items.push(item.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let item = current.trim();
    if !item.is_empty() {
        items.push(item.to_string());
    }
    items
}

fn split_thrift_items(input: &str) -> Vec<String> {
    // 按逗号拆分 Thrift 项，但要正确跳过容器泛型、注解参数、对象字面量和引号内容中的逗号。
    // 这是很多上层解析逻辑的基础工具，一旦拆错，field / annotation / const 都会跟着错位。
    let mut items = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            '(' if !in_quotes => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' if !in_quotes => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            '<' if !in_quotes => {
                angle_depth += 1;
                current.push(ch);
            }
            '>' if !in_quotes => {
                angle_depth = angle_depth.saturating_sub(1);
                current.push(ch);
            }
            '{' if !in_quotes => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' if !in_quotes => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if !in_quotes && paren_depth == 0 && angle_depth == 0 && brace_depth == 0 => {
                let item = current.trim();
                if !item.is_empty() {
                    items.push(item.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let item = current.trim();
    if !item.is_empty() {
        items.push(item.to_string());
    }
    items
}

fn trim_quoted(value: &str) -> &str {
    value.trim().trim_matches('"')
}

fn parse_binding_value(value: &str) -> (String, Vec<String>) {
    let mut parts = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let name = parts.next().unwrap_or_default().to_string();
    let rules = parts.map(ToOwned::to_owned).collect::<Vec<_>>();
    (name, rules)
}

#[allow(dead_code)]
fn parse_type_token(input: &str) -> Option<(String, &str)> {
    let mut end = 0usize;
    let mut angle_depth = 0usize;
    for (idx, ch) in input.char_indices() {
        match ch {
            '<' => {
                angle_depth += 1;
                end = idx + ch.len_utf8();
            }
            '>' => {
                angle_depth = angle_depth.saturating_sub(1);
                end = idx + ch.len_utf8();
            }
            ch if ch.is_whitespace() && angle_depth == 0 => {
                if idx == 0 {
                    continue;
                }
                return Some((input[..idx].trim().to_string(), input[idx..].trim()));
            }
            _ => end = idx + ch.len_utf8(),
        }
    }
    if end == 0 {
        None
    } else {
        Some((input[..end].trim().to_string(), input[end..].trim()))
    }
}

#[allow(dead_code)]
fn parse_field_name_and_default(input: &str) -> Option<(String, Option<String>)> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    if let Some((name, default_value)) = input.split_once('=') {
        return Some((
            base_identifier(name.trim()),
            Some(default_value.trim().trim_end_matches(',').to_string()),
        ));
    }
    Some((base_identifier(input), None))
}

fn parse_thrift_type_to_go(value: &str) -> String {
    let value = value.trim();
    if let Some(inner) = extract_container_inner(value, "list") {
        return format!("[]{}", sanitize_thrift_nested_type(inner));
    }
    if let Some(inner) = extract_container_inner(value, "set") {
        return format!("[]{}", sanitize_thrift_nested_type(inner));
    }
    if let Some(inner) = extract_container_inner(value, "map") {
        let parts = split_thrift_items(inner);
        if parts.len() == 2 {
            return format!(
                "map[{}]{}",
                sanitize_thrift_nested_type(parts[0].as_str()),
                sanitize_thrift_nested_type(parts[1].as_str())
            );
        }
    }
    let base = base_identifier(value);
    thrift_scalar_to_go(&base)
}

fn sanitize_thrift_nested_type(value: &str) -> String {
    parse_thrift_type_to_go(value)
}

fn extract_container_inner<'a>(value: &'a str, container: &str) -> Option<&'a str> {
    let prefix = format!("{container}<");
    let suffix = '>';
    let inner = value.strip_prefix(&prefix)?.strip_suffix(suffix)?;
    Some(inner.trim())
}

fn base_identifier(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('{')
        .trim_end_matches(',')
        .rsplit('.')
        .next()
        .unwrap_or(value)
        .trim()
        .to_string()
}

#[allow(dead_code)]
fn resolve_thrift_constants(ir: &mut ContractIr) {
    // 解析结束后再做一次常量回填，让字段默认值、service extends、方法元数据里的常量引用
    // 都尽量落成最终值，减少后续代码生成阶段再处理一轮符号替换。
    let const_map = ir
        .consts
        .iter()
        .map(|item| (item.name.clone(), item.value.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for ty in &mut ir.types {
        for field in &mut ty.fields {
            if let Some(default_value) = &field.default_value {
                if let Some(resolved) = const_map.get(default_value) {
                    field.default_value = Some(resolved.clone());
                }
            }
        }
    }
    for service in &mut ir.services {
        if let Some(parent) = service
            .extends
            .as_ref()
            .and_then(|name| const_map.get(name))
        {
            service.extends = Some(parent.clone());
        }
        for method in &mut service.methods {
            resolve_method_constant_refs(method, &const_map);
        }
    }
}

#[allow(dead_code)]
fn resolve_method_constant_refs(
    method: &mut ContractMethod,
    const_map: &std::collections::BTreeMap<String, String>,
) {
    for value in [
        &mut method.http_method,
        &mut method.http_path,
        &mut method.http_handler_path,
        &mut method.http_category,
        &mut method.gql_kind,
        &mut method.gql_field,
        &mut method.ws_event,
        &mut method.ws_namespace,
        &mut method.grpc_service,
        &mut method.grpc_method,
    ] {
        if let Some(current) = value.clone() {
            if let Some(resolved) = const_map.get(&current) {
                *value = Some(resolved.clone());
            }
        }
    }
}
