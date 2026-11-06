//! 负责把 Protobuf schema 解析成内部统一使用的 `ContractIr`。
//!
//! 这里同时维护两条解析路径：
//! - 主路径：使用 `protox` 编译 descriptor set，获得更完整、更可靠的语义信息；
//! - 回退路径：直接按源码文本做最小解析，主要用于缺少完整 proto 上下文的轻量场景。
//!
//! 这个模块真正重要的不是“把语法读出来”，而是把 protobuf 世界里的命名、oneof、
//! map_entry、streaming 和 options 这些语义稳定地投影到内部 IR 上。
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use prost_types::{
    field_descriptor_proto::{Label, Type},
    method_options::IdempotencyLevel,
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto, MethodOptions,
    ServiceOptions, UninterpretedOption,
};

use crate::generate_types::{
    ContractEnum, ContractEnumVariant, ContractField, ContractGrpcMetadata, ContractIr,
    ContractService, ContractStreamingMode, ContractType, ContractTypeKind,
};

use super::shared::{apply_lania_comment_overrides, method_ir, split_line_comment};
use super::{exported_name, proto_scalar_to_go, strip_inline_comment};

#[derive(Debug, Clone)]
enum ProtoSymbol {
    Message(ProtoMessageMeta),
    Enum(String),
}

#[derive(Debug, Clone)]
struct ProtoMessageMeta {
    resolved_name: String,
    map_entry: bool,
    fields: Vec<FieldDescriptorProto>,
}

pub(crate) fn parse_proto_contract(content: &str) -> Result<ContractIr> {
    // 保留一个无 import 解析能力的 fallback，便于仍然以“纯字符串”方式做最小解析。
    // 真正的完整语法主链路走 `parse_proto_contract_from_files(...)`。
    let mut types = Vec::new();
    let mut services = Vec::new();
    let mut current_message: Option<ContractType> = None;
    let mut current_service: Option<ContractService> = None;

    for raw_line in content.lines() {
        let (code, comment) = split_line_comment(raw_line);
        let line = strip_inline_comment(code).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line
            .strip_prefix("message ")
            .and_then(|value| value.strip_suffix('{').or(Some(value)))
        {
            if let Some(message) = current_message.take() {
                types.push(message);
            }
            current_message = Some(ContractType {
                name: name.trim().trim_end_matches('{').trim().to_string(),
                kind: ContractTypeKind::Struct,
                fields: Vec::new(),
            });
            continue;
        }
        if let Some(name) = line
            .strip_prefix("service ")
            .and_then(|value| value.strip_suffix('{').or(Some(value)))
        {
            if let Some(message) = current_message.take() {
                types.push(message);
            }
            if let Some(service) = current_service.take() {
                services.push(service);
            }
            current_service = Some(ContractService {
                name: name.trim().trim_end_matches('{').trim().to_string(),
                extends: None,
                grpc_metadata: Default::default(),
                methods: Vec::new(),
            });
            continue;
        }
        if line == "}" {
            if let Some(message) = current_message.take() {
                types.push(message);
                continue;
            }
            if let Some(service) = current_service.take() {
                services.push(service);
                continue;
            }
        }
        if let Some(message) = current_message.as_mut() {
            if let Some(field) = parse_proto_field(line) {
                message.fields.push(field);
            }
            continue;
        }
        if let Some(service) = current_service.as_mut() {
            if let Some(mut method) = parse_proto_method(line) {
                apply_lania_comment_overrides(&mut method, comment);
                service.methods.push(method);
            }
        }
    }
    if let Some(message) = current_message.take() {
        types.push(message);
    }
    if let Some(service) = current_service.take() {
        services.push(service);
    }

    Ok(ContractIr {
        types,
        services,
        ..ContractIr::default()
    })
}

/// 从一组 `.proto` 文件构建完整的 `ContractIr`。
///
/// 这是 protobuf 解析的主链路：
/// 1. 规范化 include 目录和输入路径；
/// 2. 用 `protox` 编译出 descriptor set；
/// 3. 基于 descriptor 构造符号表，解决重名和嵌套类型问题；
/// 4. 统一转换为 enums / types / services。
///
/// 如果 `protox` 编译失败，且输入看起来更像老式的“无 syntax 声明文本 proto”，
/// 会回退到 legacy 纯文本解析器，尽量保留兼容性。
pub(crate) fn parse_proto_contract_from_files(
    input_paths: &[PathBuf],
    include_paths: &[PathBuf],
) -> Result<ContractIr> {
    if input_paths.is_empty() {
        return Ok(ContractIr::default());
    }

    let include_paths = normalize_include_paths(include_paths, input_paths)?;
    let inputs = input_paths
        .iter()
        .map(|path| relativize_proto_path(path, &include_paths))
        .collect::<Result<Vec<_>>>()?;

    let descriptor_set = match protox::compile(&inputs, &include_paths) {
        Ok(descriptor_set) => descriptor_set,
        Err(err) => {
            if should_fallback_to_legacy_proto_parser(input_paths)? {
                return parse_proto_contract_legacy_from_files(input_paths);
            }
            return Err(err).context("failed to compile protobuf schema with protox");
        }
    };
    let symbols = build_proto_symbol_table(&descriptor_set.file);

    let mut ir = ContractIr::default();
    for file in &descriptor_set.file {
        collect_file_enums(file, &symbols, &mut ir.enums, &[]);
        collect_file_messages(file, &symbols, &mut ir.types, &[]);
        ir.services.extend(convert_proto_services(file, &symbols));
    }
    dedupe_contract_ir(&mut ir);
    Ok(ir)
}

pub(crate) fn parse_proto_field(line: &str) -> Option<ContractField> {
    let segment = line.trim_end_matches(';');
    let before_equals = segment.split('=').next()?.trim();
    let tokens = before_equals.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }
    let (ty, name, optional) = if tokens[0] == "repeated" && tokens.len() >= 3 {
        (
            format!("[]{}", proto_scalar_to_go(tokens[1])),
            tokens[2].to_string(),
            true,
        )
    } else {
        (
            proto_scalar_to_go(tokens[tokens.len() - 2]),
            tokens[tokens.len() - 1].to_string(),
            false,
        )
    };
    Some(ContractField {
        name,
        ty,
        required: false,
        optional,
        oneof_group: None,
        default_value: None,
        http_binding: None,
        validation_rules: Vec::new(),
    })
}

pub(crate) fn parse_proto_method(line: &str) -> Option<crate::generate_types::ContractMethod> {
    let segment = line.trim_end_matches(';').trim();
    let segment = segment.strip_prefix("rpc ")?;
    let name_end = segment.find('(')?;
    let name = segment[..name_end].trim().to_string();
    let request_end = segment.find(')')?;
    let request = segment[name_end + 1..request_end].trim().to_string();
    let returns_marker = "returns (";
    let returns_start = segment.find(returns_marker)? + returns_marker.len();
    let returns_end = segment[returns_start..].find(')')? + returns_start;
    let response = segment[returns_start..returns_end].trim().to_string();
    let mut method = method_ir(
        name,
        request.trim_start_matches("stream ").to_string(),
        response.trim_start_matches("stream ").to_string(),
        "rpc",
    );
    method.streaming = match (
        request.starts_with("stream "),
        response.starts_with("stream "),
    ) {
        (true, true) => ContractStreamingMode::Bidi,
        (true, false) => ContractStreamingMode::Client,
        (false, true) => ContractStreamingMode::Server,
        (false, false) => ContractStreamingMode::Unary,
    };
    Some(method)
}

fn normalize_include_paths(
    include_paths: &[PathBuf],
    input_paths: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let mut seen = BTreeSet::<PathBuf>::new();
    let mut out = Vec::new();
    for include in include_paths {
        let absolute = std::fs::canonicalize(include).unwrap_or_else(|_| include.clone());
        if seen.insert(absolute.clone()) {
            out.push(absolute);
        }
    }
    for input in input_paths {
        let parent = input.parent().ok_or_else(|| {
            anyhow!(
                "protobuf input has no parent directory: {}",
                input.display()
            )
        })?;
        let absolute = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        if seen.insert(absolute.clone()) {
            out.push(absolute);
        }
    }
    Ok(out)
}

fn relativize_proto_path(path: &Path, include_paths: &[PathBuf]) -> Result<String> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    for include in include_paths {
        if let Ok(relative) = canonical.strip_prefix(include) {
            return Ok(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Err(anyhow!(
        "protobuf input {} is not under any include directory",
        path.display()
    ))
}

fn build_proto_symbol_table(files: &[FileDescriptorProto]) -> BTreeMap<String, ProtoSymbol> {
    // 先收集“期望名称”出现次数，再决定是否需要用完整路径展开名称。
    // 这样既能保留短名称的可读性，又能在重名时稳定消歧。
    let mut desired_counts = BTreeMap::<String, usize>::new();
    let mut raw_symbols = Vec::<(String, String, bool, Vec<FieldDescriptorProto>)>::new();

    for file in files {
        let package = file.package.as_deref().unwrap_or("");
        for message in &file.message_type {
            collect_proto_message_symbols(
                package,
                message,
                &[],
                &mut desired_counts,
                &mut raw_symbols,
            );
        }
        for enum_type in &file.enum_type {
            collect_proto_enum_symbols(
                package,
                enum_type,
                &[],
                &mut desired_counts,
                &mut raw_symbols,
            );
        }
    }

    let mut symbols = BTreeMap::<String, ProtoSymbol>::new();
    for (full_name, desired_name, map_entry, fields) in raw_symbols {
        let resolved_name = if desired_counts
            .get(&desired_name)
            .copied()
            .unwrap_or_default()
            > 1
        {
            let normalized = full_name.trim_start_matches('.').replace('.', "_");
            exported_name(&normalized)
        } else {
            desired_name
        };
        let symbol = if fields.is_empty() {
            ProtoSymbol::Enum(resolved_name)
        } else {
            ProtoSymbol::Message(ProtoMessageMeta {
                resolved_name,
                map_entry,
                fields,
            })
        };
        symbols.insert(full_name, symbol);
    }
    symbols
}

fn collect_proto_message_symbols(
    package: &str,
    message: &DescriptorProto,
    parent_segments: &[String],
    desired_counts: &mut BTreeMap<String, usize>,
    out: &mut Vec<(String, String, bool, Vec<FieldDescriptorProto>)>,
) {
    let Some(name) = message.name.as_deref() else {
        return;
    };
    let mut segments = parent_segments.to_vec();
    segments.push(name.to_string());
    let desired_name = segments.join("");
    *desired_counts.entry(desired_name.clone()).or_default() += 1;
    let full_name = proto_full_name(package, &segments);
    out.push((
        full_name,
        desired_name,
        message
            .options
            .as_ref()
            .and_then(|options| options.map_entry)
            .unwrap_or(false),
        message.field.clone(),
    ));

    for nested in &message.nested_type {
        collect_proto_message_symbols(package, nested, &segments, desired_counts, out);
    }
    for enum_type in &message.enum_type {
        collect_proto_enum_symbols(package, enum_type, &segments, desired_counts, out);
    }
}

fn collect_proto_enum_symbols(
    package: &str,
    enum_type: &EnumDescriptorProto,
    parent_segments: &[String],
    desired_counts: &mut BTreeMap<String, usize>,
    out: &mut Vec<(String, String, bool, Vec<FieldDescriptorProto>)>,
) {
    let Some(name) = enum_type.name.as_deref() else {
        return;
    };
    let mut segments = parent_segments.to_vec();
    segments.push(name.to_string());
    let desired_name = segments.join("");
    *desired_counts.entry(desired_name.clone()).or_default() += 1;
    let full_name = proto_full_name(package, &segments);
    out.push((full_name, desired_name, false, Vec::new()));
}

fn proto_full_name(package: &str, segments: &[String]) -> String {
    if package.trim().is_empty() {
        format!(".{}", segments.join("."))
    } else {
        format!(".{}.{}", package, segments.join("."))
    }
}

fn collect_file_enums(
    file: &FileDescriptorProto,
    symbols: &BTreeMap<String, ProtoSymbol>,
    out: &mut Vec<ContractEnum>,
    parent_segments: &[String],
) {
    let package = file.package.as_deref().unwrap_or("");
    for enum_type in &file.enum_type {
        if let Some(item) = convert_proto_enum(package, enum_type, parent_segments, symbols) {
            out.push(item);
        }
    }
    for message in &file.message_type {
        collect_nested_enums_from_message(package, message, symbols, out, parent_segments);
    }
}

fn collect_nested_enums_from_message(
    package: &str,
    message: &DescriptorProto,
    symbols: &BTreeMap<String, ProtoSymbol>,
    out: &mut Vec<ContractEnum>,
    parent_segments: &[String],
) {
    let Some(name) = message.name.as_deref() else {
        return;
    };
    let mut segments = parent_segments.to_vec();
    segments.push(name.to_string());
    for enum_type in &message.enum_type {
        if let Some(item) = convert_proto_enum(package, enum_type, &segments, symbols) {
            out.push(item);
        }
    }
    for nested in &message.nested_type {
        collect_nested_enums_from_message(package, nested, symbols, out, &segments);
    }
}

fn collect_file_messages(
    file: &FileDescriptorProto,
    symbols: &BTreeMap<String, ProtoSymbol>,
    out: &mut Vec<ContractType>,
    parent_segments: &[String],
) {
    let package = file.package.as_deref().unwrap_or("");
    for message in &file.message_type {
        collect_proto_message_types(package, message, parent_segments, symbols, out);
    }
}

fn collect_proto_message_types(
    package: &str,
    message: &DescriptorProto,
    parent_segments: &[String],
    symbols: &BTreeMap<String, ProtoSymbol>,
    out: &mut Vec<ContractType>,
) {
    // map_entry 只是 protobuf 为 map 语法生成的中间消息，不应该暴露成业务可见的结构体类型。
    let Some(name) = message.name.as_deref() else {
        return;
    };
    let mut segments = parent_segments.to_vec();
    segments.push(name.to_string());
    let full_name = proto_full_name(package, &segments);
    let Some(ProtoSymbol::Message(meta)) = symbols.get(&full_name) else {
        return;
    };
    if meta.map_entry {
        return;
    }

    let oneof_names = message
        .oneof_decl
        .iter()
        .map(|item| item.name.clone().unwrap_or_default())
        .collect::<Vec<_>>();
    let fields = message
        .field
        .iter()
        .map(|field| convert_proto_field(field, &oneof_names, symbols))
        .collect::<Vec<_>>();
    out.push(ContractType {
        name: meta.resolved_name.clone(),
        kind: ContractTypeKind::Struct,
        fields,
    });

    for nested in &message.nested_type {
        collect_proto_message_types(package, nested, &segments, symbols, out);
    }
}

fn convert_proto_enum(
    package: &str,
    enum_type: &EnumDescriptorProto,
    parent_segments: &[String],
    symbols: &BTreeMap<String, ProtoSymbol>,
) -> Option<ContractEnum> {
    let name = enum_type.name.as_deref()?;
    let mut segments = parent_segments.to_vec();
    segments.push(name.to_string());
    let full_name = proto_full_name(package, &segments);
    let resolved_name = match symbols.get(&full_name) {
        Some(ProtoSymbol::Enum(name)) => name.clone(),
        _ => segments.join(""),
    };
    Some(ContractEnum {
        name: resolved_name,
        variants: enum_type
            .value
            .iter()
            .map(|value| ContractEnumVariant {
                name: value.name.clone().unwrap_or_default(),
                value: value.number.map(|number| number.to_string()),
            })
            .collect(),
    })
}

fn convert_proto_field(
    field: &FieldDescriptorProto,
    oneof_names: &[String],
    symbols: &BTreeMap<String, ProtoSymbol>,
) -> ContractField {
    // protobuf 的字段语义会在这里被压缩成内部统一模型：
    // - `required/optional/repeated` 映射到 required/optional/数组
    // - proto3 optional 保留为 optional，但不把它误判成 oneof
    // - oneof 字段通过 oneof_group 记录分组关系，后续可用于代码生成校验
    let label = field
        .label
        .and_then(|value| Label::try_from(value).ok())
        .unwrap_or(Label::Optional);
    let required = label == Label::Required;
    let explicit_optional = field.proto3_optional.unwrap_or(false);
    let optional = label == Label::Optional || explicit_optional;
    let oneof_group = field
        .oneof_index
        .filter(|_| !explicit_optional)
        .and_then(|index| oneof_names.get(index as usize))
        .cloned()
        .filter(|value| !value.is_empty());

    let mut validation_rules = Vec::new();
    if required {
        validation_rules.push("required".to_string());
    }

    ContractField {
        name: field.name.clone().unwrap_or_default(),
        ty: proto_field_go_type(field, symbols),
        required,
        optional,
        oneof_group,
        default_value: field.default_value.clone(),
        http_binding: None,
        validation_rules,
    }
}

fn proto_field_go_type(
    field: &FieldDescriptorProto,
    symbols: &BTreeMap<String, ProtoSymbol>,
) -> String {
    // map 在 descriptor 里会表现为指向一个 `map_entry` message 的字段，这里需要把它恢复成
    // 业务侧真正期望看到的 `map[K]V` 形式。
    if let Some(type_name) = field.type_name.as_deref() {
        if let Some(ProtoSymbol::Message(meta)) = symbols.get(type_name) {
            if meta.map_entry && meta.fields.len() >= 2 {
                let key_type = proto_field_base_go_type(&meta.fields[0], symbols);
                let value_type = proto_field_base_go_type(&meta.fields[1], symbols);
                return format!("map[{key_type}]{value_type}");
            }
        }
    }

    let base = proto_field_base_go_type(field, symbols);
    let label = field
        .label
        .and_then(|value| Label::try_from(value).ok())
        .unwrap_or(Label::Optional);
    if label == Label::Repeated {
        format!("[]{base}")
    } else {
        base
    }
}

fn proto_field_base_go_type(
    field: &FieldDescriptorProto,
    symbols: &BTreeMap<String, ProtoSymbol>,
) -> String {
    // 这里负责把 protobuf 字段类型压成最终的 Go 风格基础类型名。
    // 对 message / enum 会优先走符号表，以保证嵌套类型和重名类型拿到稳定的解析结果。
    match field
        .r#type
        .and_then(|value| Type::try_from(value).ok())
        .unwrap_or(Type::String)
    {
        Type::Double => "float64".into(),
        Type::Float => "float32".into(),
        Type::Int64 | Type::Sint64 | Type::Sfixed64 => "int64".into(),
        Type::Uint64 | Type::Fixed64 => "uint64".into(),
        Type::Int32 | Type::Sint32 | Type::Sfixed32 => "int32".into(),
        Type::Uint32 | Type::Fixed32 => "uint32".into(),
        Type::Bool => "bool".into(),
        Type::String => "string".into(),
        Type::Bytes => "[]byte".into(),
        Type::Enum | Type::Message | Type::Group => {
            if let Some(type_name) = field.type_name.as_deref() {
                if let Some(symbol) = symbols.get(type_name) {
                    return match symbol {
                        ProtoSymbol::Message(meta) => meta.resolved_name.clone(),
                        ProtoSymbol::Enum(name) => name.clone(),
                    };
                }
                return exported_name(type_name.rsplit('.').next().unwrap_or(type_name));
            }
            "any".into()
        }
    }
}

fn convert_proto_services(
    file: &FileDescriptorProto,
    symbols: &BTreeMap<String, ProtoSymbol>,
) -> Vec<ContractService> {
    file.service
        .iter()
        .map(|service| ContractService {
            name: service.name.clone().unwrap_or_default(),
            extends: None,
            grpc_metadata: parse_service_grpc_metadata(service.options.as_ref()),
            methods: service
                .method
                .iter()
                .map(|method| {
                    let request = resolve_proto_type_name(method.input_type.as_deref(), symbols);
                    let response = resolve_proto_type_name(method.output_type.as_deref(), symbols);
                    let mut item = method_ir(
                        method.name.clone().unwrap_or_default(),
                        request,
                        response,
                        "rpc",
                    );
                    item.streaming = match (
                        method.client_streaming.unwrap_or(false),
                        method.server_streaming.unwrap_or(false),
                    ) {
                        (true, true) => ContractStreamingMode::Bidi,
                        (true, false) => ContractStreamingMode::Client,
                        (false, true) => ContractStreamingMode::Server,
                        (false, false) => ContractStreamingMode::Unary,
                    };
                    item.grpc_metadata = parse_method_grpc_metadata(method.options.as_ref());
                    item
                })
                .collect(),
        })
        .collect()
}

fn resolve_proto_type_name(
    type_name: Option<&str>,
    symbols: &BTreeMap<String, ProtoSymbol>,
) -> String {
    let Some(type_name) = type_name else {
        return "Empty".into();
    };
    if let Some(symbol) = symbols.get(type_name) {
        return match symbol {
            ProtoSymbol::Message(meta) => meta.resolved_name.clone(),
            ProtoSymbol::Enum(name) => name.clone(),
        };
    }
    exported_name(type_name.rsplit('.').next().unwrap_or(type_name))
}

fn dedupe_contract_ir(ir: &mut ContractIr) {
    let mut seen_enums = BTreeSet::<String>::new();
    ir.enums.retain(|item| seen_enums.insert(item.name.clone()));

    let mut seen_types = BTreeSet::<String>::new();
    ir.types.retain(|item| seen_types.insert(item.name.clone()));

    let mut seen_services = BTreeSet::<String>::new();
    ir.services
        .retain(|item| seen_services.insert(item.name.clone()));
}

fn parse_service_grpc_metadata(options: Option<&ServiceOptions>) -> ContractGrpcMetadata {
    let Some(options) = options else {
        return ContractGrpcMetadata::default();
    };
    ContractGrpcMetadata {
        deprecated: options.deprecated.unwrap_or(false),
        idempotency_level: None,
        options: parse_uninterpreted_options(&options.uninterpreted_option),
    }
}

fn parse_method_grpc_metadata(options: Option<&MethodOptions>) -> ContractGrpcMetadata {
    let Some(options) = options else {
        return ContractGrpcMetadata::default();
    };
    ContractGrpcMetadata {
        deprecated: options.deprecated.unwrap_or(false),
        idempotency_level: options
            .idempotency_level
            .and_then(|value| IdempotencyLevel::try_from(value).ok())
            .map(|value| value.as_str_name().to_string()),
        options: parse_uninterpreted_options(&options.uninterpreted_option),
    }
}

fn parse_uninterpreted_options(options: &[UninterpretedOption]) -> BTreeMap<String, String> {
    let mut parsed = BTreeMap::new();
    for option in options {
        let Some(name) = uninterpreted_option_name(option) else {
            continue;
        };
        let Some(value) = uninterpreted_option_value(option) else {
            continue;
        };
        parsed.insert(name, value);
    }
    parsed
}

fn uninterpreted_option_name(option: &UninterpretedOption) -> Option<String> {
    if option.name.is_empty() {
        return None;
    }
    let mut segments = Vec::with_capacity(option.name.len());
    for part in &option.name {
        if part.name_part.is_empty() {
            continue;
        }
        if part.is_extension {
            segments.push(format!("({})", part.name_part));
        } else {
            segments.push(part.name_part.clone());
        }
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    }
}

fn uninterpreted_option_value(option: &UninterpretedOption) -> Option<String> {
    option
        .identifier_value
        .clone()
        .or_else(|| option.positive_int_value.map(|value| value.to_string()))
        .or_else(|| option.negative_int_value.map(|value| value.to_string()))
        .or_else(|| option.double_value.map(|value| value.to_string()))
        .or_else(|| {
            option
                .string_value
                .as_ref()
                .map(|value| String::from_utf8_lossy(value).into_owned())
        })
        .or_else(|| option.aggregate_value.clone())
}

fn should_fallback_to_legacy_proto_parser(input_paths: &[PathBuf]) -> Result<bool> {
    // 一个很保守的启发式判断：
    // 如果所有输入都看不到 `syntax =`，通常说明它们更像旧式或不完整的 proto 文本，
    // 此时允许回退到 legacy parser，尽量避免直接把兼容场景判死。
    for input_path in input_paths {
        let content = std::fs::read_to_string(input_path)
            .with_context(|| format!("failed to read source schema {}", input_path.display()))?;
        if content
            .lines()
            .map(str::trim)
            .any(|line| line.starts_with("syntax ="))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn parse_proto_contract_legacy_from_files(input_paths: &[PathBuf]) -> Result<ContractIr> {
    let mut ir = ContractIr::default();
    for input_path in input_paths {
        let content = std::fs::read_to_string(input_path)
            .with_context(|| format!("failed to read source schema {}", input_path.display()))?;
        let parsed = parse_proto_contract(&content)?;
        ir.enums.extend(parsed.enums);
        ir.types.extend(parsed.types);
        ir.services.extend(parsed.services);
    }
    dedupe_contract_ir(&mut ir);
    Ok(ir)
}
