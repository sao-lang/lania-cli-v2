//! `command-build` 的桥接请求（BridgeRequest）构造。
//!
//! 目标：
//! - 把 CLI 的 argv/options 转成 node-bridge 能理解的 `method + params`
//! - 统一处理 cwd/path 兼容规则（legacy 选项）
//! - 保持 product distribution 的参数命名与 JS 侧实现一致（camelCase 等）

use lania_command::CommandContext;
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};
use serde_json::{Map, Value};
use std::path::PathBuf;

use crate::BuildCommandPlugin;

// Bridge request 构造：
// - 统一 cwd/path 兼容处理
// - CLI option 名称 -> bridge param 名称映射（例如 output-dir -> outputDir）
// - product build/pack/publish/inspect/doctor 共享一套 request 组装逻辑

impl BuildCommandPlugin {
    pub fn build_request(context: &CommandContext, bridge: &NodeBridgeClient) -> BridgeRequest {
        // Legacy 兼容字段：
        // - `config` / `path` 可能被某些旧 bridge 使用（或仅作为 hint），即使新实现忽略也需要保留。
        let config_path = string_option_value(context, "config");
        let path = string_option_value(context, "path");
        let cwd = resolve_cwd_from_path_option(&context.cwd, path.as_deref());
        let watch = bool_option_value(context, "watch", false);
        let mode = string_option_value(context, "mode");
        let output_dir = string_option_value(context, "output-dir");

        let mut request = bridge.compiler_build_with_options_request(cwd, watch, mode, output_dir);
        request.params["config"] = serde_json::json!(config_path);
        request.params["path"] = serde_json::json!(path);
        request
    }

    pub fn build_product_request(
        context: &CommandContext,
        bridge: &NodeBridgeClient,
    ) -> BridgeRequest {
        product_distribution_request(
            bridge,
            "product.build",
            context,
            vec![(
                "outputDir",
                Value::String(
                    string_option_value(context, "output-dir")
                        .unwrap_or_else(|| ".lania/build/product".into()),
                ),
            )],
        )
    }

    pub fn pack_product_request(
        context: &CommandContext,
        bridge: &NodeBridgeClient,
    ) -> BridgeRequest {
        product_distribution_request(
            bridge,
            "product.pack",
            context,
            vec![
                (
                    "buildDir",
                    Value::String(
                        string_option_value(context, "build-dir")
                            .unwrap_or_else(|| ".lania/build/product".into()),
                    ),
                ),
                (
                    "outputDir",
                    Value::String(
                        string_option_value(context, "output-dir")
                            .unwrap_or_else(|| ".lania/pack/product/install-root".into()),
                    ),
                ),
            ],
        )
    }

    pub fn publish_product_request(
        context: &CommandContext,
        bridge: &NodeBridgeClient,
    ) -> BridgeRequest {
        // publish 的 params 相对复杂：它既包含“产物目录参数”，也包含“执行策略参数”。
        // 注意：这里的 key 必须和 TS 侧 `product.publish` 处理器约定一致（例如 `npmBin`）。
        let mut extra = vec![
            (
                "packDir",
                Value::String(
                    string_option_value(context, "pack-dir")
                        .unwrap_or_else(|| ".lania/pack/product/install-root".into()),
                ),
            ),
            (
                "outputDir",
                Value::String(
                    string_option_value(context, "output-dir")
                        .unwrap_or_else(|| ".lania/publish/product/npm-package".into()),
                ),
            ),
        ];
        if let Some(dist_tag) = string_option_value(context, "dist-tag") {
            extra.push(("distTag", Value::String(dist_tag)));
        }
        if let Some(channel) = string_option_value(context, "channel") {
            extra.push(("channel", Value::String(channel)));
        }
        if let Some(registry) = string_option_value(context, "registry") {
            extra.push(("registry", Value::String(registry)));
        }
        if let Some(platform_binaries_dir) = string_option_value(context, "platform-binaries-dir") {
            extra.push(("platformBinariesDir", Value::String(platform_binaries_dir)));
        }
        if let Some(platform_binary_paths) = string_option_value(context, "platform-binary-paths") {
            extra.push(("platformBinaryPaths", Value::String(platform_binary_paths)));
        }
        if bool_option_value(context, "execute", false) {
            extra.push(("execute", Value::Bool(true)));
        }
        if bool_option_value(context, "dry-run", false) {
            extra.push(("dryRun", Value::Bool(true)));
        }
        if bool_option_value(context, "yes", false) {
            extra.push(("yes", Value::Bool(true)));
        }
        if bool_option_value(context, "resume", false) {
            extra.push(("resume", Value::Bool(true)));
        }
        if let Some(otp) = string_option_value(context, "otp") {
            extra.push(("otp", Value::String(otp)));
        }
        if let Some(npm_bin) = string_option_value(context, "npm-bin") {
            extra.push(("npmBin", Value::String(npm_bin)));
        }
        if let Some(max_retries) = string_option_value(context, "max-retries") {
            extra.push(("maxRetries", Value::String(max_retries)));
        }
        if let Some(retry_delay_ms) = string_option_value(context, "retry-delay-ms") {
            extra.push(("retryDelayMs", Value::String(retry_delay_ms)));
        }
        if bool_option_value(context, "rollback-on-failure", false) {
            extra.push(("rollbackOnFailure", Value::Bool(true)));
        }
        product_distribution_request(bridge, "product.publish", context, extra)
    }

    pub fn inspect_product_request(
        context: &CommandContext,
        bridge: &NodeBridgeClient,
    ) -> BridgeRequest {
        Self::inspect_like_product_request(context, bridge, false)
    }

    pub fn doctor_product_request(
        context: &CommandContext,
        bridge: &NodeBridgeClient,
    ) -> BridgeRequest {
        Self::inspect_like_product_request(context, bridge, true)
    }

    fn inspect_like_product_request(
        context: &CommandContext,
        bridge: &NodeBridgeClient,
        doctor: bool,
    ) -> BridgeRequest {
        let mut extra = vec![("hostVersion", Value::String(env!("CARGO_PKG_VERSION").to_string()))];
        if bool_option_value(context, "compat", false) {
            extra.push(("compat", Value::Bool(true)));
        }
        if doctor {
            extra.push(("doctor", Value::Bool(true)));
            extra.push(("compat", Value::Bool(true)));
        }
        product_distribution_request(bridge, "product.inspect", context, extra)
    }
}

fn string_option_value(context: &CommandContext, key: &str) -> Option<String> {
    context
        .argv
        .options
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn bool_option_value(context: &CommandContext, key: &str, default: bool) -> bool {
    context
        .argv
        .options
        .get(key)
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

fn product_distribution_request(
    bridge: &NodeBridgeClient,
    method: &str,
    context: &CommandContext,
    extra: Vec<(&str, Value)>,
) -> BridgeRequest {
    // 所有 product.* 方法都接受：
    // - cwd: 绝对/基于当前 cwd 的路径
    // - path: 原始参数（可空），用于调试/兼容
    // - clean: 清理开关（默认 true）
    let path = string_option_value(context, "path");
    let cwd = resolve_cwd_from_path_option(&context.cwd, path.as_deref());
    let mut params = Map::new();
    params.insert("cwd".into(), Value::String(cwd));
    params.insert("path".into(), serde_json::json!(path));
    params.insert(
        "clean".into(),
        Value::Bool(bool_option_value(context, "clean", true)),
    );
    for (key, value) in extra {
        params.insert(key.into(), value);
    }
    bridge.request(method, Value::Object(params))
}

fn resolve_cwd_from_path_option(base_cwd: &str, raw_path: Option<&str>) -> String {
    // cwd 解析规则保持和旧 CLI 一致：
    // - 传了 `--path` 时：相对路径基于 base_cwd 拼接；绝对路径直接使用
    // - 未传 `--path` 时：使用 base_cwd
    match raw_path.map(str::trim) {
        Some(raw) if !raw.is_empty() => {
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                path.display().to_string()
            } else {
                PathBuf::from(base_cwd).join(raw).display().to_string()
            }
        }
        _ => base_cwd.to_string(),
    }
}
