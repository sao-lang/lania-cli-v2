use anyhow::Result;

use crate::generate_types::{
    ContractField, ContractIr, ContractMethod, ContractService, ContractType, ContractTypeKind,
};

use super::exported_name;
use super::shared::{method_ir, segmented_slug};

pub(crate) fn parse_graphql_schema_contract(content: &str) -> Result<ContractIr> {
    // GraphQL 相比 proto/thrift 更特别：
    // - `Query/Mutation/Subscription` 会被解释成 service methods
    // - 其它 `type/input/enum` 会进入类型系统
    let mut types = Vec::new();
    let mut services = vec![ContractService {
        name: "GraphqlService".into(),
        extends: None,
        grpc_metadata: Default::default(),
        methods: Vec::new(),
    }];
    let mut current_type: Option<(String, Vec<ContractField>)> = None;
    let mut current_root: Option<String> = None;
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line
            .strip_prefix("type ")
            .and_then(|value| value.strip_suffix('{').or(Some(value)))
        {
            let name = name.trim().trim_end_matches('{').trim().to_string();
            if matches!(name.as_str(), "Query" | "Mutation" | "Subscription") {
                current_root = Some(name);
                current_type = None;
            } else {
                current_type = Some((name, Vec::new()));
                current_root = None;
            }
            continue;
        }
        if let Some(name) = line
            .strip_prefix("input ")
            .and_then(|value| value.strip_suffix('{').or(Some(value)))
        {
            current_type = Some((
                name.trim().trim_end_matches('{').trim().to_string(),
                Vec::new(),
            ));
            current_root = None;
            continue;
        }
        if let Some(name) = line
            .strip_prefix("enum ")
            .and_then(|value| value.strip_suffix('{').or(Some(value)))
        {
            current_type = Some((
                name.trim().trim_end_matches('{').trim().to_string(),
                Vec::new(),
            ));
            current_root = None;
            continue;
        }
        if line == "}" {
            if let Some((name, fields)) = current_type.take() {
                types.push(ContractType {
                    name,
                    kind: ContractTypeKind::Struct,
                    fields,
                });
            }
            current_root = None;
            continue;
        }
        if let Some(root_kind) = &current_root {
            if let Some((method, input_type)) = parse_graphql_operation(line, root_kind) {
                if let Some(input_type) = input_type {
                    types.push(input_type);
                }
                services[0].methods.push(method);
            }
            continue;
        }
        if let Some((_, fields)) = current_type.as_mut() {
            if let Some(field) = parse_graphql_field(line) {
                fields.push(field);
            }
        }
    }
    if let Some((name, fields)) = current_type.take() {
        types.push(ContractType {
            name,
            kind: ContractTypeKind::Struct,
            fields,
        });
    }
    if services[0].methods.is_empty() {
        services.clear();
    }
    Ok(ContractIr {
        types,
        services,
        ..ContractIr::default()
    })
}

pub(crate) fn parse_graphql_field(line: &str) -> Option<ContractField> {
    let segment = line.trim_end_matches(',');
    if segment.contains('(') {
        return None;
    }
    let (name, ty) = segment.split_once(':')?;
    Some(ContractField {
        name: name.trim().to_string(),
        ty: graphql_type_to_go(ty),
        required: false,
        optional: false,
        oneof_group: None,
        default_value: None,
        http_binding: None,
        validation_rules: Vec::new(),
    })
}

pub(crate) fn parse_graphql_operation(
    line: &str,
    root_kind: &str,
) -> Option<(ContractMethod, Option<ContractType>)> {
    // GraphQL 的参数会被“提升”为一个输入类型，
    // 这样后续生成代码时可以和 proto/thrift 的 request type 处理方式保持一致。
    let segment = line.trim_end_matches(',');
    let colon_index = segment.rfind(':')?;
    let left = segment[..colon_index].trim();
    let response = graphql_type_name(&segment[colon_index + 1..]);
    let (name, request, input_type) = if let Some(open) = left.find('(') {
        let close = left.rfind(')')?;
        let name = left[..open].trim().to_string();
        let args = left[open + 1..close].trim();
        if args.is_empty() {
            (name, "Empty".to_string(), None)
        } else {
            let input_name = format!("{}Input", exported_name(&name));
            let fields = args
                .split(',')
                .filter_map(parse_graphql_argument_field)
                .collect::<Vec<_>>();
            (
                name,
                input_name.clone(),
                Some(ContractType {
                    name: input_name,
                    kind: ContractTypeKind::Struct,
                    fields,
                }),
            )
        }
    } else {
        (left.to_string(), "Empty".to_string(), None)
    };
    let mut method = method_ir(
        &name,
        request,
        response,
        match root_kind {
            "Query" => "query",
            "Mutation" => "command",
            "Subscription" => "subscription",
            _ => "rpc",
        },
    );
    method.gql_kind = Some(match root_kind {
        "Query" => "query".into(),
        "Mutation" => "mutation".into(),
        "Subscription" => "subscription".into(),
        _ => "query".into(),
    });
    method.gql_field = Some(name.clone());
    if root_kind == "Subscription" {
        method.ws_event = Some(segmented_slug(&name, "."));
    }
    Some((method, input_type))
}

pub(crate) fn parse_graphql_argument_field(value: &str) -> Option<ContractField> {
    let (name, ty) = value.split_once(':')?;
    Some(ContractField {
        name: name.trim().to_string(),
        ty: graphql_type_to_go(ty),
        required: false,
        optional: false,
        oneof_group: None,
        default_value: None,
        http_binding: None,
        validation_rules: Vec::new(),
    })
}

pub(crate) fn graphql_type_to_go(value: &str) -> String {
    let name = graphql_type_name(value);
    match name.as_str() {
        "String" | "ID" => "string".into(),
        "Boolean" => "bool".into(),
        "Int" => "int64".into(),
        "Float" => "float64".into(),
        "Empty" => "any".into(),
        other => exported_name(other),
    }
}

pub(crate) fn graphql_type_name(value: &str) -> String {
    value
        .trim()
        .trim_matches('!')
        .trim_matches('[')
        .trim_matches(']')
        .trim_matches('!')
        .trim()
        .to_string()
}
