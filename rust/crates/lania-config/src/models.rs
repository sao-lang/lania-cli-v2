//! 配置系统的核心数据模型。
//!
//! 这里同时定义了几类不同层次的模型：
//! - 文档模型：`ConfigFieldDoc` / `LanConfigSchemaDoc`
//! - 校验模型：`ConfigValidationError` / `ConfigPosition`
//! - 快照模型：`LanConfigSnapshot` / `ToolConfigSnapshot` 以及它们的子结构
//! - 插件声明模型：`ConfigPluginRef`、信任等级、来源类型
//!
//! 新手读这个文件时，建议先区分“文档/校验/运行时快照”这三类用途。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigPluginSourceKind {
    Package,
    LocalPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigPluginTrustLevel {
    FirstParty,
    ProjectLocal,
    ReviewRequired,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigValueType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFieldDoc {
    pub path: String,
    pub value_type: ConfigValueType,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanConfigSchemaDoc {
    pub version: u32,
    pub search_places: Vec<String>,
    pub fields: Vec<ConfigFieldDoc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigValidationErrorCode {
    MissingField,
    InvalidType,
    InvalidValue,
    UnknownField,
    UnsupportedVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigPosition {
    pub path: String,
    pub line: Option<u64>,
    pub column: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigValidationError {
    pub code: ConfigValidationErrorCode,
    pub message: String,
    pub position: ConfigPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigMigrationPolicy {
    Compatible,
    RewriteRecommended,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigVersionStrategy {
    pub current_version: u32,
    pub minimum_compatible_version: u32,
    pub detected_version: u32,
    pub policy: ConfigMigrationPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseProfile {
    Package,
    WebApp,
    Service,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReleaseStepConfig {
    pub enabled: bool,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseVerifyConfig {
    pub lint: ReleaseStepConfig,
    pub test: ReleaseStepConfig,
    pub build: ReleaseStepConfig,
    pub smoke: ReleaseStepConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseVersioningConfig {
    pub enabled: bool,
    pub source: Option<String>,
    pub tag_prefix: String,
    pub command: Option<String>,
}

impl Default for ReleaseVersioningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            source: Some("package.json".into()),
            tag_prefix: "v".into(),
            command: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDeployConfig {
    pub provider: String,
    pub command: Option<String>,
}

impl Default for ReleaseDeployConfig {
    fn default() -> Self {
        Self {
            provider: "custom".into(),
            command: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePostCheckConfig {
    pub url: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseGitConfig {
    pub commit: bool,
    pub tag: bool,
    pub push: bool,
    pub remote: Option<String>,
    pub branch: Option<String>,
}

impl Default for ReleaseGitConfig {
    fn default() -> Self {
        Self {
            commit: true,
            tag: true,
            push: false,
            remote: Some("origin".into()),
            branch: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseConfigSnapshot {
    pub profile: ReleaseProfile,
    pub env: Option<String>,
    pub channel: Option<String>,
    pub state_file: String,
    pub verify: ReleaseVerifyConfig,
    pub versioning: ReleaseVersioningConfig,
    pub changelog: ReleaseStepConfig,
    pub artifact: ReleaseStepConfig,
    pub deploy: ReleaseDeployConfig,
    pub post_check: ReleasePostCheckConfig,
    pub git: ReleaseGitConfig,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigPluginRef {
    pub name: String,
    pub package: String,
    pub methods: Vec<String>,
    pub declared_as: String,
    pub source_kind: ConfigPluginSourceKind,
    pub trust_level: ConfigPluginTrustLevel,
    pub loadable: bool,
    pub reason: Option<String>,
}

impl ConfigPluginRef {
    pub fn requires_review(&self) -> bool {
        // 可加载但需要人工确认：通常是“看起来像包名”的第三方插件。
        self.loadable && self.trust_level == ConfigPluginTrustLevel::ReviewRequired
    }

    pub fn is_rejected(&self) -> bool {
        // 不可加载的统一视为 rejected（包括信任等级显式拒绝或校验失败）。
        !self.loadable || self.trust_level == ConfigPluginTrustLevel::Rejected
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LanExtensionsSnapshot {
    pub dynamic_commands: bool,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiscoverySnapshot {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub allow_extensions: Vec<String>,
    pub raw: Value,
}

impl Default for SchemaDiscoverySnapshot {
    fn default() -> Self {
        Self {
            files: vec![
                "lania.schemas.ts".into(),
                "lania.schemas.js".into(),
                "lania.schemas.cjs".into(),
            ],
            dirs: vec![".lania/schemas".into()],
            allow_extensions: vec![
                ".ts".into(),
                ".js".into(),
                ".cjs".into(),
                ".json".into(),
                ".yaml".into(),
                ".yml".into(),
            ],
            raw: Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookBindingKind {
    Waterfall,
    Parallel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookBindingSource {
    Plugin,
    Inline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HookBinding {
    pub r#type: Option<HookBindingSource>,
    pub kind: Option<HookBindingKind>,
    pub plugin: Option<String>,
    pub handler: Option<String>,
    pub timeout_ms: Option<u64>,
    pub on_error: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiOutputSnapshot {
    pub mode: String,
    pub raw: Value,
}

impl Default for UiOutputSnapshot {
    fn default() -> Self {
        Self {
            mode: "json".into(),
            raw: Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProgressSnapshot {
    pub style: String,
    pub grouping: String,
    pub raw: Value,
}

impl Default for UiProgressSnapshot {
    fn default() -> Self {
        Self {
            style: "spinner".into(),
            grouping: "command".into(),
            raw: Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiInteractionSnapshot {
    pub mode: String,
    pub timeout_ms: Option<u64>,
    pub default_strategy: String,
    pub raw: Value,
}

impl Default for UiInteractionSnapshot {
    fn default() -> Self {
        Self {
            mode: "auto".into(),
            timeout_ms: None,
            default_strategy: "use_defaults".into(),
            raw: Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UiSnapshot {
    pub locale: Option<String>,
    pub output: UiOutputSnapshot,
    pub progress: UiProgressSnapshot,
    pub interaction: UiInteractionSnapshot,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommandsSnapshot {
    pub aliases: BTreeMap<String, String>,
    pub shortcuts: BTreeMap<String, String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeCommandOverride {
    pub build: Option<Vec<String>>,
    pub dev: Option<Vec<String>>,
    pub package_manager: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanConfigSnapshot {
    pub cwd: String,
    pub config_path: Option<String>,
    pub exists: bool,
    pub supported_extensions: Vec<String>,
    pub build_tool: String,
    pub build_adaptors: BTreeMap<String, Value>,
    pub lint_adaptors: BTreeMap<String, Value>,
    pub lint_tools: Vec<String>,
    /// 显式声明的运行时；`None` 表示按特征文件自动探测。
    ///
    /// 之前只能从 `raw` 里摸（`snapshot.raw["runtime"]`），
    /// 导致每个消费者都要自己判空 + 转型。
    pub runtime: Option<String>,
    /// 每个运行时的 build/dev 命令覆盖。
    pub runtimes: BTreeMap<String, RuntimeCommandOverride>,
    pub plugins: Vec<ConfigPluginRef>,
    pub extensions: LanExtensionsSnapshot,
    pub ui: UiSnapshot,
    pub schema_discovery: SchemaDiscoverySnapshot,
    pub commands: CommandsSnapshot,
    pub hooks: BTreeMap<String, Vec<HookBinding>>,
    pub release: Option<ReleaseConfigSnapshot>,
    pub custom: Value,
    pub schema_version: u32,
    pub version_strategy: ConfigVersionStrategy,
    pub validation_errors: Vec<ConfigValidationError>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolConfigSnapshot {
    pub cwd: String,
    pub tool: String,
    pub config_path: Option<String>,
    pub exists: bool,
    pub resolved: bool,
    pub validation_errors: Vec<ConfigValidationError>,
    pub raw: Value,
}
