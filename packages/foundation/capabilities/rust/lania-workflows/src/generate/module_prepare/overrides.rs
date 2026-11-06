use crate::generate_schema::exported_name;
use crate::generate_types::{
    ContractIr, ContractMethod, ContractService, ModuleOperationOverride, ModuleOverridesConfig,
};

// overrides 只在合同 IR 层工作。
// 不论原始输入来自 proto/thrift/json/graphql，这一层看到的都是统一的 ContractIr，
// 因而可以用同一套规则补充/改写 operation、service 与传输层元信息。
pub(super) fn apply_module_overrides(ir: &mut ContractIr, overrides: Option<&ModuleOverridesConfig>) {
    let Some(overrides) = overrides else {
        return;
    };
    // 如果 override 指向已有方法，就原地修改；
    // 如果方法不存在，则按 override 自动补出一个新的 service/method。
    for (operation_name, operation_override) in &overrides.operations {
        let mut applied = false;
        for service in &mut ir.services {
            for method in &mut service.methods {
                if method.name == *operation_name {
                    apply_operation_override(method, operation_override);
                    if let Some(service_name) = &operation_override.service {
                        service.name = service_name.clone();
                    }
                    applied = true;
                }
            }
        }
        if applied {
            continue;
        }
        let service_name = operation_override
            .service
            .clone()
            .unwrap_or_else(|| "GeneratedService".into());
        let service = find_or_create_service(&mut ir.services, &service_name);
        let mut method = ContractMethod {
            name: operation_name.clone(),
            request: operation_override
                .input
                .clone()
                .unwrap_or_else(|| format!("{}Input", exported_name(operation_name))),
            response: operation_override
                .output
                .clone()
                .unwrap_or_else(|| format!("{}Output", exported_name(operation_name))),
            streaming: Default::default(),
            params: Vec::new(),
            throws: Vec::new(),
            oneway: false,
            kind: operation_override
                .kind
                .clone()
                .unwrap_or_else(|| "rpc".into()),
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
        };
        apply_method_transport_overrides(&mut method, operation_override);
        service.methods.push(method);
    }
}

// 这里专注处理 operation 级别的通用字段改写，
// 更细分的 http/ws/graphql/grpc 传输层字段交给下面的 helper 处理。
fn apply_operation_override(
    method: &mut ContractMethod,
    operation_override: &ModuleOperationOverride,
) {
    if let Some(input) = &operation_override.input {
        method.request = input.clone();
    }
    if let Some(output) = &operation_override.output {
        method.response = output.clone();
    }
    if let Some(kind) = &operation_override.kind {
        method.kind = kind.clone();
    }
    apply_method_transport_overrides(method, operation_override);
}

// 传输层 override 被集中收口，避免不同调用点分别拼接 http/ws/graphql/grpc 字段。
fn apply_method_transport_overrides(
    method: &mut ContractMethod,
    operation_override: &ModuleOperationOverride,
) {
    if let Some(http) = &operation_override.http {
        if let Some(method_name) = &http.method {
            method.http_method = Some(method_name.to_ascii_uppercase());
        }
        if let Some(path) = &http.path {
            method.http_path = Some(path.clone());
        }
    }
    if let Some(ws) = &operation_override.ws {
        if let Some(event) = &ws.event {
            method.ws_event = Some(event.clone());
        }
        if let Some(namespace) = &ws.namespace {
            method.ws_namespace = Some(namespace.clone());
        }
    }
    if let Some(graphql) = &operation_override.graphql {
        if let Some(kind) = &graphql.kind {
            method.gql_kind = Some(kind.clone());
        }
        if let Some(field) = &graphql.field {
            method.gql_field = Some(field.clone());
        }
    }
    if let Some(grpc) = &operation_override.grpc {
        if let Some(service) = &grpc.service {
            method.grpc_service = Some(service.clone());
        }
        if let Some(name) = &grpc.method {
            method.grpc_method = Some(name.clone());
        }
    }
}

// 当 override 需要凭空新增一个 operation 时，先确保 service 存在。
// 这里把“查找或创建 service”的可变借用细节封装掉，避免主流程过于嘈杂。
fn find_or_create_service<'a>(
    services: &'a mut Vec<ContractService>,
    service_name: &str,
) -> &'a mut ContractService {
    if let Some(index) = services
        .iter()
        .position(|service| service.name == service_name)
    {
        return &mut services[index];
    }
    services.push(ContractService {
        name: service_name.to_string(),
        extends: None,
        grpc_metadata: Default::default(),
        methods: Vec::new(),
    });
    let index = services.len() - 1;
    &mut services[index]
}
