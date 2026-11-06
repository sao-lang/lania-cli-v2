use anyhow::{anyhow, Result};

pub(crate) fn normalize_source_kind(value: &str) -> Result<String> {
    // 这里先把各种别名归一化成内部标准值，
    // 这样后续逻辑只需要处理 `proto/thrift/json/graphql` 四种 canonical name。
    match value.trim().to_ascii_lowercase().as_str() {
        "proto" | "protobuf" => Ok("proto".into()),
        "thrift" => Ok("thrift".into()),
        // We treat YAML as a JSON-schema compatible input format (YAML is a superset of JSON).
        "json" | "json-schema" | "json_schema" | "yaml" | "yml" => Ok("json".into()),
        "graphql" | "graphql-schema" | "graphql_schema" | "gql" => Ok("graphql".into()),
        other => Err(anyhow!("unsupported source kind: {other}")),
    }
}

pub(crate) fn normalize_target_kind(value: &str) -> Result<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "grpc" => Ok("grpc".into()),
        "http" => Ok("http".into()),
        "ws" | "websocket" => Ok("ws".into()),
        "graphql" => Ok("graphql".into()),
        other => Err(anyhow!("unsupported target kind: {other}")),
    }
}

pub(crate) fn normalize_source_filter(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "protobuf" => "proto".into(),
        "json-schema" | "json_schema" | "yaml" | "yml" => "json".into(),
        "graphql-schema" | "graphql_schema" | "gql" => "graphql".into(),
        other => other.into(),
    }
}

pub(crate) fn normalize_target_filter(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "websocket" => "ws".into(),
        other => other.into(),
    }
}
