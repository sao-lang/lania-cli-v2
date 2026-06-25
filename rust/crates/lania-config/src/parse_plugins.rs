//! 插件声明解析与信任分级。
//!
//! 这个模块把配置里的插件声明（字符串或对象）转换成 `ConfigPluginRef`，
//! 同时做一层非常重要的“来源分类/信任分级”：
//! - first-party 包
//! - project local 相对路径
//! - 需要 review 的第三方包
//! - 明确拒绝的声明（例如绝对路径）
//!
//! 这也是配置系统里少数带有“安全边界”意味的解析逻辑。

use crate::{
    anyhow, ConfigPluginRef, ConfigPluginSourceKind, ConfigPluginTrustLevel, Result, Value,
};

type JsonObject = serde_json::Map<String, Value>;

fn object_value<'a>(object: &'a JsonObject, key: &str) -> Option<&'a Value> {
    object.get(key)
}

pub(crate) fn parse_plugin_ref(value: &Value) -> Result<ConfigPluginRef> {
    if let Some(package) = value.as_str() {
        // 简写："@scope/pkg" 等字符串视作 package 声明。
        return Ok(classify_plugin_ref(
            package.to_string(),
            package.to_string(),
            vec![],
        ));
    }

    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("plugin entry must be string or object"))?;
    let declared_as = object_value(object, "package")
        .or_else(|| object_value(object, "name"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("plugin entry missing package"))?
        .to_string();
    // 这里把 `package` 和 `name` 区分开：
    // - `declared_as` 代表配置里真正写了什么，用于安全判断/加载
    // - `name` 更像展示名，允许和 package 相同，也允许省略后回退成 package

    Ok(classify_plugin_ref(
        object_value(object, "name")
            .and_then(|value| value.as_str())
            .unwrap_or(&declared_as)
            .to_string(),
        declared_as,
        object_value(object, "methods")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect::<Vec<String>>(),
    ))
}

pub(crate) fn classify_plugin_ref(
    name: String,
    declared_as: String,
    methods: Vec<String>,
) -> ConfigPluginRef {
    if declared_as.starts_with("./") || declared_as.starts_with("../") {
        // 本地相对路径：允许在项目内加载，但必须限制扩展名，避免误加载非 JS/TS 文件。
        let valid_extension = [".js", ".cjs", ".mjs", ".ts"]
            .iter()
            .any(|suffix| declared_as.ends_with(suffix));
        return ConfigPluginRef {
            name,
            package: declared_as.clone(),
            methods,
            declared_as,
            source_kind: ConfigPluginSourceKind::LocalPath,
            trust_level: if valid_extension {
                ConfigPluginTrustLevel::ProjectLocal
            } else {
                ConfigPluginTrustLevel::Rejected
            },
            loadable: valid_extension,
            reason: (!valid_extension)
                .then_some("local plugin path must end with .js, .cjs, .mjs or .ts".into()),
        };
    }

    if declared_as.starts_with('/') {
        // 绝对路径会绕过 workspace 边界，默认拒绝。
        return ConfigPluginRef {
            name,
            package: declared_as.clone(),
            methods,
            declared_as,
            source_kind: ConfigPluginSourceKind::LocalPath,
            trust_level: ConfigPluginTrustLevel::Rejected,
            loadable: false,
            reason: Some("absolute plugin paths are not allowed".into()),
        };
    }

    let has_whitespace = declared_as.chars().any(char::is_whitespace);
    let looks_like_package = !declared_as.is_empty() && !has_whitespace;
    // 这里的 “looks_like_package” 判断非常保守：
    // 只排除明显不合法的声明，不尝试完整复刻 npm 包名规范。
    // 更严格的来源审计应该放在后续安装/加载策略里，而不是把 parser 做成过重的校验器。
    // 包插件的信任分级：
    // - `@lania/plugin-*` 视作 first-party
    // - 其它“看起来像包名”的默认需要 review
    // - 否则视作无效声明并拒绝
    let trust_level = if declared_as.starts_with("@lania/plugin-") {
        ConfigPluginTrustLevel::FirstParty
    } else if looks_like_package {
        ConfigPluginTrustLevel::ReviewRequired
    } else {
        ConfigPluginTrustLevel::Rejected
    };

    ConfigPluginRef {
        name,
        package: declared_as.clone(),
        methods,
        declared_as,
        source_kind: ConfigPluginSourceKind::Package,
        trust_level: trust_level.clone(),
        loadable: trust_level != ConfigPluginTrustLevel::Rejected,
        reason: (trust_level == ConfigPluginTrustLevel::Rejected)
            .then_some("package plugin declaration is invalid".into()),
    }
}
