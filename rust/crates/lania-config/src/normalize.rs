//! 把 node-bridge 返回的原始配置 payload 规范化成 Rust 侧稳定可用的 snapshot。
//!
//! 为什么需要 `normalize` 这一层？
//! - 上游 payload 可能来自 `lan.config.ts/js/json/yaml`，形态并不完全一致；
//! - 某些字段是 bridge 运行时“推断出来”的，不一定直接存在于原始配置里；
//! - Rust 宿主希望下游 workflow/host 看到的是统一结构，而不是到处手写 JSON 取值逻辑。
//!
//! 可以把这里理解成“边界层”：
//! - 输入：松散的 `serde_json::Value`
//! - 输出：强类型 `LanConfigSnapshot` / `ToolConfigSnapshot`

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::{LanConfigSnapshot, ToolConfigSnapshot, CURRENT_LAN_CONFIG_VERSION};
use crate::parse_plugins::parse_plugin_ref;
use crate::parse_snapshots::{
    parse_commands_snapshot, parse_extensions_snapshot, parse_hook_bindings_snapshot,
    parse_release_snapshot, parse_schema_discovery_snapshot, parse_ui_snapshot,
};
use crate::parse_utils::as_object_map;
use crate::version::version_strategy;

fn prefer_config_value<'a>(payload: &'a Value, raw: &'a Value, key: &str) -> Option<&'a Value> {
    payload.get(key).or_else(|| raw.get(key))
}

pub(crate) fn load_lan_snapshot(payload: &Value) -> Result<LanConfigSnapshot> {
    // payload 来自 node-bridge 的 `config.loadLan`：
    // - payload["config"] 是原始配置（可能来自 js/json/ts/yaml）
    // - payload 中也可能带一些“计算字段”（buildTool/buildAdaptors/...），优先使用 payload。
    let cwd = payload["cwd"]
        .as_str()
        .ok_or_else(|| anyhow!("config.loadLan payload missing cwd"))?
        .to_string();
    let raw = payload["config"].clone();
    let schema_version = raw["version"]
        .as_u64()
        .map(|version| version as u32)
        .unwrap_or(CURRENT_LAN_CONFIG_VERSION);
    // build_tool 的来源优先级：
    // 1) node-bridge 推断出的 buildTool（更“可信”，可包含默认值与环境推断）
    // 2) 配置文件里显式写的 buildTool
    // 3) fallback 为 vite（保持旧项目可用）
    let build_tool = prefer_config_value(payload, &raw, "buildTool")
        .and_then(Value::as_str)
        .unwrap_or("vite")
        .to_string();
    // adaptor 字段必须是 object，否则会被 normalize 成空 map（避免整个 snapshot 构建失败）。
    let build_adaptors = as_object_map(payload["buildAdaptors"].clone())
        .or_else(|_| as_object_map(raw["buildAdaptors"].clone()))
        .unwrap_or_default();
    let lint_adaptors = as_object_map(payload["lintAdaptors"].clone())
        .or_else(|_| as_object_map(raw["lintAdaptors"].clone()))
        .unwrap_or_default();
    let lint_tools = payload["lintTools"]
        .as_array()
        .or_else(|| raw["lintTools"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Vec<String>>();
    let plugins = payload["plugins"]
        .as_array()
        .or_else(|| raw["plugins"].as_array())
        .into_iter()
        .flatten()
        .cloned()
        // parse_plugin_ref 做“弱解析”：单个插件条目格式错误不会炸掉整个配置，
        // 但会在后续的校验/报告里体现（对用户更友好）。
        .filter_map(|value| parse_plugin_ref(&value).ok())
        .collect::<Vec<crate::ConfigPluginRef>>();
    // 这些子结构的解析拆到 `parse_snapshots` 中：
    // - 好处是每个 section（ui/hooks/release/commands/...）可以独立维护
    // - normalize 入口则只保留“装配顺序”和“优先级规则”
    let extensions = parse_extensions_snapshot(prefer_config_value(payload, &raw, "extensions"));
    let ui = parse_ui_snapshot(prefer_config_value(payload, &raw, "ui"));
    let schema_discovery =
        parse_schema_discovery_snapshot(prefer_config_value(payload, &raw, "schemaDiscovery"));
    let commands = parse_commands_snapshot(prefer_config_value(payload, &raw, "commands"));
    let hooks = parse_hook_bindings_snapshot(prefer_config_value(payload, &raw, "hooks"));
    let release = parse_release_snapshot(prefer_config_value(payload, &raw, "release"));

    Ok(LanConfigSnapshot {
        cwd,
        config_path: payload["configPath"].as_str().map(ToOwned::to_owned),
        exists: payload["exists"].as_bool().unwrap_or(false),
        supported_extensions: payload["supportedExtensions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect::<Vec<String>>(),
        build_tool,
        build_adaptors,
        lint_adaptors,
        lint_tools,
        plugins,
        extensions,
        ui,
        schema_discovery,
        commands,
        hooks,
        release,
        custom: prefer_config_value(payload, &raw, "custom")
            .cloned()
            .unwrap_or(Value::Null),
        schema_version,
        // `schema_version` 决定“应该按哪套兼容策略理解这份配置”。
        // 把它预先转成 `version_strategy`，可以减少后续各处重复判断版本号。
        version_strategy: version_strategy(schema_version),
        // validation_errors 不影响加载成功：它是“配置可用，但有问题”的诊断信息。
        // 这样 CLI 能继续跑（尤其是迁移期），同时把错误展示给用户/上报给 hooks。
        //
        // 这也体现了 normalize/validate 的分工：
        // - normalize 负责“尽量把东西装配成可消费快照”
        // - validate 负责“指出哪里不符合规范”
        // 二者不是互斥关系，很多时候会同时发生。
        validation_errors: super::validate::validate_lan_config(&raw, schema_version),
        raw,
    })
}

pub(crate) fn load_tool_snapshot(payload: &Value) -> Result<ToolConfigSnapshot> {
    // Tool config 比 lan config 简单很多：
    // 它更像“某个工具的最终 resolved 配置结果”，因此这里只做最薄的一层提取与校验。
    // 换句话说，tool snapshot 更接近“读取结果对象”，而不是像 lan snapshot 那样承担大量兼容装配职责。
    Ok(ToolConfigSnapshot {
        cwd: payload["cwd"]
            .as_str()
            .ok_or_else(|| anyhow!("config.loadTool payload missing cwd"))?
            .to_string(),
        tool: payload["tool"]
            .as_str()
            .ok_or_else(|| anyhow!("config.loadTool payload missing tool"))?
            .to_string(),
        config_path: payload["configPath"].as_str().map(ToOwned::to_owned),
        exists: payload["exists"].as_bool().unwrap_or(false),
        resolved: payload["resolved"].as_bool().unwrap_or(false),
        // 工具配置的 validation 只针对 payload["config"]（已经是最终 resolved 结果），
        // 不对 package.json 的“来源形式”做区分。
        validation_errors: super::validate::validate_tool_config(&payload["config"]),
        raw: payload["config"].clone(),
    })
}
