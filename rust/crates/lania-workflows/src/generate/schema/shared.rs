use crate::generate_types::ContractMethod;

pub(super) fn split_line_comment(line: &str) -> (&str, Option<&str>) {
    // 这里刻意保持规则简单且可预测：只支持单行 `//` 注释（不做多行/块注释解析）。
    if let Some((code, comment)) = line.split_once("//") {
        (code, Some(comment))
    } else {
        (line, None)
    }
}

pub(super) fn split_thrift_comment(line: &str) -> (&str, Option<&str>) {
    // Thrift 常见两种注释风格：`//` 与 `#`，这里两者都兼容。
    let (line, comment_slash) = split_line_comment(line);
    if comment_slash.is_some() {
        return (line, comment_slash);
    }
    if let Some((code, comment)) = line.split_once('#') {
        (code, Some(comment))
    } else {
        (line, None)
    }
}

pub(super) fn apply_lania_comment_overrides(method: &mut ContractMethod, comment: Option<&str>) {
    let Some(comment) = comment else { return };
    // 我们只读取 `lania:` 之后的部分（大小写不敏感），其它前缀内容全部忽略。
    let lower = comment.to_ascii_lowercase();
    let Some(pos) = lower.find("lania:") else {
        return;
    };
    let payload = comment[pos + "lania:".len()..].trim();
    if payload.is_empty() {
        return;
    }
    let mut parts = payload.split_whitespace();
    let kind = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    match kind.as_str() {
        // 示例：
        // - `// lania:http GET /:id`
        // - `// lania:http method=GET path=/:id`
        "http" => {
            let rest = parts.collect::<Vec<_>>();
            let mut http_method: Option<String> = None;
            let mut http_path: Option<String> = None;
            for item in &rest {
                if let Some(value) = item.strip_prefix("method=") {
                    http_method = Some(value.to_ascii_uppercase());
                }
                if let Some(value) = item.strip_prefix("path=") {
                    http_path = Some(value.to_string());
                }
            }
            if http_method.is_none() && !rest.is_empty() {
                http_method = Some(rest[0].to_ascii_uppercase());
            }
            if http_path.is_none() && rest.len() >= 2 {
                http_path = Some(rest[1].to_string());
            }
            if let Some(value) = http_method {
                method.http_method = Some(value.clone());
                // 尽力而为的默认规则：GET/HEAD/OPTIONS 这类动词默认视作 query。
                if matches!(value.as_str(), "GET" | "HEAD" | "OPTIONS") {
                    method.kind = "query".into();
                }
            }
            if let Some(value) = http_path {
                method.http_path = Some(value);
            }
        }
        // 示例：`// lania:ws namespace=/ws/user event=user.created`
        "ws" => {
            for item in parts {
                if let Some(value) = item.strip_prefix("namespace=") {
                    method.ws_namespace = Some(value.to_string());
                }
                if let Some(value) = item.strip_prefix("event=") {
                    method.ws_event = Some(value.to_string());
                    method.kind = "event".into();
                }
            }
        }
        // 示例：`// lania:grpc service=UserService method=GetUser`
        "grpc" => {
            for item in parts {
                if let Some(value) = item.strip_prefix("service=") {
                    method.grpc_service = Some(value.to_string());
                }
                if let Some(value) = item.strip_prefix("method=") {
                    method.grpc_method = Some(value.to_string());
                }
            }
        }
        // 示例：`// lania:graphql kind=query field=user`
        "graphql" => {
            for item in parts {
                if let Some(value) = item.strip_prefix("kind=") {
                    method.gql_kind = Some(value.to_string());
                }
                if let Some(value) = item.strip_prefix("field=") {
                    method.gql_field = Some(value.to_string());
                }
            }
        }
        // 示例：`// lania:kind query`
        "kind" => {
            if let Some(value) = parts.next() {
                method.kind = value.to_string();
            }
        }
        _ => {}
    }
}

pub(super) fn method_ir(
    name: impl Into<String>,
    request: impl Into<String>,
    response: impl Into<String>,
    kind: impl Into<String>,
) -> ContractMethod {
    // 小工具函数：避免在各类 schema parser 里反复手写一大段 `ContractMethod { ... }`。
    ContractMethod {
        name: name.into(),
        request: request.into(),
        response: response.into(),
        streaming: Default::default(),
        params: Vec::new(),
        throws: Vec::new(),
        oneway: false,
        kind: kind.into(),
        http_method: None,
        http_path: None,
        http_handler_path: None,
        http_category: None,
        gql_kind: None,
        gql_field: None,
        ws_event: None,
        ws_namespace: None,
        grpc_metadata: Default::default(),
        grpc_service: None,
        grpc_method: None,
    }
}

pub(crate) fn segmented_slug(value: &str, separator: &str) -> String {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut previous_is_lower_or_digit = false;
    for ch in value.chars() {
        if !ch.is_ascii_alphanumeric() {
            if !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
            previous_is_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() && previous_is_lower_or_digit && !current.is_empty() {
            parts.push(current.clone());
            current.clear();
        }
        current.push(ch.to_ascii_lowercase());
        previous_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts.join(separator)
}
