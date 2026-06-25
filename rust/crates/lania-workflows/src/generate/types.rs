//! generate 子流程共享的类型定义与中间态结构。
//!
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ContractConfig {
    // `ContractConfig` / `ModuleConfig` 这批结构体本质上就是“配置文件的 Rust 镜像”。
    // 它们的主要职责不是做业务逻辑，而是把 YAML/JSON 反序列化成更强类型的中间表示。
    pub(crate) version: u32,
    pub(crate) defaults: Option<ContractDefaults>,
    pub(crate) entries: Vec<ContractEntryConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContractDefaults {
    pub(crate) language: Option<String>,
    pub(crate) output: Option<ContractOutputConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContractOutputConfig {
    pub(crate) contract_dir: Option<String>,
    pub(crate) transport_dir: Option<String>,
    pub(crate) module_file: Option<String>,
    pub(crate) manifest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ContractEntryConfig {
    pub(crate) name: String,
    pub(crate) source: ContractSourceConfig,
    pub(crate) targets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ContractSourceConfig {
    pub(crate) kind: String,
    pub(crate) inputs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ModuleConfig {
    pub(crate) version: u32,
    pub(crate) framework: ModuleFrameworkConfig,
    #[serde(default)]
    pub(crate) inputs: Vec<ModuleInputConfig>,
    #[serde(default)]
    pub(crate) targets: Vec<ModuleTargetConfig>,
    pub(crate) output: Option<ModuleOutputConfig>,
    pub(crate) inject: Option<ModuleInjectConfig>,
    pub(crate) overrides: Option<ModuleOverridesConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ModuleFrameworkConfig {
    pub(crate) name: String,
    pub(crate) language: Option<String>,
    pub(crate) main: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ModuleInputConfig {
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) include: Vec<String>,
    /// 每个 input 可选的 targets 覆盖项。
    /// 如果为空，则回退到顶层 `targets` 的推导结果。
    #[serde(default)]
    pub(crate) targets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ModuleTargetConfig {
    pub(crate) kind: String,
    pub(crate) enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleOutputConfig {
    pub(crate) root: Option<String>,
    pub(crate) module_dir: Option<String>,
    pub(crate) adapter_dir: Option<String>,
    pub(crate) contract_dir: Option<String>,
    pub(crate) http_root_dir: Option<String>,
    pub(crate) grpc_root_dir: Option<String>,
    pub(crate) manifest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ModuleInjectConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) target_main: Option<String>,
    pub(crate) strategy: Option<String>,
    pub(crate) marker: Option<ModuleInjectMarkerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ModuleInjectMarkerConfig {
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ModuleOverridesConfig {
    #[serde(default)]
    pub(crate) operations: BTreeMap<String, ModuleOperationOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ModuleOperationOverride {
    pub(crate) service: Option<String>,
    pub(crate) input: Option<String>,
    pub(crate) output: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) http: Option<ModuleHttpOverride>,
    pub(crate) ws: Option<ModuleWsOverride>,
    pub(crate) graphql: Option<ModuleGraphqlOverride>,
    pub(crate) grpc: Option<ModuleGrpcOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ModuleHttpOverride {
    pub(crate) method: Option<String>,
    pub(crate) path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ModuleWsOverride {
    pub(crate) namespace: Option<String>,
    pub(crate) event: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ModuleGraphqlOverride {
    pub(crate) kind: Option<String>,
    pub(crate) field: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ModuleGrpcOverride {
    pub(crate) service: Option<String>,
    pub(crate) method: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ContractManifest {
    pub(crate) version: u32,
    pub(crate) config_path: String,
    pub(crate) module_file: Option<String>,
    pub(crate) entries: BTreeMap<String, ContractManifestEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ContractManifestEntry {
    pub(crate) source_kind: String,
    pub(crate) targets: Vec<String>,
    pub(crate) input_hash: u64,
    pub(crate) ir_hash: u64,
    pub(crate) outputs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ModuleManifest {
    pub(crate) version: u32,
    pub(crate) config_path: String,
    pub(crate) framework: String,
    pub(crate) shared_outputs: Vec<String>,
    pub(crate) entries: BTreeMap<String, ModuleManifestEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ModuleManifestEntry {
    pub(crate) source_kind: String,
    pub(crate) targets: Vec<String>,
    pub(crate) input_hash: u64,
    pub(crate) ir_hash: u64,
    pub(crate) outputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledContractEntry {
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) source_kind: String,
    pub(crate) targets: Vec<String>,
    pub(crate) input_paths: Vec<PathBuf>,
    pub(crate) ir: ContractIr,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledModuleEntry {
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) source_kind: String,
    pub(crate) targets: Vec<String>,
    pub(crate) input_paths: Vec<PathBuf>,
    pub(crate) ir: ContractIr,
    pub(crate) module_name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ContractIr {
    // `ContractIr` 是 generate 子系统最重要的中间层：
    // 不管输入源是 proto / thrift / json schema / graphql，
    // 最终都会先转成统一的 IR，再交给后面的渲染层生成代码。
    pub(crate) aliases: Vec<ContractAlias>,
    pub(crate) consts: Vec<ContractConst>,
    pub(crate) enums: Vec<ContractEnum>,
    pub(crate) types: Vec<ContractType>,
    pub(crate) services: Vec<ContractService>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractAlias {
    pub(crate) name: String,
    pub(crate) target: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractConst {
    pub(crate) name: String,
    pub(crate) ty: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractEnum {
    pub(crate) name: String,
    pub(crate) variants: Vec<ContractEnumVariant>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractEnumVariant {
    pub(crate) name: String,
    pub(crate) value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ContractTypeKind {
    #[default]
    Struct,
    Union,
    Exception,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractType {
    pub(crate) name: String,
    pub(crate) kind: ContractTypeKind,
    pub(crate) fields: Vec<ContractField>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractField {
    pub(crate) name: String,
    pub(crate) ty: String,
    pub(crate) required: bool,
    pub(crate) optional: bool,
    pub(crate) oneof_group: Option<String>,
    #[allow(dead_code)]
    pub(crate) default_value: Option<String>,
    pub(crate) http_binding: Option<ContractHttpFieldBinding>,
    pub(crate) validation_rules: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractService {
    pub(crate) name: String,
    pub(crate) extends: Option<String>,
    pub(crate) grpc_metadata: ContractGrpcMetadata,
    pub(crate) methods: Vec<ContractMethod>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractMethod {
    pub(crate) name: String,
    pub(crate) request: String,
    pub(crate) response: String,
    pub(crate) streaming: ContractStreamingMode,
    pub(crate) params: Vec<ContractField>,
    pub(crate) throws: Vec<ContractField>,
    pub(crate) oneway: bool,
    pub(crate) kind: String,
    pub(crate) http_method: Option<String>,
    pub(crate) http_path: Option<String>,
    pub(crate) http_handler_path: Option<String>,
    pub(crate) http_category: Option<String>,
    pub(crate) gql_kind: Option<String>,
    pub(crate) gql_field: Option<String>,
    pub(crate) ws_event: Option<String>,
    pub(crate) ws_namespace: Option<String>,
    pub(crate) grpc_metadata: ContractGrpcMetadata,
    pub(crate) grpc_service: Option<String>,
    pub(crate) grpc_method: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ContractStreamingMode {
    #[default]
    Unary,
    Server,
    Client,
    Bidi,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ContractGrpcMetadata {
    pub(crate) deprecated: bool,
    pub(crate) idempotency_level: Option<String>,
    pub(crate) options: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContractHttpFieldBinding {
    pub(crate) source: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GeneratedContractPlan {
    pub(crate) path: PathBuf,
    pub(crate) content: String,
    pub(crate) owner: Option<String>,
}

#[derive(Debug)]
pub(crate) struct PreparedGeneratePlan {
    // `PreparedGeneratePlan` / `PreparedGenerateModulePlan` 表示“准备阶段已经完成”的结果。
    // 后续 apply/diff/check 都基于它们工作，避免重复读配置、重复解析 schema。
    pub(crate) config_path: PathBuf,
    pub(crate) config_dir: PathBuf,
    pub(crate) manifest_path: PathBuf,
    pub(crate) manifest: ContractManifest,
    pub(crate) selected_entries: Vec<CompiledContractEntry>,
    pub(crate) generated_plans: Vec<GeneratedContractPlan>,
    pub(crate) language: String,
}

#[derive(Debug)]
pub(crate) struct PreparedGenerateModulePlan {
    pub(crate) config_path: PathBuf,
    pub(crate) config_dir: PathBuf,
    pub(crate) manifest_path: PathBuf,
    pub(crate) manifest: ModuleManifest,
    pub(crate) all_entries: Vec<CompiledModuleEntry>,
    pub(crate) selected_entries: Vec<CompiledModuleEntry>,
    pub(crate) generated_plans: Vec<GeneratedContractPlan>,
    pub(crate) language: String,
    pub(crate) framework: String,
    pub(crate) planning_notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ModuleOutputPaths {
    pub(crate) module_dir: PathBuf,
    pub(crate) adapter_dir: PathBuf,
    pub(crate) contract_dir: PathBuf,
    pub(crate) http_root_dir: Option<PathBuf>,
    pub(crate) http_root_import: Option<String>,
    pub(crate) grpc_root_dir: Option<PathBuf>,
    pub(crate) grpc_root_import: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct ContractPlanSummary {
    // 这是生成器常见的一种“四分法”结果：
    // - to_write：应该写的新/变更文件
    // - unchanged：内容没变，可跳过
    // - conflicts：已有 unmanaged 文件，不能直接覆盖
    // - stale：旧的生成文件，可以按 clean 策略删除
    pub(crate) to_write: Vec<PathBuf>,
    pub(crate) unchanged: Vec<PathBuf>,
    pub(crate) conflicts: Vec<PathBuf>,
    pub(crate) stale: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub(crate) struct ContractWriteOutcome {
    pub(crate) written: Vec<PathBuf>,
    pub(crate) conflicts: Vec<PathBuf>,
    pub(crate) skipped: Vec<PathBuf>,
    pub(crate) removed: Vec<PathBuf>,
}
