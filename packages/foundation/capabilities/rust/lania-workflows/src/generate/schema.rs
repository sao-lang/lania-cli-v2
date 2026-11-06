//! generate API / module 共享的 schema 解析门面。
//!
//! 这一层只负责按职责组织子模块并维持历史导出接口：
//! - `normalize`：source/target/filter 归一化
//! - `json_schema` / `graphql` / `proto` / `thrift`：各类 schema parser
//! - `shared`：解析器共用的小工具

mod graphql;
mod json_schema;
mod normalize;
mod proto;
mod shared;
mod thrift;

#[allow(unused_imports)]
pub(crate) use super::schema_utils::{
    exported_name, proto_scalar_to_go, render_contract_go, render_transport_go, sanitize_go_type,
    slugify, stable_hash, strip_inline_comment, thrift_scalar_to_go,
};
#[allow(unused_imports)]
pub(crate) use graphql::{
    graphql_type_name, graphql_type_to_go, parse_graphql_argument_field, parse_graphql_field,
    parse_graphql_operation, parse_graphql_schema_contract,
};
#[allow(unused_imports)]
pub(crate) use json_schema::{
    json_schema_type_to_go, parse_json_schema_contract, parse_json_schema_node,
};
#[allow(unused_imports)]
pub(crate) use normalize::{
    normalize_source_filter, normalize_source_kind, normalize_target_filter, normalize_target_kind,
};
#[allow(unused_imports)]
pub(crate) use proto::{
    parse_proto_contract, parse_proto_contract_from_files, parse_proto_field, parse_proto_method,
};
pub(crate) use shared::segmented_slug;
#[allow(unused_imports)]
pub(crate) use thrift::{
    parse_thrift_contract, parse_thrift_contract_from_path, parse_thrift_field, parse_thrift_method,
};
