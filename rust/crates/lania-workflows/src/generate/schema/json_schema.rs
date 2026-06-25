use anyhow::{Context, Result};
use serde_json::Value;

use crate::generate_types::{
    ContractField, ContractIr, ContractMethod, ContractService, ContractType, ContractTypeKind,
};

use super::shared::method_ir;
use super::{exported_name, sanitize_go_type};

#[derive(Debug, serde::Deserialize)]
struct SchemaHttp {
    method: Option<String>,
    path: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SchemaWs {
    namespace: Option<String>,
    event: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SchemaGrpc {
    service: Option<String>,
    method: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SchemaGraphql {
    kind: Option<String>,
    field: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SchemaOperation {
    name: Option<String>,
    service: Option<String>,
    input: Option<String>,
    output: Option<String>,
    kind: Option<String>,
    http: Option<SchemaHttp>,
    ws: Option<SchemaWs>,
    grpc: Option<SchemaGrpc>,
    graphql: Option<SchemaGraphql>,
}

pub(crate) fn parse_json_schema_contract(content: &str) -> Result<ContractIr> {
    // JSON Schema 的特点是“通常只有类型定义，没有服务定义”，
    // 因此这里会产出 `types`，但 `services` 可能为空。
    let value: Value = serde_json::from_str(content).context("invalid json schema")?;
    let mut types = Vec::new();
    parse_json_schema_node(
        value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("RootSchema"),
        &value,
        &mut types,
    );
    if let Some(defs) = value.get("$defs").and_then(Value::as_object) {
        for (name, node) in defs {
            parse_json_schema_node(name, node, &mut types);
        }
    }
    if let Some(defs) = value.get("definitions").and_then(Value::as_object) {
        for (name, node) in defs {
            parse_json_schema_node(name, node, &mut types);
        }
    }
    types.sort_by(|left, right| left.name.cmp(&right.name));
    types.dedup_by(|left, right| left.name == right.name);

    let services = parse_schema_defined_operations(&value);
    Ok(ContractIr {
        types,
        services,
        ..ContractIr::default()
    })
}

pub(crate) fn parse_json_schema_node(name: &str, node: &Value, types: &mut Vec<ContractType>) {
    let properties = node.get("properties").and_then(Value::as_object);
    if let Some(properties) = properties {
        let mut fields = Vec::new();
        for (field_name, property) in properties {
            fields.push(ContractField {
                name: field_name.clone(),
                ty: json_schema_type_to_go(property),
                required: false,
                optional: false,
                oneof_group: None,
                default_value: None,
                http_binding: None,
                validation_rules: Vec::new(),
            });
        }
        types.push(ContractType {
            name: exported_name(name),
            kind: ContractTypeKind::Struct,
            fields,
        });
    }
}

pub(crate) fn json_schema_type_to_go(node: &Value) -> String {
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        return exported_name(reference.rsplit('/').next().unwrap_or("Any"));
    }
    if let Some(items) = node.get("items") {
        return format!("[]{}", sanitize_go_type(&json_schema_type_to_go(items)));
    }
    match node.get("type").and_then(Value::as_str).unwrap_or("object") {
        "string" => "string".into(),
        "boolean" => "bool".into(),
        "integer" => "int64".into(),
        "number" => "float64".into(),
        "array" => "[]any".into(),
        "object" => exported_name(
            node.get("title")
                .and_then(Value::as_str)
                .unwrap_or("Object"),
        ),
        _ => "any".into(),
    }
}

fn parse_schema_defined_operations(value: &Value) -> Vec<ContractService> {
    // 可选能力：允许在 schema 本身里定义 operations。
    // 这样 JSON/YAML 可以直接描述 WS/HTTP/GQL/GRPC 路由，而不一定非要借助
    // `lania.module.yaml` 的 overrides。
    let mut services = Vec::new();
    let ops_value = value
        .get("x-lania-operations")
        .or_else(|| value.get("x_lania_operations"));
    if let Some(ops_value) = ops_value {
        let mut operations = Vec::new();
        if let Some(items) = ops_value.as_array() {
            for item in items {
                if let Ok(op) = serde_json::from_value::<SchemaOperation>(item.clone()) {
                    operations.push(op);
                }
            }
        } else if let Some(map) = ops_value.as_object() {
            for (name, item) in map {
                if let Ok(mut op) = serde_json::from_value::<SchemaOperation>(item.clone()) {
                    if op.name.is_none() {
                        op.name = Some(name.clone());
                    }
                    operations.push(op);
                }
            }
        }
        for op in operations {
            let SchemaOperation {
                name,
                service,
                input,
                output,
                kind,
                http,
                ws,
                grpc,
                graphql,
            } = op;
            let name = name.unwrap_or_else(|| "Operation".into());
            let service_name = service.unwrap_or_else(|| "GeneratedService".into());
            let request = input.unwrap_or_else(|| format!("{}Input", exported_name(&name)));
            let response = output.unwrap_or_else(|| format!("{}Output", exported_name(&name)));
            let op = SchemaOperation {
                name: Some(name.clone()),
                service: Some(service_name.clone()),
                input: Some(request.clone()),
                output: Some(response.clone()),
                kind,
                http,
                ws,
                grpc,
                graphql,
            };
            let method = build_schema_operation_method(name.clone(), request, response, op);
            push_service_method(&mut services, service_name, method);
        }
    }
    services
}

fn build_schema_operation_method(
    name: String,
    request: String,
    response: String,
    op: SchemaOperation,
) -> ContractMethod {
    let mut method = method_ir(
        name,
        request,
        response,
        op.kind.unwrap_or_else(|| "rpc".into()),
    );
    if let Some(http) = op.http {
        if let Some(value) = http.method {
            method.http_method = Some(value.to_ascii_uppercase());
        }
        if let Some(value) = http.path {
            method.http_path = Some(value);
        }
    }
    if let Some(ws) = op.ws {
        method.kind = "event".into();
        if let Some(value) = ws.namespace {
            method.ws_namespace = Some(value);
        }
        if let Some(value) = ws.event {
            method.ws_event = Some(value);
        }
    }
    if let Some(grpc) = op.grpc {
        method.grpc_service = grpc.service;
        method.grpc_method = grpc.method;
    }
    if let Some(graphql) = op.graphql {
        method.gql_kind = graphql.kind;
        method.gql_field = graphql.field;
    }
    method
}

fn push_service_method(
    services: &mut Vec<ContractService>,
    service_name: String,
    method: ContractMethod,
) {
    if let Some(index) = services
        .iter()
        .position(|svc: &ContractService| svc.name == service_name)
    {
        services[index].methods.push(method);
    } else {
        services.push(ContractService {
            name: service_name,
            extends: None,
            grpc_metadata: Default::default(),
            methods: vec![method],
        });
    }
}
