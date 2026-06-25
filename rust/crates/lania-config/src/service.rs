//! 配置子系统对外暴露的服务入口。
//!
//! 这个文件把两类能力集中暴露给外部：
//! - “文档能力”：输出配置 schema 的结构化说明或 markdown 文档
//! - “加载能力”：把 bridge 返回的 payload 规范化成 `LanConfigSnapshot` / `ToolConfigSnapshot`
//!
//! 也可以把它看成 `lania-config` crate 的 Facade。

use crate::{
    discovery, normalize, ConfigFieldDoc, ConfigValueType, LanConfigSchemaDoc, LanConfigSnapshot,
    Result, ToolConfigSnapshot, Value, CURRENT_LAN_CONFIG_VERSION,
};

#[derive(Debug, Clone, Default)]
pub struct ConfigService;

impl ConfigService {
    pub fn lan_schema_doc() -> LanConfigSchemaDoc {
        // 这里返回的是“配置文档描述对象”，而不是实际配置值。
        // 它主要服务于：
        // - `config` 类命令输出 schema 文档
        // - IDE / 文档生成 / 测试中对字段清单的统一复用
        LanConfigSchemaDoc {
            version: CURRENT_LAN_CONFIG_VERSION,
            search_places: discovery::lan_config_search_places(),
            fields: vec![
                ConfigFieldDoc {
                    path: "$.version".into(),
                    value_type: ConfigValueType::Number,
                    required: false,
                    description: "schema version used for migration compatibility".into(),
                },
                ConfigFieldDoc {
                    path: "$.buildTool".into(),
                    value_type: ConfigValueType::String,
                    required: false,
                    description: "primary build tool name such as vite/webpack/rollup".into(),
                },
                ConfigFieldDoc {
                    path: "$.buildAdaptors".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "compiler adaptor configuration map".into(),
                },
                ConfigFieldDoc {
                    path: "$.lintAdaptors".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "linter adaptor configuration map".into(),
                },
                ConfigFieldDoc {
                    path: "$.lintTools".into(),
                    value_type: ConfigValueType::Array,
                    required: false,
                    description: "enabled lint tools in execution order".into(),
                },
                ConfigFieldDoc {
                    path: "$.plugins".into(),
                    value_type: ConfigValueType::Array,
                    required: false,
                    description: "plugin declarations as package name or object".into(),
                },
                ConfigFieldDoc {
                    path: "$.extensions".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "v2.1 extensions such as dynamicCommands toggles".into(),
                },
                ConfigFieldDoc {
                    path: "$.ui".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "terminal ui settings for output, progress, and interaction"
                        .into(),
                },
                ConfigFieldDoc {
                    path: "$.ui.locale".into(),
                    value_type: ConfigValueType::String,
                    required: false,
                    description: "ui locale override (en|zh)".into(),
                },
                ConfigFieldDoc {
                    path: "$.ui.output".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "output mode and event streaming strategy".into(),
                },
                ConfigFieldDoc {
                    path: "$.ui.progress".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "progress rendering style".into(),
                },
                ConfigFieldDoc {
                    path: "$.ui.interaction".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "interactive/non-interactive prompt behavior".into(),
                },
                ConfigFieldDoc {
                    path: "$.extensions.dynamicCommands".into(),
                    value_type: ConfigValueType::Boolean,
                    required: false,
                    description: "enable schema-driven runtime commands".into(),
                },
                ConfigFieldDoc {
                    path: "$.schemaDiscovery".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "override runtime schema manifest discovery files and directories"
                        .into(),
                },
                ConfigFieldDoc {
                    path: "$.hooks".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "formal v2.1 hook bindings keyed by onXxx hook names".into(),
                },
                ConfigFieldDoc {
                    path: "$.commands".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "command aliases and optional shell shortcuts".into(),
                },
                ConfigFieldDoc {
                    path: "$.custom".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "custom project-specific config passed through to plugins".into(),
                },
                ConfigFieldDoc {
                    path: "$.release".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "project release orchestration config".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.profile".into(),
                    value_type: ConfigValueType::String,
                    required: false,
                    description: "release profile: package/web_app/service/custom".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.env".into(),
                    value_type: ConfigValueType::String,
                    required: false,
                    description: "target environment name".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.channel".into(),
                    value_type: ConfigValueType::String,
                    required: false,
                    description: "release channel or dist-tag".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.stateFile".into(),
                    value_type: ConfigValueType::String,
                    required: false,
                    description: "release state file path for resume/status".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.verify".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "verify stage config for lint/test/build/smoke".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.versioning".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "version/tag strategy and optional custom command".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.deploy".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "deploy adapter or custom deploy command".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.postCheck".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "post-release health check url or command".into(),
                },
                ConfigFieldDoc {
                    path: "$.release.git".into(),
                    value_type: ConfigValueType::Object,
                    required: false,
                    description: "finalize git behavior for commit/tag/push".into(),
                },
                ConfigFieldDoc {
                    path: "$.pluginAllowlist".into(),
                    value_type: ConfigValueType::Array,
                    required: false,
                    description: "explicitly trusted third-party package plugins".into(),
                },
                ConfigFieldDoc {
                    path: "$.pluginMethodAllowlist".into(),
                    value_type: ConfigValueType::Array,
                    required: false,
                    description: "allowed bridge methods exposed by runtime plugins".into(),
                },
                ConfigFieldDoc {
                    path: "$.pluginTrustedSources".into(),
                    value_type: ConfigValueType::Array,
                    required: false,
                    description: "allowed plugin source kinds: package and/or local_path".into(),
                },
                ConfigFieldDoc {
                    path: "$.pluginRequireSignature".into(),
                    value_type: ConfigValueType::Boolean,
                    required: false,
                    description:
                        "require third-party package plugins to declare a trusted signature".into(),
                },
                ConfigFieldDoc {
                    path: "$.pluginSignatureAllowlist".into(),
                    value_type: ConfigValueType::Array,
                    required: false,
                    description: "package plugins trusted without inline signature metadata".into(),
                },
            ],
        }
    }

    pub fn lan_schema_markdown() -> String {
        // markdown 只是 schema doc 的一种渲染形式。
        // 先构造结构化 doc，再渲染成文本，比直接手写 markdown 更容易维护。
        let doc = Self::lan_schema_doc();
        let mut sections = vec![
            format!("# lan.config schema v{}", doc.version),
            String::new(),
            "## Search Places".into(),
        ];
        sections.extend(doc.search_places.iter().map(|place| format!("- `{place}`")));
        sections.push(String::new());
        sections.push("## Fields".into());
        sections.extend(doc.fields.iter().map(|field| {
            format!(
                "- `{}`: {:?}{} - {}",
                field.path,
                field.value_type,
                if field.required { " (required)" } else { "" },
                field.description
            )
        }));
        sections.join("\n")
    }

    pub fn load_lan_snapshot(payload: &Value) -> Result<LanConfigSnapshot> {
        // 真正的“重逻辑”在 `normalize` 模块：
        // service 层只暴露稳定入口，减少外部直接依赖内部 normalize 细节。
        normalize::load_lan_snapshot(payload)
    }

    pub fn load_tool_snapshot(payload: &Value) -> Result<ToolConfigSnapshot> {
        normalize::load_tool_snapshot(payload)
    }
}
