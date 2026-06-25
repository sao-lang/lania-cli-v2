//! 根据模板与上下文渲染模块文件集合（generate-module 侧）。
//!
//! 这里的“渲染”是纯函数式的：输入 `CompiledModuleEntry`，输出一组 `GeneratedContractPlan`。
//! - plan 只包含 `{path, content, owner}`，不会直接写磁盘；
//! - 写盘和 hook 由 workflow 层统一处理。
//!
//! 新手重点关注：
//! - 为什么把不同 target 的输出拆成独立文件（更稳定、更易增量 diff）
//! - `owner` 字段如何用于 scoped clean / 只清理某个 entry 的产物
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::{anyhow, Result};

use crate::generate_schema::{exported_name, sanitize_go_type, segmented_slug, slugify};
use crate::generate_types::{
    CompiledModuleEntry, ContractField, ContractMethod, ContractService, ContractStreamingMode,
    ContractType, ContractTypeKind, GeneratedContractPlan, ModuleOutputPaths,
};

pub(crate) fn render_module_entry(
    entry: &CompiledModuleEntry,
    output: &ModuleOutputPaths,
) -> Result<Vec<GeneratedContractPlan>> {
    // 这里是 module 渲染链路的总入口：
    // - 根据 entry 判断当前模块应该走普通多目标输出、单 HTTP 输出还是单 gRPC 输出；
    // - 产出的是“待写盘计划”，而不是直接写文件；
    // - 每个 plan 都会带上 owner，供后续 scoped clean 和冲突诊断使用。
    if is_single_http_output_entry(entry) {
        return render_http_single_module_plans(entry, output);
    }
    if is_single_grpc_output_entry(entry) {
        return render_grpc_single_module_plans(entry, output);
    }
    // 一个模块最少生成两类文件：
    // 1) contracts：暴露 schema / DTO 定义
    // 2) modules：暴露模块元信息，供注册表聚合
    let mut plans = vec![
        GeneratedContractPlan {
            path: output
                .contract_dir
                .join(format!("{}.contract.gen.go", entry.slug)),
            content: render_module_contract_go(entry),
            // owner 用于把输出归属到某个 entry，便于 scoped 清理与冲突诊断。
            owner: Some(entry.name.clone()),
        },
        GeneratedContractPlan {
            path: output
                .module_dir
                .join(format!("{}_module.gen.go", entry.slug)),
            content: render_module_go(entry),
            owner: Some(entry.name.clone()),
        },
    ];
    for target in &entry.targets {
        if target == "grpc" {
            plans.extend(render_grpc_adapter_plans(entry, output));
            continue;
        }
        let target_dir = if target == "grpc" {
            grpc_output_root_dir(output)
        } else {
            output.adapter_dir.join(target)
        };
        // 每个 target 再补一份 adapter 声明，让 HTTP / gRPC / WS / GraphQL
        // 的导出约定都能落到独立文件，便于按协议接入。
        plans.push(GeneratedContractPlan {
            path: target_dir.join(format!("{}_{}.gen.go", entry.slug, target)),
            content: render_module_adapter_go(entry, target)?,
            owner: Some(entry.name.clone()),
        });
        // 除了声明列表，还会额外生成一份 lania-g DSL 接线脚手架：
        // 它刻意对齐 `cmd/*-demo` 风格，最终形态类似
        // `Controller/Service/Gateway/Resolver(...).Build()`。
        plans.push(GeneratedContractPlan {
            path: target_dir.join(format!("{}_{}_dsl.gen.go", entry.slug, target)),
            content: render_module_adapter_dsl_go(entry, target)?,
            owner: Some(entry.name.clone()),
        });
    }
    Ok(plans)
}

fn grpc_output_root_dir(output: &ModuleOutputPaths) -> std::path::PathBuf {
    output
        .grpc_root_dir
        .clone()
        .unwrap_or_else(|| output.adapter_dir.join("grpc"))
}

fn grpc_bootstrap_path(entry: &CompiledModuleEntry, output: &ModuleOutputPaths) -> std::path::PathBuf {
    output
        .grpc_root_dir
        .as_ref()
        .map(|root| root.join("bootstrap.gen.go"))
        .unwrap_or_else(|| {
            output
                .adapter_dir
                .join("grpc")
                .join("demo")
                .join(&entry.slug)
                .join("bootstrap.gen.go")
        })
}

fn render_grpc_single_module_plans(
    entry: &CompiledModuleEntry,
    output: &ModuleOutputPaths,
) -> Result<Vec<GeneratedContractPlan>> {
    Ok(render_grpc_adapter_plans(entry, output))
}

fn render_grpc_adapter_plans(
    entry: &CompiledModuleEntry,
    output: &ModuleOutputPaths,
) -> Vec<GeneratedContractPlan> {
    let grpc_root_dir = grpc_output_root_dir(output);
    let mut plans = Vec::new();
    for service in &entry.ir.services {
        let methods = collect_service_methods(entry, service);
        let service_dir = grpc_root_dir.join(grpc_service_dir_name(service));
        plans.push(GeneratedContractPlan {
            path: service_dir.join("dto.gen.go"),
            content: render_grpc_group_types_go(entry, service),
            owner: Some(entry.name.clone()),
        });
        plans.push(GeneratedContractPlan {
            path: service_dir.join("metadata.gen.go"),
            content: render_grpc_group_metadata_go(entry, service, &methods),
            owner: Some(entry.name.clone()),
        });
        plans.push(GeneratedContractPlan {
            path: service_dir.join("errors.gen.go"),
            content: render_grpc_group_error_helpers_go(service),
            owner: Some(entry.name.clone()),
        });
        plans.push(GeneratedContractPlan {
            path: service_dir.join("register.gen.go"),
            content: render_grpc_group_register_go(entry, service, &methods),
            owner: Some(entry.name.clone()),
        });
        for method in methods {
            plans.push(GeneratedContractPlan {
                path: service_dir.join(format!("{}.gen.go", grpc_method_file_name(method))),
                content: render_grpc_group_method_go(entry, service, method),
                owner: Some(entry.name.clone()),
            });
        }
    }
    plans.push(GeneratedContractPlan {
        path: grpc_bootstrap_path(entry, output),
        content: render_grpc_demo_main_go(entry, output),
        owner: Some(entry.name.clone()),
    });
    plans
}

fn render_http_single_module_plans(
    entry: &CompiledModuleEntry,
    output: &ModuleOutputPaths,
) -> Result<Vec<GeneratedContractPlan>> {
    let http_root_dir = http_single_entry_root_dir(output);
    let mut plans = Vec::new();
    for (group, methods) in collect_http_controller_groups(entry) {
        let group_dir = http_root_dir.join(group.trim_matches('/'));
        plans.push(GeneratedContractPlan {
            path: group_dir.join("dto.gen.go"),
            content: render_http_group_types_go(entry, &group),
            owner: Some(entry.name.clone()),
        });
        plans.push(GeneratedContractPlan {
            path: group_dir.join("errors.gen.go"),
            content: render_http_group_error_helpers_go(entry, &group),
            owner: Some(entry.name.clone()),
        });
        plans.push(GeneratedContractPlan {
            path: group_dir.join("envelope.gen.go"),
            content: render_http_group_envelope_go(&group),
            owner: Some(entry.name.clone()),
        });
        plans.push(GeneratedContractPlan {
            path: group_dir.join("register.gen.go"),
            content: render_http_group_register_go(entry, &group, &methods),
            owner: Some(entry.name.clone()),
        });
        for (service, method) in methods {
            plans.push(GeneratedContractPlan {
                path: group_dir.join(format!("{}.gen.go", http_method_file_name(method))),
                content: render_http_group_method_go(entry, service, method, &group)?,
                owner: Some(entry.name.clone()),
            });
        }
    }
    plans.push(GeneratedContractPlan {
        path: http_bootstrap_path(entry, output),
        content: render_http_demo_main_go(entry, output),
        owner: Some(entry.name.clone()),
    });
    Ok(plans)
}

fn http_single_entry_root_dir(output: &ModuleOutputPaths) -> std::path::PathBuf {
    output
        .http_root_dir
        .clone()
        .unwrap_or_else(|| output.adapter_dir.join("http"))
}

fn http_bootstrap_path(entry: &CompiledModuleEntry, output: &ModuleOutputPaths) -> std::path::PathBuf {
    output
        .http_root_dir
        .as_ref()
        .map(|root| root.join("bootstrap.gen.go"))
        .unwrap_or_else(|| {
            output
                .adapter_dir
                .join("http")
                .join("demo")
                .join(&entry.slug)
                .join("bootstrap.gen.go")
        })
}

pub(crate) fn is_single_http_output_entry(entry: &CompiledModuleEntry) -> bool {
    entry.targets.len() == 1 && entry.targets.iter().all(|target| target == "http")
}

pub(crate) fn is_single_grpc_output_entry(entry: &CompiledModuleEntry) -> bool {
    entry.targets.len() == 1 && entry.targets.iter().all(|target| target == "grpc")
}

pub(crate) fn render_module_contract_go(entry: &CompiledModuleEntry) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str("package contracts\n\n");
    body.push_str(&format!(
        "const {}SchemaSource = \"{}\"\n\n",
        exported_name(&entry.name),
        entry.source_kind
    ));
    if entry
        .ir
        .services
        .iter()
        .flat_map(|service| service.methods.iter())
        .any(|method| method.request == "Empty" || method.response == "Empty")
    {
        // 仅在 schema 真正使用 Empty 时才生成，避免每个文件都带一份无用类型。
        body.push_str("type Empty struct {}\n\n");
    }
    for ty in &entry.ir.types {
        body.push_str(&format!("type {} struct {{\n", exported_name(&ty.name)));
        for field in &ty.fields {
            let json_name = field
                .http_binding
                .as_ref()
                .map(|binding| binding.name.as_str())
                .unwrap_or(field.name.as_str());
            body.push_str(&format!(
                "    {} {} `json:\"{}\"`\n",
                go_field_name(&field.name),
                sanitize_go_type(&field.ty),
                json_name
            ));
        }
        body.push_str("}\n\n");
    }
    body
}

pub(crate) fn render_module_go(entry: &CompiledModuleEntry) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str("package modules\n\n");
    body.push_str(&format!(
        "const {}Name = \"{}\"\n\n",
        entry.module_name, entry.name
    ));
    body.push_str(&format!(
        "func Build{}() map[string]any {{\n",
        entry.module_name
    ));
    body.push_str("    return map[string]any{\n");
    body.push_str(&format!("        \"name\": {}Name,\n", entry.module_name));
    body.push_str(&format!("        \"source\": \"{}\",\n", entry.source_kind));
    body.push_str("        \"targets\": []string{\n");
    for target in &entry.targets {
        body.push_str(&format!("            \"{target}\",\n"));
    }
    body.push_str("        },\n");
    body.push_str("    }\n");
    body.push_str("}\n");
    body
}

pub(crate) fn render_module_adapter_go(
    entry: &CompiledModuleEntry,
    target: &str,
) -> Result<String> {
    if target == "grpc" {
        return Ok(render_grpc_single_module_go(entry));
    }
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {target}\n\n"));
    let declaration_name = format!(
        "{}{}Declarations",
        exported_name(&entry.name),
        exported_name(target)
    );
    body.push_str(&format!("var {declaration_name} = []string{{\n"));
    for service in &entry.ir.services {
        for method in &service.methods {
            // 这里输出的是“协议声明字符串”，不是具体 handler 实现。
            // 生成物的职责是把 schema 中的方法与外部协议名对齐，供后续注册/检查使用。
            let declaration = match target {
                "grpc" => format!(
                    "{}.{} => grpc:{}",
                    exported_name(&service.name),
                    exported_name(&method.name),
                    method
                        .grpc_method
                        .clone()
                        .unwrap_or_else(|| exported_name(&method.name))
                ),
                "http" => format!(
                    "{} {} => {}.{}",
                    method
                        .http_method
                        .clone()
                        .unwrap_or_else(|| default_http_method(method)),
                    method
                        .http_path
                        .clone()
                        .unwrap_or_else(|| default_http_path(service, method)),
                    exported_name(&service.name),
                    exported_name(&method.name)
                ),
                "ws" => format!(
                    "{} => {}.{}",
                    method
                        .ws_event
                        .clone()
                        .unwrap_or_else(|| default_ws_event(service, method)),
                    exported_name(&service.name),
                    exported_name(&method.name)
                ),
                "graphql" => format!(
                    "{} {} => {}.{}",
                    method
                        .gql_kind
                        .clone()
                        .unwrap_or_else(|| default_graphql_kind(method)),
                    method
                        .gql_field
                        .clone()
                        .unwrap_or_else(|| slugify(&method.name).replace('_', "")),
                    exported_name(&service.name),
                    exported_name(&method.name)
                ),
                other => return Err(anyhow!("unsupported target kind: {other}")),
            };
            body.push_str(&format!("    {:?},\n", declaration));
        }
    }
    body.push_str("}\n");
    Ok(body)
}

pub(crate) fn render_module_adapter_dsl_go(
    entry: &CompiledModuleEntry,
    target: &str,
) -> Result<String> {
    if target == "grpc" {
        return Ok(render_grpc_declarations_go(entry));
    }
    // 这个文件不是“纯文档”生成物，而是要真正参与编译的扩展点：
    // - 生成 receiver stub（类型壳），用户可以在同 package 的其它文件里实现方法
    // - 生成 registration helper，把 DSL 声明写入 adapter registry
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {target}\n\n"));
    body.push_str("import (\n");
    body.push_str("    \"errors\"\n");
    match target {
        // 注意：导入路径里的 package 名可能与当前 package 冲突，因此统一显式起别名。
        "http" => {
            body.push_str("    httpadapter \"lania-g/v3/adapter/http\"\n");
            body.push_str(
                "    httpbinding \"github.com/sao-lang/lania-g/protocol/http/v3/binding\"\n",
            );
        }
        "grpc" => {
            body.push_str("    grpcadapter \"lania-g/v3/adapter/grpc\"\n");
            body.push_str(
                "    grpcbinding \"github.com/sao-lang/lania-g/protocol/grpc/v3/binding\"\n",
            );
            body.push_str("    \"google.golang.org/protobuf/types/known/structpb\"\n");
        }
        "ws" => body.push_str("    wsadapter \"lania-g/v3/adapter/ws\"\n"),
        "graphql" => body.push_str("    graphqladapter \"lania-g/v3/adapter/graphql\"\n"),
        other => return Err(anyhow!("unsupported target kind: {other}")),
    };
    body.push_str(")\n\n");

    if target == "grpc" {
        body.push_str(&render_grpc_shared_types(entry));
    }

    // 每个 service 生成一个独立 receiver，避免不同 service 之间的方法名冲突。
    for service in &entry.ir.services {
        match target {
            "http" => {
                let mut groups = BTreeMap::<String, Vec<&ContractMethod>>::new();
                for method in &service.methods {
                    groups
                        .entry(http_group_name(service, method))
                        .or_default()
                        .push(method);
                }
                for (group, methods) in groups {
                    let controller_type = http_controller_type_name(&group);
                    let controller_base = http_controller_base_name(&group);
                    body.push_str(&format!(
                        "// {controller_type} is a generated HTTP controller for group `{group}`.\n"
                    ));
                    body.push_str(
                        "// Fill in method bodies in this file or move the implementation to a non-generated file later.\n",
                    );
                    body.push_str(&format!("type {controller_type} struct {{}}\n\n"));

                    for method in &methods {
                        if let Some(body_request) =
                            render_http_request_type(entry, service, method)?
                        {
                            body.push_str(&body_request);
                        }
                        if let Some(args_type) = render_http_args_type(entry, service, method)? {
                            body.push_str(&args_type);
                        }
                    }

                    let register_fn = format!(
                        "Register{}{}Http",
                        exported_name(&entry.name),
                        exported_name(&group)
                    );
                    let controller_prefix = format!("/{}", group.trim_matches('/'));
                    body.push_str(&format!(
                        "func {register_fn}(api *httpadapter.API, controller *{controller_type}) {{\n"
                    ));
                    body.push_str(
                        "    if api == nil || controller == nil {\n        return\n    }\n",
                    );
                    body.push_str(&format!(
                        "    b := api.Controller(\"{controller_prefix}\", controller)\n"
                    ));
                    for method in &methods {
                        let builder_call = http_builder_call(method);
                        let relative_path = http_relative_path(service, method, &group);
                        let method_name = http_controller_method_name(method, &controller_base);
                        body.push_str(&format!(
                            "    b.{builder_call}({relative_path:?}, controller.{method_name})\n"
                        ));
                    }
                    body.push_str("    b.Build()\n}\n\n");

                    for method in &methods {
                        body.push_str(&render_http_controller_method(
                            entry,
                            service,
                            method,
                            &controller_type,
                            &controller_base,
                        )?);
                    }
                }
            }
            "grpc" => {
                let receiver_type = format!(
                    "{}{}{}Receiver",
                    exported_name(&entry.name),
                    exported_name(&service.name),
                    exported_name(target)
                );

                body.push_str(&format!(
                    "// {receiver_type} is a generated receiver stub for `{}` over `{}`.\n",
                    service.name, target
                ));
                body.push_str(
                    "// Implement these methods in a separate (non-generated) file in the same package.\n",
                );
                body.push_str(&format!("type {receiver_type} struct {{}}\n\n"));

                let service_name = service
                    .methods
                    .iter()
                    .find_map(|m| m.grpc_service.clone())
                    .unwrap_or_else(|| exported_name(&service.name));
                let register_fn = format!(
                    "Register{}{}Grpc",
                    exported_name(&entry.name),
                    exported_name(&service.name)
                );
                body.push_str(&format!(
                    "func {register_fn}(api *grpcadapter.API, receiver *{receiver_type}) {{\n"
                ));
                body.push_str("    if api == nil || receiver == nil {\n        return\n    }\n");
                body.push_str(&format!(
                    "    b := api.Service(\"{service_name}\", receiver)\n"
                ));
                for method in &service.methods {
                    let method_name = method
                        .grpc_method
                        .clone()
                        .unwrap_or_else(|| exported_name(&method.name));
                    let builder_call = grpc_builder_call(method);
                    body.push_str(&format!(
                        "    b.{builder_call}(\"{method_name}\", receiver.{})",
                        exported_name(&method.name)
                    ));
                    if matches!(
                        method.streaming,
                        ContractStreamingMode::Unary | ContractStreamingMode::Server
                    ) {
                        body.push_str(".WithReqType((*structpb.Struct)(nil))");
                    }
                    body.push('\n');
                }
                body.push_str("    b.Build()\n}\n\n");

                for method in &service.methods {
                    body.push_str(&render_grpc_receiver_method(entry, method, &receiver_type));
                }
            }
            "ws" => {
                let receiver_type = format!(
                    "{}{}{}Receiver",
                    exported_name(&entry.name),
                    exported_name(&service.name),
                    exported_name(target)
                );

                body.push_str(&format!(
                    "// {receiver_type} is a generated receiver stub for `{}` over `{}`.\n",
                    service.name, target
                ));
                body.push_str(
                    "// Implement these methods in a separate (non-generated) file in the same package.\n",
                );
                body.push_str(&format!("type {receiver_type} struct {{}}\n\n"));

                let namespace = service
                    .methods
                    .iter()
                    .find_map(|m| m.ws_namespace.clone())
                    .unwrap_or_else(|| format!("/ws/{}", slugify(&service.name).replace('_', "-")));
                let register_fn = format!(
                    "Register{}{}Ws",
                    exported_name(&entry.name),
                    exported_name(&service.name)
                );
                body.push_str(&format!(
                    "func {register_fn}(api *wsadapter.API, gateway *{receiver_type}) {{\n"
                ));
                body.push_str("    if api == nil || gateway == nil {\n        return\n    }\n");
                body.push_str(&format!("    b := api.Gateway(\"{namespace}\", gateway)\n"));
                for method in &service.methods {
                    let event = method
                        .ws_event
                        .clone()
                        .unwrap_or_else(|| default_ws_event(service, method));
                    body.push_str(&format!(
                        "    b.On(\"{event}\", gateway.{})\n",
                        exported_name(&method.name)
                    ));
                }
                body.push_str("    b.Build()\n}\n\n");

                for method in &service.methods {
                    body.push_str(&format!(
                        "func (r *{receiver_type}) {}(args any) (any, error) {{\n    _ = args\n    return nil, errors.New(\"TODO\")\n}}\n\n",
                        exported_name(&method.name)
                    ));
                }
            }
            "graphql" => {
                let receiver_type = format!(
                    "{}{}{}Receiver",
                    exported_name(&entry.name),
                    exported_name(&service.name),
                    exported_name(target)
                );

                body.push_str(&format!(
                    "// {receiver_type} is a generated receiver stub for `{}` over `{}`.\n",
                    service.name, target
                ));
                body.push_str(
                    "// Implement these methods in a separate (non-generated) file in the same package.\n",
                );
                body.push_str(&format!("type {receiver_type} struct {{}}\n\n"));

                let resolver_name = exported_name(&service.name);
                let register_fn = format!(
                    "Register{}{}Graphql",
                    exported_name(&entry.name),
                    exported_name(&service.name)
                );
                body.push_str(&format!(
                    "func {register_fn}(api *graphqladapter.API, resolver *{receiver_type}) {{\n"
                ));
                body.push_str("    if api == nil || resolver == nil {\n        return\n    }\n");
                if service.methods.is_empty() {
                    body.push_str(&format!(
                        "    api.Resolver(\"{resolver_name}\", resolver).Build()\n"
                    ));
                    body.push_str("}\n\n");
                    continue;
                }
                // GraphQL DSL 的链式调用发生在 FieldBuilder 上：
                // - 它会重新暴露 Query/Mutation/Subscription
                // - `Build()` 返回的是 resolver definitions，而不是 builder 自身
                body.push_str(&format!(
                    "    api.Resolver(\"{resolver_name}\", resolver).\n"
                ));
                for method in &service.methods {
                    let gql_kind = method
                        .gql_kind
                        .clone()
                        .unwrap_or_else(|| default_graphql_kind(method));
                    let field = method
                        .gql_field
                        .clone()
                        .unwrap_or_else(|| slugify(&method.name).replace('_', ""));
                    let call = match gql_kind.to_ascii_lowercase().as_str() {
                        "mutation" => "Mutation",
                        "subscription" => "Subscription",
                        _ => "Query",
                    };
                    // `.Returns(...)` 虽然是可选的，但对 schema 推断与文档生成更友好。
                    body.push_str(&format!(
                        "        {call}(\"{field}\", resolver.{}).Returns({:?}).\n",
                        exported_name(&method.name),
                        sanitize_go_type(&method.response),
                    ));
                }
                body.push_str("        Build()\n");
                body.push_str("}\n\n");
                for method in &service.methods {
                    body.push_str(&format!(
                        "func (r *{receiver_type}) {}(args any) (any, error) {{\n    _ = args\n    return nil, errors.New(\"TODO\")\n}}\n\n",
                        exported_name(&method.name)
                    ));
                }
            }
            _ => unreachable!(),
        }
    }

    Ok(body)
}

#[allow(dead_code)]
fn render_http_single_module_go(entry: &CompiledModuleEntry) -> Result<String> {
    let has_exception_types = entry
        .ir
        .types
        .iter()
        .any(|ty| ty.kind == ContractTypeKind::Exception);
    let has_throws = entry
        .ir
        .services
        .iter()
        .flat_map(|service| collect_service_methods(entry, service))
        .any(|method| !method.throws.is_empty());
    let has_oneway = entry
        .ir
        .services
        .iter()
        .flat_map(|service| collect_service_methods(entry, service))
        .any(|method| method.oneway);
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str("package http\n\n");
    body.push_str("import (\n");
    body.push_str("    \"errors\"\n");
    if has_exception_types {
        body.push_str("    \"fmt\"\n");
    }
    if has_exception_types || has_throws || has_oneway {
        body.push_str("    \"net/http\"\n");
    }
    body.push_str("    httpbinding \"github.com/sao-lang/lania-g/protocol/http/v3/binding\"\n");
    body.push_str("    httpadapter \"lania-g/v3/adapter/http\"\n");
    body.push_str(")\n\n");

    if entry
        .ir
        .services
        .iter()
        .flat_map(|service| service.methods.iter())
        .any(|method| method.request == "Empty" || method.response == "Empty")
    {
        body.push_str("type Empty struct {}\n\n");
    }

    for alias in &entry.ir.aliases {
        body.push_str(&format!(
            "type {} = {}\n\n",
            exported_name(&alias.name),
            sanitize_go_type(&alias.target)
        ));
    }
    if !entry.ir.consts.is_empty() {
        body.push_str("const (\n");
        for item in &entry.ir.consts {
            body.push_str(&format!(
                "    {} {} = {}\n",
                go_field_name(&item.name),
                sanitize_go_type(&item.ty),
                render_go_literal(&item.ty, &item.value)
            ));
        }
        body.push_str(")\n\n");
    }
    for enum_type in &entry.ir.enums {
        let enum_name = exported_name(&enum_type.name);
        body.push_str(&format!("type {enum_name} int32\n\n"));
        body.push_str("const (\n");
        let mut next_value = 0i32;
        for variant in &enum_type.variants {
            let value = variant
                .value
                .as_deref()
                .and_then(|raw| raw.parse::<i32>().ok())
                .unwrap_or(next_value);
            body.push_str(&format!(
                "    {}{} {} = {}\n",
                exported_name(&enum_type.name),
                go_field_name(&variant.name),
                enum_name,
                value
            ));
            next_value = value + 1;
        }
        body.push_str(")\n\n");
    }

    for ty in &entry.ir.types {
        body.push_str(&render_http_model_type(ty));
        if ty.kind == ContractTypeKind::Union {
            body.push_str(&render_http_union_validator(ty));
        }
        if ty.kind == ContractTypeKind::Exception {
            body.push_str(&render_http_exception_error_method(ty));
        }
    }
    if has_throws {
        body.push_str(&render_http_error_status_helper(entry));
    }

    for (group, methods) in collect_http_controller_groups(entry) {
        let controller_type = http_controller_type_name(&group);
        let controller_base = http_controller_base_name(&group);
        body.push_str(&format!(
            "// {controller_type} is a generated HTTP controller for group `{group}`.\n"
        ));
        body.push_str(
            "// Fill in method bodies in this file or move the implementation to a non-generated file later.\n",
        );
        body.push_str(&format!("type {controller_type} struct {{}}\n\n"));

        for (service, method) in &methods {
            if let Some(body_request) = render_http_request_type(entry, service, method)? {
                body.push_str(&body_request);
            }
            if let Some(args_type) = render_http_args_type(entry, service, method)? {
                body.push_str(&args_type);
            }
        }

        let register_fn = format!(
            "Register{}{}Http",
            exported_name(&entry.name),
            exported_name(&group)
        );
        let controller_prefix = format!("/{}", group.trim_matches('/'));
        body.push_str(&format!(
            "func {register_fn}(api *httpadapter.API, controller *{controller_type}) {{\n"
        ));
        body.push_str("    if api == nil || controller == nil {\n        return\n    }\n");
        body.push_str(&format!(
            "    b := api.Controller(\"{controller_prefix}\", controller)\n"
        ));
        for (service, method) in &methods {
            let builder_call = http_builder_call(method);
            let relative_path = http_relative_path(service, method, &group);
            let method_name = http_controller_method_name(method, &controller_base);
            body.push_str(&format!(
                "    b.{builder_call}({relative_path:?}, controller.{method_name})\n"
            ));
        }
        body.push_str("    b.Build()\n}\n\n");

        for (service, method) in &methods {
            body.push_str(&render_http_controller_method(
                entry,
                service,
                method,
                &controller_type,
                &controller_base,
            )?);
        }
    }

    Ok(body)
}

fn render_http_group_types_go(entry: &CompiledModuleEntry, group: &str) -> String {
    let has_exception_types = entry
        .ir
        .types
        .iter()
        .any(|ty| ty.kind == ContractTypeKind::Exception);
    let has_union_types = entry
        .ir
        .types
        .iter()
        .any(|ty| ty.kind == ContractTypeKind::Union);
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", http_group_package_name(group)));
    if has_exception_types || has_union_types {
        body.push_str("import (\n");
        if has_union_types {
            body.push_str("    \"errors\"\n");
        }
        if has_exception_types {
            body.push_str("    \"fmt\"\n");
        }
        body.push_str(")\n\n");
    }

    if entry
        .ir
        .services
        .iter()
        .flat_map(|service| service.methods.iter())
        .any(|method| method.request == "Empty" || method.response == "Empty")
    {
        body.push_str("type Empty struct {}\n\n");
    }
    for alias in &entry.ir.aliases {
        body.push_str(&format!(
            "type {} = {}\n\n",
            exported_name(&alias.name),
            sanitize_go_type(&alias.target)
        ));
    }
    if !entry.ir.consts.is_empty() {
        body.push_str("const (\n");
        for item in &entry.ir.consts {
            body.push_str(&format!(
                "    {} {} = {}\n",
                go_field_name(&item.name),
                sanitize_go_type(&item.ty),
                render_go_literal(&item.ty, &item.value)
            ));
        }
        body.push_str(")\n\n");
    }
    for enum_type in &entry.ir.enums {
        let enum_name = exported_name(&enum_type.name);
        body.push_str(&format!("type {enum_name} int32\n\n"));
        body.push_str("const (\n");
        let mut next_value = 0i32;
        for variant in &enum_type.variants {
            let value = variant
                .value
                .as_deref()
                .and_then(|raw| raw.parse::<i32>().ok())
                .unwrap_or(next_value);
            body.push_str(&format!(
                "    {}{} {} = {}\n",
                exported_name(&enum_type.name),
                go_field_name(&variant.name),
                enum_name,
                value
            ));
            next_value = value + 1;
        }
        body.push_str(")\n\n");
    }
    for ty in &entry.ir.types {
        body.push_str(&render_http_model_type(ty));
        if ty.kind == ContractTypeKind::Union {
            body.push_str(&render_http_union_validator(ty));
        }
        if ty.kind == ContractTypeKind::Exception {
            body.push_str(&render_http_exception_error_method(ty));
        }
    }
    body
}

fn render_http_group_error_helpers_go(entry: &CompiledModuleEntry, group: &str) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", http_group_package_name(group)));
    body.push_str("import (\n");
    body.push_str("    \"net/http\"\n");
    body.push_str(")\n\n");
    body.push_str(&render_http_error_status_helper(entry));
    body.push_str("func HTTPErrorCodeFromError(err error) int32 {\n");
    body.push_str("    if err == nil {\n        return 0\n    }\n");
    body.push_str("    return int32(HTTPStatusFromError(err))\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPErrorEnvelopeFromError(err error) HTTPEnvelope[any] {\n");
    body.push_str("    if err == nil {\n        return HTTPSuccessEnvelope[any](nil)\n    }\n");
    body.push_str("    return HTTPEnvelope[any]{\n");
    body.push_str("        Code:    HTTPErrorCodeFromError(err),\n");
    body.push_str("        Message: httpErrorMessage(err),\n");
    body.push_str("        Success: false,\n");
    body.push_str("    }\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPWriteError(err error) (int, HTTPEnvelope[any]) {\n");
    body.push_str(
        "    if err == nil {\n        return http.StatusOK, HTTPSuccessEnvelope[any](nil)\n    }\n",
    );
    body.push_str("    return HTTPStatusFromError(err), HTTPErrorEnvelopeFromError(err)\n");
    body.push_str("}\n\n");
    body.push_str("func httpErrorMessage(err error) string {\n");
    body.push_str("    if err == nil {\n        return \"\"\n    }\n");
    body.push_str("    return err.Error()\n");
    body.push_str("}\n");
    body
}

fn render_http_group_envelope_go(group: &str) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", http_group_package_name(group)));
    body.push_str("type HTTPEnvelope[T any] struct {\n");
    body.push_str("    Code    int32  `json:\"code\"`\n");
    body.push_str("    Message string `json:\"msg\"`\n");
    body.push_str("    Data    T      `json:\"data,omitempty\"`\n");
    body.push_str("    Success bool   `json:\"success\"`\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPSuccessEnvelope[T any](data T) HTTPEnvelope[T] {\n");
    body.push_str("    return HTTPEnvelope[T]{\n");
    body.push_str("        Code:    0,\n");
    body.push_str("        Message: \"ok\",\n");
    body.push_str("        Data:    data,\n");
    body.push_str("        Success: true,\n");
    body.push_str("    }\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPFailureEnvelope(message string) HTTPEnvelope[any] {\n");
    body.push_str("    return HTTPEnvelope[any]{\n");
    body.push_str("        Code:    1,\n");
    body.push_str("        Message: message,\n");
    body.push_str("        Success: false,\n");
    body.push_str("    }\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPEnvelopeFromResult[T any](data T, err error) HTTPEnvelope[T] {\n");
    body.push_str("    if err != nil {\n");
    body.push_str("        return HTTPEnvelope[T]{\n");
    body.push_str("            Code:    int32(HTTPStatusFromError(err)),\n");
    body.push_str("            Message: httpErrorMessage(err),\n");
    body.push_str("            Success: false,\n");
    body.push_str("        }\n");
    body.push_str("    }\n");
    body.push_str("    return HTTPSuccessEnvelope(data)\n");
    body.push_str("}\n");
    body
}

fn render_http_group_register_go(
    entry: &CompiledModuleEntry,
    group: &str,
    methods: &[(&ContractService, &ContractMethod)],
) -> String {
    let controller_type = http_controller_type_name(group);
    let controller_base = http_controller_base_name(group);
    let register_fn = format!(
        "Register{}{}Http",
        exported_name(&entry.name),
        exported_name(group)
    );
    let controller_prefix = format!("/{}", group.trim_matches('/'));
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", http_group_package_name(group)));
    body.push_str("import (\n");
    body.push_str("    httpadapter \"lania-g/v3/adapter/http\"\n");
    body.push_str(")\n\n");
    body.push_str(&format!(
        "// {controller_type} is a generated HTTP controller for group `{group}`.\n"
    ));
    body.push_str(
        "// Fill in method bodies in generated files or move the implementation to a non-generated file later.\n",
    );
    body.push_str(&format!("type {controller_type} struct {{}}\n\n"));
    body.push_str(&format!(
        "func {register_fn}(api *httpadapter.API, controller *{controller_type}) {{\n"
    ));
    body.push_str("    if api == nil || controller == nil {\n        return\n    }\n");
    body.push_str(&format!(
        "    b := api.Controller(\"{controller_prefix}\", controller)\n"
    ));
    for (service, method) in methods {
        let builder_call = http_builder_call(method);
        let relative_path = http_relative_path(service, method, group);
        let method_name = http_controller_method_name(method, &controller_base);
        body.push_str(&format!(
            "    b.{builder_call}({relative_path:?}, controller.{method_name})\n"
        ));
    }
    body.push_str("    b.Build()\n}\n");
    body
}

fn render_http_group_method_go(
    entry: &CompiledModuleEntry,
    service: &ContractService,
    method: &ContractMethod,
    group: &str,
) -> Result<String> {
    let fields = collect_http_fields(entry, method);
    let body_fields = fields
        .iter()
        .filter(|field| field.binding_source == "body")
        .cloned()
        .collect::<Vec<_>>();
    let uses_json_binding = uses_http_json_binding(&body_fields);
    let args_fields = if uses_json_binding {
        fields
            .iter()
            .filter(|field| field.binding_source != "body")
            .cloned()
            .collect::<Vec<_>>()
    } else {
        fields.clone()
    };
    let needs_httpbinding = uses_json_binding || !args_fields.is_empty() || method.oneway;
    let needs_net_http = method.oneway;

    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", http_group_package_name(group)));
    body.push_str("import (\n");
    body.push_str("    \"errors\"\n");
    if needs_net_http {
        body.push_str("    \"net/http\"\n");
    }
    if needs_httpbinding {
        body.push_str("    httpbinding \"github.com/sao-lang/lania-g/protocol/http/v3/binding\"\n");
    }
    body.push_str(")\n\n");
    if let Some(body_request) = render_http_request_type(entry, service, method)? {
        body.push_str(&body_request);
    }
    if let Some(args_type) = render_http_args_type(entry, service, method)? {
        body.push_str(&args_type);
    }
    body.push_str(&render_http_controller_method(
        entry,
        service,
        method,
        &http_controller_type_name(group),
        &http_controller_base_name(group),
    )?);
    Ok(body)
}

#[allow(dead_code)]
fn render_http_error_helpers_go(_entry: &CompiledModuleEntry) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str("package http\n\n");
    body.push_str("import (\n");
    body.push_str("    \"net/http\"\n");
    body.push_str(")\n\n");
    body.push_str("func HTTPErrorCodeFromError(err error) int32 {\n");
    body.push_str("    if err == nil {\n        return 0\n    }\n");
    body.push_str("    return int32(HTTPStatusFromError(err))\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPErrorEnvelopeFromError(err error) HTTPEnvelope[any] {\n");
    body.push_str("    if err == nil {\n        return HTTPSuccessEnvelope[any](nil)\n    }\n");
    body.push_str("    return HTTPEnvelope[any]{\n");
    body.push_str("        Code:    HTTPErrorCodeFromError(err),\n");
    body.push_str("        Message: httpErrorMessage(err),\n");
    body.push_str("        Success: false,\n");
    body.push_str("    }\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPWriteError(err error) (int, HTTPEnvelope[any]) {\n");
    body.push_str(
        "    if err == nil {\n        return http.StatusOK, HTTPSuccessEnvelope[any](nil)\n    }\n",
    );
    body.push_str("    return HTTPStatusFromError(err), HTTPErrorEnvelopeFromError(err)\n");
    body.push_str("}\n\n");
    body.push_str("func httpErrorMessage(err error) string {\n");
    body.push_str("    if err == nil {\n        return \"\"\n    }\n");
    body.push_str("    return err.Error()\n");
    body.push_str("}\n");
    body
}

#[allow(dead_code)]
fn render_http_envelope_go() -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str("package http\n\n");
    body.push_str("type HTTPEnvelope[T any] struct {\n");
    body.push_str("    Code    int32  `json:\"code\"`\n");
    body.push_str("    Message string `json:\"msg\"`\n");
    body.push_str("    Data    T      `json:\"data,omitempty\"`\n");
    body.push_str("    Success bool   `json:\"success\"`\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPSuccessEnvelope[T any](data T) HTTPEnvelope[T] {\n");
    body.push_str("    return HTTPEnvelope[T]{\n");
    body.push_str("        Code:    0,\n");
    body.push_str("        Message: \"ok\",\n");
    body.push_str("        Data:    data,\n");
    body.push_str("        Success: true,\n");
    body.push_str("    }\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPFailureEnvelope(message string) HTTPEnvelope[any] {\n");
    body.push_str("    return HTTPEnvelope[any]{\n");
    body.push_str("        Code:    1,\n");
    body.push_str("        Message: message,\n");
    body.push_str("        Success: false,\n");
    body.push_str("    }\n");
    body.push_str("}\n\n");
    body.push_str("func HTTPEnvelopeFromResult[T any](data T, err error) HTTPEnvelope[T] {\n");
    body.push_str("    if err != nil {\n");
    body.push_str("        return HTTPEnvelope[T]{\n");
    body.push_str("            Code:    int32(HTTPStatusFromError(err)),\n");
    body.push_str("            Message: httpErrorMessage(err),\n");
    body.push_str("            Success: false,\n");
    body.push_str("        }\n");
    body.push_str("    }\n");
    body.push_str("    return HTTPSuccessEnvelope(data)\n");
    body.push_str("}\n");
    body
}

fn render_http_demo_main_go(entry: &CompiledModuleEntry, output: &ModuleOutputPaths) -> String {
    let mut groups = BTreeMap::<String, String>::new();
    for service in &entry.ir.services {
        let mut seen = BTreeMap::<String, ()>::new();
        for method in collect_service_methods(entry, service) {
            let group = http_group_name(service, method);
            if seen.insert(group.clone(), ()).is_some() {
                continue;
            }
            groups.insert(
                group.clone(),
                format!(
                    "Register{}{}Http",
                    exported_name(&entry.name),
                    exported_name(&group)
                ),
            );
        }
    }

    let bootstrap_type = format!(
        "{}Bootstrap",
        append_exported_suffix(&exported_name(&entry.name), "Http")
    );
    let constructor_name = format!("New{bootstrap_type}");
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n");
    body.push_str(
        "// Replace `REPLACE_WITH_YOUR_MODULE` with your real Go module path before compiling.\n\n",
    );
    body.push_str("package bootstrap\n\n");
    body.push_str("import (\n");
    body.push_str("    httpadapter \"github.com/sao-lang/lania-g/protocol/http/v3\"\n");
    let http_import_root = output
        .http_root_import
        .as_deref()
        .unwrap_or("generated/lania/adapters/http");
    for group in groups.keys() {
        body.push_str(&format!(
            "    {} \"REPLACE_WITH_YOUR_MODULE/{}/{}\"\n",
            http_group_import_alias(group),
            http_import_root.trim_matches('/'),
            group.trim_matches('/')
        ));
    }
    body.push_str(")\n\n");
    body.push_str(&format!(
        "type {bootstrap_type} struct {{\n"
    ));
    for group in groups.keys() {
        let controller_type = http_controller_type_name(group);
        body.push_str(&format!(
            "    {controller_type} *{}.{}\n",
            http_group_import_alias(group),
            controller_type
        ));
    }
    body.push_str("}\n\n");
    body.push_str(&format!("func {constructor_name}() *{bootstrap_type} {{\n"));
    body.push_str(&format!("    bootstrap := &{bootstrap_type}{{}}\n"));
    body.push_str("    bootstrap.ensureDefaults()\n");
    body.push_str("    return bootstrap\n");
    body.push_str("}\n\n");
    body.push_str(&format!("func (b *{bootstrap_type}) ensureDefaults() {{\n"));
    body.push_str("    if b == nil {\n        return\n    }\n");
    for group in groups.keys() {
        let controller_type = http_controller_type_name(group);
        body.push_str(&format!(
            "    if b.{controller_type} == nil {{\n        b.{controller_type} = &{}.{}{{}}\n    }}\n",
            http_group_import_alias(group),
            controller_type
        ));
    }
    body.push_str("}\n\n");
    body.push_str(&format!(
        "func (b *{bootstrap_type}) Providers() []any {{\n"
    ));
    body.push_str("    if b == nil {\n        return nil\n    }\n");
    body.push_str("    b.ensureDefaults()\n");
    body.push_str("    return []any{\n");
    for group in groups.keys() {
        let controller_type = http_controller_type_name(group);
        body.push_str(&format!("        b.{controller_type},\n"));
    }
    body.push_str("    }\n");
    body.push_str("}\n\n");
    body.push_str(&format!(
        "func (b *{bootstrap_type}) Register(api *httpadapter.API) {{\n"
    ));
    body.push_str("    if api == nil || b == nil {\n        return\n    }\n");
    body.push_str("    b.ensureDefaults()\n");
    for group in groups.keys() {
        let register_fn = groups.get(group).expect("group register fn");
        let controller_type = http_controller_type_name(group);
        body.push_str(&format!(
            "    {}.{register_fn}(api, b.{controller_type})\n",
            http_group_import_alias(group)
        ));
    }
    body.push_str("}\n");
    body
}

fn render_http_model_type(ty: &ContractType) -> String {
    let mut body = String::new();
    body.push_str(&format!("type {} struct {{\n", exported_name(&ty.name)));
    for field in &ty.fields {
        let json_name = field
            .http_binding
            .as_ref()
            .map(|binding| binding.name.as_str())
            .unwrap_or(field.name.as_str());
        let tag_value = if field.optional || ty.kind == ContractTypeKind::Union {
            format!("{json_name},omitempty")
        } else {
            json_name.to_string()
        };
        let tags = render_go_tags(&[
            ("json", Some(tag_value.as_str())),
            (
                "default",
                render_default_tag_value(field.default_value.as_deref()).as_deref(),
            ),
        ]);
        body.push_str(&format!(
            "    {} {} {}\n",
            go_field_name(&field.name),
            render_http_model_field_type(ty, field),
            tags
        ));
    }
    body.push_str("}\n\n");
    body
}

fn render_http_model_field_type(ty: &ContractType, field: &ContractField) -> String {
    let field_type = sanitize_go_type(&field.ty);
    if ty.kind == ContractTypeKind::Union
        && !field_type.starts_with("[]")
        && !field_type.starts_with("map[")
    {
        format!("*{field_type}")
    } else {
        field_type
    }
}

fn render_http_union_validator(ty: &ContractType) -> String {
    let type_name = exported_name(&ty.name);
    let mut body = String::new();
    body.push_str(&format!("func (v {type_name}) ValidateUnion() error {{\n"));
    body.push_str("    count := 0\n");
    for field in &ty.fields {
        let field_name = go_field_name(&field.name);
        let field_type = sanitize_go_type(&field.ty);
        if field_type.starts_with("[]") || field_type.starts_with("map[") {
            body.push_str(&format!(
                "    if len(v.{field_name}) > 0 {{\n        count++\n    }}\n"
            ));
        } else {
            body.push_str(&format!(
                "    if v.{field_name} != nil {{\n        count++\n    }}\n"
            ));
        }
    }
    body.push_str("    if count > 1 {\n");
    body.push_str(&format!(
        "        return errors.New(\"{type_name} allows only one active field\")\n"
    ));
    body.push_str("    }\n    return nil\n}\n\n");
    body
}

fn render_http_exception_error_method(ty: &ContractType) -> String {
    let type_name = exported_name(&ty.name);
    let message_field = ty
        .fields
        .iter()
        .find(|field| normalize_http_binding_key(&field.name) == "message")
        .map(|field| go_field_name(&field.name));
    let code_field = ty
        .fields
        .iter()
        .find(|field| normalize_http_binding_key(&field.name) == "code")
        .map(|field| go_field_name(&field.name));
    let mut body = String::new();
    body.push_str(&format!("func (e *{type_name}) Error() string {{\n"));
    body.push_str(&format!(
        "    if e == nil {{\n        return {:?}\n    }}\n",
        slugify(&ty.name).replace('_', " ")
    ));
    match (code_field, message_field) {
        (Some(code), Some(message)) => body.push_str(&format!(
            "    return fmt.Sprintf(\"{}: code=%v message=%s\", e.{code}, e.{message})\n",
            slugify(&ty.name).replace('_', " ")
        )),
        (_, Some(message)) => body.push_str(&format!("    return e.{message}\n")),
        _ => body.push_str(&format!(
            "    return {:?}\n",
            slugify(&ty.name).replace('_', " ")
        )),
    }
    body.push_str("}\n\n");
    body
}

fn render_http_error_status_helper(entry: &CompiledModuleEntry) -> String {
    let mut exception_names = entry
        .ir
        .services
        .iter()
        .flat_map(|service| collect_service_methods(entry, service))
        .flat_map(|method| method.throws.iter())
        .map(|field| sanitize_go_type(&field.ty))
        .collect::<Vec<_>>();
    exception_names.sort();
    exception_names.dedup();
    let mut body = String::new();
    body.push_str("func HTTPStatusFromError(err error) int {\n");
    body.push_str("    switch err.(type) {\n");
    for exception_name in exception_names {
        body.push_str(&format!(
            "    case *{exception_name}:\n        return {}\n",
            guess_http_status_for_exception(&exception_name)
        ));
    }
    body.push_str("    default:\n        return http.StatusInternalServerError\n    }\n}\n\n");
    body
}

fn guess_http_status_for_exception(name: &str) -> &'static str {
    // 这里故意使用基于名称的启发式映射，而不是强依赖显式配置。
    // 代码生成阶段通常只能看到 contract 名称，看不到业务实现细节，因此需要给 demo / 骨架代码
    // 一个“足够合理”的默认状态码。匹配不到时统一退回 500，避免误报成功或客户端错误。
    let normalized = normalize_http_binding_key(name);
    if normalized.contains("notfound") {
        "http.StatusNotFound"
    } else if normalized.contains("validation") || normalized.contains("badrequest") {
        "http.StatusBadRequest"
    } else if normalized.contains("conflict") || normalized.contains("biz") {
        "http.StatusConflict"
    } else if normalized.contains("unauthorized") {
        "http.StatusUnauthorized"
    } else if normalized.contains("forbidden") {
        "http.StatusForbidden"
    } else {
        "http.StatusInternalServerError"
    }
}

fn render_grpc_group_types_go(entry: &CompiledModuleEntry, service: &ContractService) -> String {
    let _ = service;
    let has_oneof_types = entry.ir.types.iter().any(has_grpc_oneof_fields);
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", grpc_service_package_name(service)));
    if has_oneof_types {
        body.push_str("import (\n");
        body.push_str("    \"errors\"\n");
        body.push_str(")\n\n");
    }
    body.push_str(&render_grpc_shared_types(entry));
    body
}

fn render_grpc_group_metadata_go(
    _entry: &CompiledModuleEntry,
    service: &ContractService,
    methods: &[&ContractMethod],
) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", grpc_service_package_name(service)));
    body.push_str("type ServiceMetadata struct {\n");
    body.push_str("    Name string\n");
    body.push_str("    Deprecated bool\n");
    body.push_str("    Options map[string]string\n");
    body.push_str("}\n\n");
    body.push_str("type MethodMetadata struct {\n");
    body.push_str("    Service string\n");
    body.push_str("    Method string\n");
    body.push_str("    FullMethod string\n");
    body.push_str("    StreamingMode string\n");
    body.push_str("    RequestType string\n");
    body.push_str("    ResponseType string\n");
    body.push_str("    Deprecated bool\n");
    body.push_str("    IdempotencyLevel string\n");
    body.push_str("    Options map[string]string\n");
    body.push_str("}\n\n");

    let service_name = grpc_service_name(service);
    body.push_str("var Services = map[string]ServiceMetadata{\n");
    body.push_str(&format!("    \"{service_name}\": {{\n"));
    body.push_str(&format!("        Name: \"{service_name}\",\n"));
    body.push_str(&format!(
        "        Deprecated: {},\n",
        service.grpc_metadata.deprecated
    ));
    body.push_str(&format!(
        "        Options: {},\n",
        render_go_string_map_literal(&service.grpc_metadata.options)
    ));
    body.push_str("    },\n");
    body.push_str("}\n\n");

    body.push_str("var Methods = map[string]MethodMetadata{\n");
    for method in methods {
        let method_name = grpc_method_name(method);
        let full_method = grpc_full_method_name(service, method);
        body.push_str(&format!("    \"{full_method}\": {{\n"));
        body.push_str(&format!("        Service: \"{service_name}\",\n"));
        body.push_str(&format!("        Method: \"{method_name}\",\n"));
        body.push_str(&format!("        FullMethod: \"{full_method}\",\n"));
        body.push_str(&format!(
            "        StreamingMode: \"{}\",\n",
            grpc_streaming_mode_name(method)
        ));
        body.push_str(&format!(
            "        RequestType: \"{}\",\n",
            sanitize_go_type(&method.request)
        ));
        body.push_str(&format!(
            "        ResponseType: \"{}\",\n",
            sanitize_go_type(&method.response)
        ));
        body.push_str(&format!(
            "        Deprecated: {},\n",
            method.grpc_metadata.deprecated
        ));
        body.push_str(&format!(
            "        IdempotencyLevel: {:?},\n",
            method
                .grpc_metadata
                .idempotency_level
                .as_deref()
                .unwrap_or("")
        ));
        body.push_str(&format!(
            "        Options: {},\n",
            render_go_string_map_literal(&method.grpc_metadata.options)
        ));
        body.push_str("    },\n");
    }
    body.push_str("}\n\n");
    body.push_str("func MethodMetadataByFullMethod(fullMethod string) (MethodMetadata, bool) {\n");
    body.push_str("    meta, ok := Methods[fullMethod]\n");
    body.push_str("    return meta, ok\n");
    body.push_str("}\n\n");
    body.push_str("func ServiceMetadataByName(name string) (ServiceMetadata, bool) {\n");
    body.push_str("    meta, ok := Services[name]\n");
    body.push_str("    return meta, ok\n");
    body.push_str("}\n");
    body
}

fn render_grpc_group_error_helpers_go(service: &ContractService) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", grpc_service_package_name(service)));
    body.push_str("import (\n");
    body.push_str("    \"errors\"\n");
    body.push_str("    \"reflect\"\n");
    body.push_str("    \"strings\"\n");
    body.push_str("    \"google.golang.org/grpc/codes\"\n");
    body.push_str("    \"google.golang.org/grpc/status\"\n");
    body.push_str(")\n\n");
    body.push_str("func GRPCStatusCodeFromError(err error) codes.Code {\n");
    body.push_str("    if err == nil {\n        return codes.OK\n    }\n");
    body.push_str(
        "    if code := status.Code(err); code != codes.Unknown {\n        return code\n    }\n",
    );
    body.push_str("    var coded interface{ GRPCStatusCode() codes.Code }\n");
    body.push_str(
        "    if errors.As(err, &coded) {\n        return coded.GRPCStatusCode()\n    }\n",
    );
    body.push_str("    name := strings.ToLower(grpcGeneratedErrorTypeName(err))\n");
    body.push_str("    switch {\n");
    body.push_str("    case strings.Contains(name, \"invalid\"), strings.Contains(name, \"validation\"):\n        return codes.InvalidArgument\n");
    body.push_str(
        "    case strings.Contains(name, \"notfound\"):\n        return codes.NotFound\n",
    );
    body.push_str("    case strings.Contains(name, \"alreadyexists\"), strings.Contains(name, \"conflict\"):\n        return codes.AlreadyExists\n");
    body.push_str(
        "    case strings.Contains(name, \"permission\"):\n        return codes.PermissionDenied\n",
    );
    body.push_str(
        "    case strings.Contains(name, \"unauth\"):\n        return codes.Unauthenticated\n",
    );
    body.push_str("    case strings.Contains(name, \"timeout\"), strings.Contains(name, \"deadline\"):\n        return codes.DeadlineExceeded\n");
    body.push_str(
        "    case strings.Contains(name, \"unavailable\"):\n        return codes.Unavailable\n",
    );
    body.push_str("    default:\n        return codes.Internal\n    }\n}\n\n");
    body.push_str("func GRPCStatusError(err error) error {\n");
    body.push_str("    if err == nil {\n        return nil\n    }\n");
    body.push_str("    return status.Error(GRPCStatusCodeFromError(err), err.Error())\n");
    body.push_str("}\n\n");
    body.push_str("func GRPCErrorMessage(err error) string {\n");
    body.push_str("    if err == nil {\n        return \"\"\n    }\n");
    body.push_str("    return err.Error()\n");
    body.push_str("}\n\n");
    body.push_str("func grpcGeneratedErrorTypeName(err error) string {\n");
    body.push_str("    if err == nil {\n        return \"\"\n    }\n");
    body.push_str("    typ := reflect.TypeOf(err)\n");
    body.push_str(
        "    for typ != nil && typ.Kind() == reflect.Ptr {\n        typ = typ.Elem()\n    }\n",
    );
    body.push_str("    if typ == nil {\n        return \"\"\n    }\n");
    body.push_str("    return typ.Name()\n");
    body.push_str("}\n");
    body
}

fn render_grpc_group_register_go(
    entry: &CompiledModuleEntry,
    service: &ContractService,
    methods: &[&ContractMethod],
) -> String {
    let receiver_type = grpc_receiver_type_name(entry, service);
    let register_fn = grpc_register_fn_name(entry, service);
    let service_name = grpc_service_name(service);
    let needs_structpb = methods.iter().any(|method| {
        matches!(
            method.streaming,
            ContractStreamingMode::Unary | ContractStreamingMode::Server
        )
    });
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", grpc_service_package_name(service)));
    body.push_str("import (\n");
    body.push_str("    grpcadapter \"lania-g/v3/adapter/grpc\"\n");
    if needs_structpb {
        body.push_str("    \"google.golang.org/protobuf/types/known/structpb\"\n");
    }
    body.push_str(")\n\n");
    body.push_str(&format!(
        "// {receiver_type} is a generated receiver stub for `{}` over `grpc`.\n",
        service.name
    ));
    body.push_str(
        "// Implement these methods in a separate (non-generated) file in the same package.\n",
    );
    body.push_str(&format!("type {receiver_type} struct {{}}\n\n"));
    body.push_str(&format!(
        "func {register_fn}(api *grpcadapter.API, receiver *{receiver_type}) {{\n"
    ));
    body.push_str("    if api == nil || receiver == nil {\n        return\n    }\n");
    body.push_str(&format!("    b := api.Service(\"{service_name}\", receiver)\n"));
    for method in methods {
        let method_name = grpc_method_name(method);
        let builder_call = grpc_builder_call(method);
        body.push_str(&format!(
            "    b.{builder_call}(\"{method_name}\", receiver.{})",
            exported_name(&method.name)
        ));
        if matches!(
            method.streaming,
            ContractStreamingMode::Unary | ContractStreamingMode::Server
        ) {
            body.push_str(".WithReqType((*structpb.Struct)(nil))");
        }
        body.push('\n');
    }
    body.push_str("    b.Build()\n");
    body.push_str("}\n");
    body
}

fn render_grpc_group_method_go(
    entry: &CompiledModuleEntry,
    service: &ContractService,
    method: &ContractMethod,
) -> String {
    let receiver_type = grpc_receiver_type_name(entry, service);
    let needs_structpb = !matches!(method.streaming, ContractStreamingMode::Unary);
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str(&format!("package {}\n\n", grpc_service_package_name(service)));
    body.push_str("import (\n");
    body.push_str("    \"errors\"\n");
    body.push_str("    grpcbinding \"github.com/sao-lang/lania-g/protocol/grpc/v3/binding\"\n");
    if needs_structpb {
        body.push_str("    \"google.golang.org/protobuf/types/known/structpb\"\n");
    }
    body.push_str(")\n\n");
    body.push_str(&render_grpc_receiver_method(entry, method, &receiver_type));
    body
}

fn render_grpc_shared_types(entry: &CompiledModuleEntry) -> String {
    let mut body = String::new();
    if entry
        .ir
        .services
        .iter()
        .flat_map(|service| service.methods.iter())
        .any(|method| method.request == "Empty" || method.response == "Empty")
    {
        body.push_str("type Empty struct {}\n\n");
    }
    for enum_type in &entry.ir.enums {
        let enum_name = exported_name(&enum_type.name);
        body.push_str(&format!("type {enum_name} int32\n\n"));
        body.push_str("const (\n");
        let mut next_value = 0i32;
        for variant in &enum_type.variants {
            let value = variant
                .value
                .as_deref()
                .and_then(|raw| raw.parse::<i32>().ok())
                .unwrap_or(next_value);
            body.push_str(&format!(
                "    {}{} {} = {}\n",
                exported_name(&enum_type.name),
                go_field_name(&variant.name),
                enum_name,
                value
            ));
            next_value = value + 1;
        }
        body.push_str(")\n\n");
    }
    for ty in &entry.ir.types {
        body.push_str(&render_grpc_model_type(ty));
        if has_grpc_oneof_fields(ty) {
            body.push_str(&render_grpc_oneof_validator(ty));
        }
    }
    body
}

fn render_grpc_single_module_go(entry: &CompiledModuleEntry) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str("package grpc\n\n");
    body.push_str("import (\n");
    body.push_str("    \"errors\"\n");
    body.push_str("    grpcadapter \"lania-g/v3/adapter/grpc\"\n");
    body.push_str("    grpcbinding \"github.com/sao-lang/lania-g/protocol/grpc/v3/binding\"\n");
    body.push_str("    \"google.golang.org/protobuf/types/known/structpb\"\n");
    body.push_str(")\n\n");
    body.push_str(&render_grpc_shared_types(entry));

    for service in &entry.ir.services {
        let receiver_type = format!(
            "{}{}{}Receiver",
            exported_name(&entry.name),
            exported_name(&service.name),
            "Grpc"
        );

        body.push_str(&format!(
            "// {receiver_type} is a generated receiver stub for `{}` over `grpc`.\n",
            service.name
        ));
        body.push_str(
            "// Implement these methods in a separate (non-generated) file in the same package.\n",
        );
        body.push_str(&format!("type {receiver_type} struct {{}}\n\n"));

        let service_name = service
            .methods
            .iter()
            .find_map(|m| m.grpc_service.clone())
            .unwrap_or_else(|| exported_name(&service.name));
        let register_fn = format!(
            "Register{}{}Grpc",
            exported_name(&entry.name),
            exported_name(&service.name)
        );
        body.push_str(&format!(
            "func {register_fn}(api *grpcadapter.API, receiver *{receiver_type}) {{\n"
        ));
        body.push_str("    if api == nil || receiver == nil {\n        return\n    }\n");
        body.push_str(&format!(
            "    b := api.Service(\"{service_name}\", receiver)\n"
        ));
        for method in &service.methods {
            let method_name = method
                .grpc_method
                .clone()
                .unwrap_or_else(|| exported_name(&method.name));
            let builder_call = grpc_builder_call(method);
            body.push_str(&format!(
                "    b.{builder_call}(\"{method_name}\", receiver.{})",
                exported_name(&method.name)
            ));
            if matches!(
                method.streaming,
                ContractStreamingMode::Unary | ContractStreamingMode::Server
            ) {
                body.push_str(".WithReqType((*structpb.Struct)(nil))");
            }
            body.push('\n');
        }
        body.push_str("    b.Build()\n}\n\n");

        for method in &service.methods {
            body.push_str(&render_grpc_receiver_method(entry, method, &receiver_type));
        }
    }

    body
}

fn render_grpc_declarations_go(entry: &CompiledModuleEntry) -> String {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str("package grpc\n\n");
    let declaration_name = format!("{}GrpcDeclarations", exported_name(&entry.name));
    body.push_str(&format!("var {declaration_name} = []string{{\n"));
    for service in &entry.ir.services {
        let service_name = service
            .methods
            .iter()
            .find_map(|m| m.grpc_service.clone())
            .unwrap_or_else(|| exported_name(&service.name));
        for method in &service.methods {
            let method_name = method
                .grpc_method
                .clone()
                .unwrap_or_else(|| exported_name(&method.name));
            let declaration = format!(
                "Service(\"{service_name}\").{}(\"{method_name}\")",
                grpc_builder_call(method)
            );
            body.push_str(&format!("    `{declaration}`,\n"));
        }
    }
    body.push_str("}\n");
    body
}

fn render_grpc_demo_main_go(entry: &CompiledModuleEntry, output: &ModuleOutputPaths) -> String {
    let bootstrap_type = format!(
        "{}Bootstrap",
        append_exported_suffix(&exported_name(&entry.name), "Grpc")
    );
    let constructor_name = format!("New{bootstrap_type}");
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n");
    body.push_str(
        "// Replace `REPLACE_WITH_YOUR_MODULE` with your real Go module path before compiling.\n\n",
    );
    body.push_str("package bootstrap\n\n");
    body.push_str("import (\n");
    body.push_str("    grpcadapter \"github.com/sao-lang/lania-g/protocol/grpc/v3\"\n");
    let grpc_import_root = output
        .grpc_root_import
        .as_deref()
        .unwrap_or("generated/lania/adapters/grpc");
    for service in &entry.ir.services {
        body.push_str(&format!(
            "    {} \"REPLACE_WITH_YOUR_MODULE/{}/{}\"\n",
            grpc_service_import_alias(service),
            grpc_import_root.trim_matches('/'),
            grpc_service_dir_name(service)
        ));
    }
    body.push_str(")\n\n");
    body.push_str(&format!("type {bootstrap_type} struct {{\n"));
    let mut receiver_fields = Vec::new();
    for service in &entry.ir.services {
        let receiver_type = grpc_receiver_type_name(entry, service);
        receiver_fields.push((receiver_type, service));
    }
    for (receiver_type, service) in &receiver_fields {
        body.push_str(&format!(
            "    {receiver_type} *{}.{}\n",
            grpc_service_import_alias(service),
            receiver_type
        ));
    }
    body.push_str("}\n\n");
    body.push_str(&format!("func {constructor_name}() *{bootstrap_type} {{\n"));
    body.push_str(&format!("    bootstrap := &{bootstrap_type}{{}}\n"));
    body.push_str("    bootstrap.ensureDefaults()\n");
    body.push_str("    return bootstrap\n");
    body.push_str("}\n\n");
    body.push_str(&format!("func (b *{bootstrap_type}) ensureDefaults() {{\n"));
    body.push_str("    if b == nil {\n        return\n    }\n");
    for (receiver_type, service) in &receiver_fields {
        body.push_str(&format!(
            "    if b.{receiver_type} == nil {{\n        b.{receiver_type} = &{}.{}{{}}\n    }}\n",
            grpc_service_import_alias(service),
            receiver_type
        ));
    }
    body.push_str("}\n\n");
    body.push_str(&format!(
        "func (b *{bootstrap_type}) Providers() []any {{\n"
    ));
    body.push_str("    if b == nil {\n        return nil\n    }\n");
    body.push_str("    b.ensureDefaults()\n");
    body.push_str("    return []any{\n");
    for (receiver_type, _) in &receiver_fields {
        body.push_str(&format!("        b.{receiver_type},\n"));
    }
    body.push_str("    }\n");
    body.push_str("}\n\n");
    body.push_str(&format!(
        "func (b *{bootstrap_type}) Register(api *grpcadapter.API) {{\n"
    ));
    body.push_str("    if api == nil || b == nil {\n        return\n    }\n");
    body.push_str("    b.ensureDefaults()\n");
    for (receiver_type, service) in &receiver_fields {
        body.push_str(&format!(
            "    {}.{}(api, b.{receiver_type})\n",
            grpc_service_import_alias(service),
            grpc_register_fn_name(entry, service)
        ));
    }
    body.push_str("}\n");
    body
}

fn render_go_string_map_literal(values: &BTreeMap<String, String>) -> String {
    if values.is_empty() {
        return "map[string]string{}".to_string();
    }
    let mut parts = Vec::with_capacity(values.len());
    for (key, value) in values {
        parts.push(format!("{key:?}: {value:?}"));
    }
    format!("map[string]string{{{}}}", parts.join(", "))
}

fn render_grpc_model_type(ty: &ContractType) -> String {
    let mut body = String::new();
    body.push_str(&format!("type {} struct {{\n", exported_name(&ty.name)));
    for field in &ty.fields {
        let json_name = field.name.as_str();
        let tag_value = if field.optional || field.oneof_group.is_some() {
            format!("{json_name},omitempty")
        } else {
            json_name.to_string()
        };
        let validate_value = join_csv(&field.validation_rules);
        let tags = render_go_tags(&[
            ("json", Some(tag_value.as_str())),
            ("validate", validate_value.as_deref()),
            (
                "default",
                render_default_tag_value(field.default_value.as_deref()).as_deref(),
            ),
        ]);
        body.push_str(&format!(
            "    {} {} {}\n",
            go_field_name(&field.name),
            grpc_model_field_type(field),
            tags
        ));
    }
    body.push_str("}\n\n");
    body
}

fn grpc_model_field_type(field: &ContractField) -> String {
    let field_type = sanitize_go_type(&field.ty);
    if field.oneof_group.is_some()
        && !field_type.starts_with("[]")
        && !field_type.starts_with("map[")
        && field_type != "[]byte"
    {
        format!("*{field_type}")
    } else {
        field_type
    }
}

fn has_grpc_oneof_fields(ty: &ContractType) -> bool {
    ty.fields.iter().any(|field| field.oneof_group.is_some())
}

fn render_grpc_oneof_validator(ty: &ContractType) -> String {
    let type_name = exported_name(&ty.name);
    let mut groups = BTreeMap::<String, Vec<&ContractField>>::new();
    for field in &ty.fields {
        if let Some(group) = &field.oneof_group {
            groups.entry(group.clone()).or_default().push(field);
        }
    }

    let mut body = String::new();
    body.push_str(&format!("func (v {type_name}) ValidateOneof() error {{\n"));
    for (group, fields) in groups {
        let count_name = format!("count{}", exported_name(&group));
        body.push_str(&format!("    {count_name} := 0\n"));
        for field in fields {
            let field_name = go_field_name(&field.name);
            let field_type = sanitize_go_type(&field.ty);
            if field_type.starts_with("[]") || field_type.starts_with("map[") {
                body.push_str(&format!(
                    "    if len(v.{field_name}) > 0 {{\n        {count_name}++\n    }}\n"
                ));
            } else {
                body.push_str(&format!(
                    "    if v.{field_name} != nil {{\n        {count_name}++\n    }}\n"
                ));
            }
        }
        body.push_str(&format!(
            "    if {count_name} > 1 {{\n        return errors.New(\"{type_name}.{group} allows only one active field\")\n    }}\n"
        ));
    }
    body.push_str("    return nil\n}\n\n");
    body
}

fn grpc_builder_call(method: &ContractMethod) -> &'static str {
    match method.streaming {
        ContractStreamingMode::Server => "ServerStreamMethod",
        ContractStreamingMode::Client => "ClientStreamMethod",
        ContractStreamingMode::Bidi => "BidiStreamMethod",
        ContractStreamingMode::Unary => "Method",
    }
}

fn grpc_service_name(service: &ContractService) -> String {
    service
        .methods
        .iter()
        .find_map(|method| method.grpc_service.clone())
        .unwrap_or_else(|| exported_name(&service.name))
}

fn grpc_service_dir_name(service: &ContractService) -> String {
    segmented_slug(&grpc_service_name(service), "_")
}

fn grpc_service_package_name(service: &ContractService) -> String {
    let package = grpc_service_dir_name(service);
    if package.is_empty() {
        "grpcgen".into()
    } else {
        package
    }
}

fn grpc_service_import_alias(service: &ContractService) -> String {
    format!("generated{}", go_field_name(&grpc_service_dir_name(service)))
}

fn grpc_receiver_type_name(entry: &CompiledModuleEntry, service: &ContractService) -> String {
    format!(
        "{}{}{}Receiver",
        exported_name(&entry.name),
        exported_name(&service.name),
        "Grpc"
    )
}

fn grpc_register_fn_name(entry: &CompiledModuleEntry, service: &ContractService) -> String {
    format!(
        "Register{}{}Grpc",
        exported_name(&entry.name),
        exported_name(&service.name)
    )
}

fn grpc_method_name(method: &ContractMethod) -> String {
    method
        .grpc_method
        .clone()
        .unwrap_or_else(|| exported_name(&method.name))
}

fn grpc_method_file_name(method: &ContractMethod) -> String {
    segmented_slug(&method.name, "_")
}

fn grpc_full_method_name(service: &ContractService, method: &ContractMethod) -> String {
    format!(
        "/{}/{}",
        grpc_service_name(service),
        grpc_method_name(method)
    )
}

fn grpc_streaming_mode_name(method: &ContractMethod) -> &'static str {
    match method.streaming {
        ContractStreamingMode::Unary => "unary",
        ContractStreamingMode::Server => "server_stream",
        ContractStreamingMode::Client => "client_stream",
        ContractStreamingMode::Bidi => "bidi_stream",
    }
}

fn render_grpc_receiver_method(
    entry: &CompiledModuleEntry,
    method: &ContractMethod,
    receiver_type: &str,
) -> String {
    // 为单个 gRPC 方法生成接收器骨架代码。
    //
    // 这里生成的不是完整业务实现，而是“可编译、可接线”的占位骨架：
    // - 根据 streaming 模式决定函数签名；
    // - 在需要时自动绑定请求结构；
    // - 当请求包含 oneof 字段时，补上默认校验调用；
    // - 最后返回 TODO，让业务方明确填充真实逻辑。
    let method_name = exported_name(&method.name);
    let request_type = sanitize_go_type(&method.request);
    let needs_request_bind = method.request != "Empty";
    let request_has_oneof = entry
        .ir
        .types
        .iter()
        .find(|ty| exported_name(&ty.name) == request_type)
        .is_some_and(has_grpc_oneof_fields);
    let args_type = format!("{}{}Args", receiver_type, method_name);

    let mut body = String::new();
    match method.streaming {
        ContractStreamingMode::Unary => {
            body.push_str(&format!(
                "func (r *{receiver_type}) {method_name}(ctx grpcbinding.GRPCContext) (any, error) {{\n"
            ));
            if needs_request_bind {
                body.push_str(&format!("    var req {request_type}\n"));
                body.push_str("    if err := ctx.ShouldBindReq(&req); err != nil {\n");
                body.push_str("        return nil, err\n");
                body.push_str("    }\n");
                if request_has_oneof {
                    body.push_str(
                        "    if err := req.ValidateOneof(); err != nil {\n        return nil, err\n    }\n",
                    );
                }
                body.push_str("    _ = req\n");
            } else {
                body.push_str("    _ = ctx\n");
            }
            body.push_str("    return nil, errors.New(\"TODO\")\n");
            body.push_str("}\n\n");
        }
        ContractStreamingMode::Server => {
            body.push_str(&format!("type {args_type} struct {{\n"));
            body.push_str("    Ctx grpcbinding.GRPCContext\n");
            body.push_str("    Stream grpcbinding.ServerStream[*structpb.Struct]\n");
            body.push_str("}\n\n");
            body.push_str(&format!(
                "func (r *{receiver_type}) {method_name}(args {args_type}) error {{\n"
            ));
            if needs_request_bind {
                body.push_str(&format!("    var req {request_type}\n"));
                body.push_str("    if err := args.Ctx.ShouldBindReq(&req); err != nil {\n");
                body.push_str("        return err\n");
                body.push_str("    }\n");
                if request_has_oneof {
                    body.push_str(
                        "    if err := req.ValidateOneof(); err != nil {\n        return err\n    }\n",
                    );
                }
                body.push_str("    _ = req\n");
            }
            body.push_str("    _ = args.Stream\n");
            body.push_str("    return errors.New(\"TODO\")\n");
            body.push_str("}\n\n");
        }
        ContractStreamingMode::Client => {
            body.push_str(&format!("type {args_type} struct {{\n"));
            body.push_str("    Ctx grpcbinding.GRPCContext\n");
            body.push_str("    Stream grpcbinding.ClientStream[*structpb.Struct]\n");
            body.push_str("}\n\n");
            body.push_str(&format!(
                "func (r *{receiver_type}) {method_name}(args {args_type}) (any, error) {{\n"
            ));
            body.push_str("    _ = args\n");
            body.push_str("    return nil, errors.New(\"TODO\")\n");
            body.push_str("}\n\n");
        }
        ContractStreamingMode::Bidi => {
            body.push_str(&format!("type {args_type} struct {{\n"));
            body.push_str("    Ctx grpcbinding.GRPCContext\n");
            body.push_str(
                "    Stream grpcbinding.BidiStream[*structpb.Struct, *structpb.Struct]\n",
            );
            body.push_str("}\n\n");
            body.push_str(&format!(
                "func (r *{receiver_type}) {method_name}(args {args_type}) error {{\n"
            ));
            body.push_str("    _ = args\n");
            body.push_str("    return errors.New(\"TODO\")\n");
            body.push_str("}\n\n");
        }
    }
    body
}

fn collect_service_methods<'a>(
    entry: &'a CompiledModuleEntry,
    service: &'a ContractService,
) -> Vec<&'a ContractMethod> {
    let mut methods = Vec::new();
    if let Some(parent_name) = &service.extends {
        if let Some(parent) = entry
            .ir
            .services
            .iter()
            .find(|item| &item.name == parent_name)
        {
            methods.extend(collect_service_methods(entry, parent));
        }
    }
    methods.extend(service.methods.iter());
    methods
}

fn collect_http_controller_groups<'a>(
    entry: &'a CompiledModuleEntry,
) -> BTreeMap<String, Vec<(&'a ContractService, &'a ContractMethod)>> {
    // 按“控制器分组”收集 HTTP 方法。
    //
    // 分组名优先来自显式配置（handler_path / category），否则退回到 service 名称。
    // `seen` 用于按“分组 + HTTP 方法 + 路径 + 方法名”去重，避免 service 继承链把同一个路由
    // 多次展开到生成结果里。
    let mut groups = BTreeMap::<String, Vec<(&'a ContractService, &'a ContractMethod)>>::new();
    let mut seen = BTreeSet::<String>::new();
    for service in &entry.ir.services {
        for method in collect_service_methods(entry, service) {
            let group = http_group_name(service, method);
            let route_key = format!(
                "{group}::{}::{}::{}",
                method
                    .http_method
                    .clone()
                    .unwrap_or_else(|| default_http_method(method)),
                method
                    .http_path
                    .clone()
                    .unwrap_or_else(|| default_http_path(service, method)),
                method.name
            );
            if !seen.insert(route_key) {
                continue;
            }
            groups.entry(group).or_default().push((service, method));
        }
    }
    groups
}

fn render_go_literal(ty: &str, value: &str) -> String {
    match sanitize_go_type(ty).as_str() {
        "string" => format!("{value:?}"),
        "bool" | "int8" | "int16" | "int32" | "int64" | "uint32" | "uint64" | "float32"
        | "float64" => value.to_string(),
        _ => format!("{value:?}"),
    }
}

pub(crate) fn default_http_method(method: &ContractMethod) -> String {
    match method.kind.as_str() {
        "query" => "GET".into(),
        _ => "POST".into(),
    }
}

pub(crate) fn default_http_path(service: &ContractService, method: &ContractMethod) -> String {
    format!(
        "/{}/{}",
        slugify(&service.name).replace('_', "-"),
        slugify(&method.name).replace('_', "-")
    )
}

pub(crate) fn default_ws_event(service: &ContractService, method: &ContractMethod) -> String {
    format!(
        "{}.{}",
        segmented_slug(&service.name, "."),
        segmented_slug(&method.name, ".")
    )
}

pub(crate) fn default_graphql_kind(method: &ContractMethod) -> String {
    match method.kind.as_str() {
        "subscription" | "event" => "subscription".into(),
        "command" => "mutation".into(),
        _ => "query".into(),
    }
}

#[derive(Debug, Clone)]
struct HttpFieldRenderSpec {
    go_name: String,
    go_type: String,
    binding_source: String,
    binding_name: String,
    required: bool,
    default_value: Option<String>,
    validation_rules: Vec<String>,
}

fn render_http_request_type(
    entry: &CompiledModuleEntry,
    _service: &ContractService,
    method: &ContractMethod,
) -> Result<Option<String>> {
    let fields = collect_http_fields(entry, method);
    let body_fields = fields
        .iter()
        .filter(|field| field.binding_source == "body")
        .cloned()
        .collect::<Vec<_>>();
    if body_fields.is_empty() || !uses_http_json_binding(&body_fields) {
        return Ok(None);
    }
    let request_type = http_request_type_name(method);
    let mut body = String::new();
    body.push_str(&format!("type {request_type} struct {{\n"));
    for field in &body_fields {
        let tags = render_go_tags(&[
            ("json", Some(field.binding_name.as_str())),
            (
                "default",
                render_default_tag_value(field.default_value.as_deref()).as_deref(),
            ),
            ("validate", join_csv(&field.validation_rules).as_deref()),
        ]);
        body.push_str(&format!(
            "    {} {} {}\n",
            field.go_name, field.go_type, tags
        ));
    }
    body.push_str("}\n\n");
    Ok(Some(body))
}

fn render_http_args_type(
    entry: &CompiledModuleEntry,
    _service: &ContractService,
    method: &ContractMethod,
) -> Result<Option<String>> {
    let fields = collect_http_fields(entry, method);
    let uses_json_binding = uses_http_json_binding(
        &fields
            .iter()
            .filter(|field| field.binding_source == "body")
            .cloned()
            .collect::<Vec<_>>(),
    );
    let args_fields = if uses_json_binding {
        fields
            .into_iter()
            .filter(|field| field.binding_source != "body")
            .collect::<Vec<_>>()
    } else {
        fields
    };
    if args_fields.is_empty() {
        return Ok(None);
    }
    let args_type = http_args_type_name(method);
    let mut body = String::new();
    body.push_str(&format!("type {args_type} struct {{\n"));
    if uses_json_binding || method.oneway {
        body.push_str("    Ctx httpbinding.Context\n");
    }
    for field in &args_fields {
        let tag_name = match field.binding_source.as_str() {
            "body" => "body",
            "query" => "query",
            "param" => "param",
            "header" => "header",
            "form" => "form",
            _ => "json",
        };
        let tags = render_go_tags(&[
            (tag_name, Some(field.binding_name.as_str())),
            ("required", if field.required { Some("true") } else { None }),
            (
                "default",
                render_default_tag_value(field.default_value.as_deref()).as_deref(),
            ),
        ]);
        let field_type = http_args_field_type(field);
        body.push_str(&format!("    {} {} {}\n", field.go_name, field_type, tags));
    }
    body.push_str("}\n\n");
    Ok(Some(body))
}

fn http_args_field_type(field: &HttpFieldRenderSpec) -> String {
    match field.binding_source.as_str() {
        "body" => format!("httpbinding.Body[{}]", field.go_type),
        "query" => format!("httpbinding.Query[{}]", field.go_type),
        "param" => format!("httpbinding.Param[{}]", field.go_type),
        "header" => format!("httpbinding.Header[{}]", field.go_type),
        _ => field.go_type.clone(),
    }
}

fn render_http_controller_method(
    entry: &CompiledModuleEntry,
    _service: &ContractService,
    method: &ContractMethod,
    controller_type: &str,
    controller_base: &str,
) -> Result<String> {
    let fields = collect_http_fields(entry, method);
    let body_fields = fields
        .iter()
        .filter(|field| field.binding_source == "body")
        .cloned()
        .collect::<Vec<_>>();
    let uses_json_binding = uses_http_json_binding(&body_fields);
    let args_fields = if uses_json_binding {
        fields
            .iter()
            .filter(|field| field.binding_source != "body")
            .cloned()
            .collect::<Vec<_>>()
    } else {
        fields.clone()
    };
    let method_name = http_controller_method_name(method, controller_base);
    let signature = if uses_json_binding && !args_fields.is_empty() {
        format!("args {}", http_args_type_name(method))
    } else if uses_json_binding {
        "ctx httpbinding.Context".to_string()
    } else if !args_fields.is_empty() {
        format!("args {}", http_args_type_name(method))
    } else {
        String::new()
    };

    let mut body = String::new();
    body.push_str(&format!(
        "func (c *{controller_type}) {method_name}({signature}) (any, error) {{\n"
    ));
    if uses_json_binding {
        let request_type = http_request_type_name(method);
        body.push_str(&format!("    var req {request_type}\n"));
        let bind_ctx = if args_fields.is_empty() {
            "ctx"
        } else {
            "args.Ctx"
        };
        body.push_str(&format!(
            "    if err := {bind_ctx}.ShouldBindJSON(&req); err != nil {{\n"
        ));
        body.push_str("        return nil, err\n");
        body.push_str("    }\n");
        for field in body_fields
            .iter()
            .filter(|field| is_union_go_type(entry, &field.go_type))
        {
            body.push_str(&format!(
                "    if err := req.{}.ValidateUnion(); err != nil {{\n        return nil, err\n    }}\n",
                field.go_name
            ));
        }
        body.push_str("    _ = req\n");
    }
    if !args_fields.is_empty() {
        body.push_str("    _ = args\n");
    }
    if method.oneway {
        let ctx_expr = if uses_json_binding && !args_fields.is_empty() {
            "args.Ctx"
        } else if uses_json_binding {
            "ctx"
        } else if !args_fields.is_empty() {
            "args.Ctx"
        } else {
            ""
        };
        if !ctx_expr.is_empty() {
            body.push_str(&format!("    {ctx_expr}.Status(http.StatusAccepted)\n"));
        }
    }
    body.push_str("    return nil, errors.New(\"TODO\")\n");
    body.push_str("}\n\n");
    Ok(body)
}

fn collect_http_fields(
    entry: &CompiledModuleEntry,
    method: &ContractMethod,
) -> Vec<HttpFieldRenderSpec> {
    resolve_http_method_fields(entry, method)
        .iter()
        .map(|field| {
            let inferred_path_binding = infer_http_path_binding_name(method, &field.name);
            let binding_source = field
                .http_binding
                .as_ref()
                .map(|binding| binding.source.clone())
                .or_else(|| inferred_path_binding.as_ref().map(|_| "param".to_string()))
                .unwrap_or_else(|| default_http_binding_source(method));
            let binding_name = field
                .http_binding
                .as_ref()
                .map(|binding| binding.name.clone())
                .or(inferred_path_binding)
                .unwrap_or_else(|| field.name.clone());
            HttpFieldRenderSpec {
                go_name: go_field_name(&field.name),
                go_type: sanitize_go_type(&field.ty),
                binding_source,
                binding_name,
                required: field.required,
                default_value: field.default_value.clone(),
                validation_rules: field.validation_rules.clone(),
            }
        })
        .collect()
}

fn resolve_http_method_fields(
    entry: &CompiledModuleEntry,
    method: &ContractMethod,
) -> Vec<ContractField> {
    entry
        .ir
        .types
        .iter()
        .find(|ty| ty.name == method.request)
        .map(|ty| ty.fields.clone())
        .unwrap_or_else(|| method.params.clone())
}

fn uses_http_json_binding(body_fields: &[HttpFieldRenderSpec]) -> bool {
    body_fields
        .iter()
        .any(|field| !field.validation_rules.is_empty())
}

fn is_union_go_type(entry: &CompiledModuleEntry, go_type: &str) -> bool {
    let normalized = sanitize_go_type(go_type);
    entry
        .ir
        .types
        .iter()
        .any(|ty| ty.kind == ContractTypeKind::Union && exported_name(&ty.name) == normalized)
}

fn default_http_binding_source(method: &ContractMethod) -> String {
    // 默认绑定来源遵循常见 HTTP 约定：
    // - GET / HEAD / OPTIONS 以 query 为主；
    // - 其余方法默认走 body。
    //
    // 如果字段本身声明了 http_binding，或路径参数推断命中，这个默认值会被覆盖。
    match method
        .http_method
        .clone()
        .unwrap_or_else(|| default_http_method(method))
        .as_str()
    {
        "GET" | "HEAD" | "OPTIONS" => "query".into(),
        _ => "body".into(),
    }
}

fn infer_http_path_binding_name(method: &ContractMethod, field_name: &str) -> Option<String> {
    // 尝试把请求字段映射为路径参数。
    //
    // 支持 `:id` 和 `{id}` 两种路径占位符写法，并通过归一化比较来容忍命名风格差异，
    // 例如 `user_id`、`userId`、`UserID` 最终都会按相同 key 比较。
    let path = method.http_path.as_deref()?;
    let normalized_field = normalize_http_binding_key(field_name);
    for segment in path.split('/') {
        if let Some(name) = segment.strip_prefix(':') {
            if normalize_http_binding_key(name) == normalized_field {
                return Some(name.to_string());
            }
        }
        if segment.starts_with('{') && segment.ends_with('}') {
            let name = &segment[1..segment.len() - 1];
            if normalize_http_binding_key(name) == normalized_field {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn normalize_http_binding_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn http_group_name(service: &ContractService, method: &ContractMethod) -> String {
    // HTTP 分组名决定了控制器目录、包名和注册边界。
    // 显式声明优先，缺失时才退回到 service 名称，目的是让默认输出尽量稳定可预测。
    method
        .http_handler_path
        .clone()
        .or_else(|| method.http_category.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| slugify(&service.name).replace('_', "-"))
}

fn http_group_package_name(group: &str) -> String {
    let segment = group
        .split('/')
        .next_back()
        .unwrap_or(group.trim_matches('/'))
        .trim_matches('/');
    let slug = slugify(segment).replace('-', "_");
    if slug.is_empty() {
        "httpgen".into()
    } else {
        slug
    }
}

fn http_group_import_alias(group: &str) -> String {
    format!("generated{}", go_field_name(&segmented_slug(group, "_")))
}

fn http_controller_type_name(group: &str) -> String {
    format!("{}Controller", http_controller_base_name(group))
}

fn http_controller_base_name(group: &str) -> String {
    exported_name(&singularize_group_name(
        group
            .split('/')
            .next_back()
            .unwrap_or(group.trim_matches('/')),
    ))
}

fn singularize_group_name(value: &str) -> String {
    let trimmed = value.trim_matches('/');
    if let Some(prefix) = trimmed.strip_suffix("ies") {
        return format!("{prefix}y");
    }
    if trimmed.len() > 1 && trimmed.ends_with('s') && !trimmed.ends_with("ss") {
        return trimmed[..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

fn http_builder_call(method: &ContractMethod) -> &'static str {
    match method
        .http_method
        .clone()
        .unwrap_or_else(|| default_http_method(method))
        .to_ascii_uppercase()
        .as_str()
    {
        "GET" => "Get",
        "POST" => "Post",
        "PUT" => "Put",
        "DELETE" => "Delete",
        "PATCH" => "Patch",
        "HEAD" => "Head",
        "OPTIONS" => "Options",
        _ => "Post",
    }
}

fn http_relative_path(service: &ContractService, method: &ContractMethod, group: &str) -> String {
    // 从完整 HTTP 路径里裁出相对于 group 的尾部路径。
    //
    // 例如 group 为 `users`、完整路径为 `/api/users/:id` 时，这里希望得到 `/:id`，
    // 这样注册器可以把 group 作为前缀、把 tail 作为局部路径继续拼装。
    // 如果裁剪失败，就回退到完整路径，保证生成代码仍然可用。
    let full_path = method
        .http_path
        .clone()
        .unwrap_or_else(|| default_http_path(service, method));
    let group_path = format!("/{}", group.trim_matches('/'));
    if let Some(index) = full_path.rfind(&group_path) {
        let mut tail = full_path[index + group_path.len()..].trim().to_string();
        if tail == "/" || tail.is_empty() {
            return String::new();
        }
        if !tail.starts_with('/') {
            tail.insert(0, '/');
        }
        return tail;
    }
    full_path
}

fn http_controller_method_name(method: &ContractMethod, controller_base: &str) -> String {
    // 控制器方法名会尽量去掉与 controller 自身重复的后缀，避免出现
    // `ListUsersUsers` 这类可读性差的命名。去重失败时则保留原始导出名。
    let name = exported_name(&method.name);
    let plural = format!("{controller_base}s");
    for suffix in [plural.as_str(), controller_base] {
        if name.len() > suffix.len() && name.ends_with(suffix) {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    name
}

fn http_request_type_name(method: &ContractMethod) -> String {
    format!("{}Request", unexported_name(&method.name))
}

fn http_args_type_name(method: &ContractMethod) -> String {
    format!("{}Args", unexported_name(&method.name))
}

fn http_method_file_name(method: &ContractMethod) -> String {
    segmented_slug(&method.name, "_")
}

fn unexported_name(value: &str) -> String {
    let exported = exported_name(value);
    let mut chars = exported.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}", first.to_ascii_lowercase(), chars.as_str())
}

fn append_exported_suffix(base: &str, suffix: &str) -> String {
    if base.ends_with(suffix) {
        base.to_string()
    } else {
        format!("{base}{suffix}")
    }
}

fn go_field_name(value: &str) -> String {
    let parts = value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if !parts.is_empty() {
        return parts.into_iter().map(go_public_name).collect::<String>();
    }
    go_public_name(value)
}

fn go_public_name(value: &str) -> String {
    if value.eq_ignore_ascii_case("id") {
        return "ID".into();
    }
    let normalized = if value.chars().all(|ch| !ch.is_ascii_lowercase()) {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    };
    let mut chars = normalized.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
}

fn join_csv(values: &[String]) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join(","))
    }
}

fn render_default_tag_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn render_go_tags(tags: &[(&str, Option<&str>)]) -> String {
    let items = tags
        .iter()
        .filter_map(|(name, value)| value.map(|value| format!(r#"{name}:"{value}""#)))
        .collect::<Vec<_>>();
    if items.is_empty() {
        String::new()
    } else {
        format!("`{}`", items.join(" "))
    }
}

pub(crate) fn render_module_registry_file(
    entries: &[CompiledModuleEntry],
    path: &Path,
) -> GeneratedContractPlan {
    let mut body = String::from("// Code generated by lania generate module. DO NOT EDIT.\n\n");
    body.push_str("package modules\n\n");
    // 注册表只保留模块名列表，避免在聚合文件里重复拷贝完整模块定义。
    body.push_str("func GeneratedModules() []string {\n    return []string{\n");
    for entry in entries {
        body.push_str(&format!(
            "        Build{}()[\"name\"].(string),\n",
            entry.module_name
        ));
    }
    body.push_str("    }\n}\n");
    GeneratedContractPlan {
        path: path.to_path_buf(),
        content: body,
        owner: None,
    }
}
