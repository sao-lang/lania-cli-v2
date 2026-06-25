//! `command-dev` 的桥接请求（BridgeRequest）构造。
//!
//! 这个文件负责把 CLI 的 argv/options 映射为 node-bridge 能理解的 `method + params`。
//! 重点兼容点：
//! - `--path` 的 cwd 解析规则需要保持老 CLI 行为（相对路径基于当前 cwd 拼接）。
//! - `config/path` 等字段虽然可能不再是必需，但为了兼容仍会透传到 params。
//! - 其它 dev 选项（host/hmr/open/mode/port）仅做轻量转发，不在 Rust 侧做业务判断。

use lania_command::CommandContext;
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};
use std::path::Path;

use crate::DevCommandPlugin;

// 标准 `lan dev` 的 CLI -> bridge request 映射。
//
// 这里把解析后的 argv/options 转成 `BridgeRequest`：
// - 把 `--path` 归一化成 cwd（保持旧行为：相对路径基于当前 cwd 拼接）
// - 透传可选参数（host/hmr/open/mode）
// - 保留 `config/path` 字段用于兼容（即使新 bridge 可能不再依赖它们）

impl DevCommandPlugin {
    pub fn build_request(context: &CommandContext, bridge: &NodeBridgeClient) -> BridgeRequest {
        // 兼容字段：`config` / `path` 属于历史遗留参数。
        // 即使 bridge 侧的某些 workflow 已经不需要，也保留透传，避免老调用点/老 bridge 行为变化。
        let config_path = context
            .argv
            .options
            .get("config")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let path = context
            .argv
            .options
            .get("path")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);

        // cwd 解析规则（保持旧 CLI 语义）：
        // - `--path` 是绝对路径：直接作为 cwd
        // - `--path` 是相对路径：基于当前命令 cwd 拼接
        // - 未提供 `--path`：使用当前命令 cwd
        let cwd = match &path {
            Some(raw) if !raw.trim().is_empty() => {
                let p = Path::new(raw.trim());
                if p.is_absolute() {
                    raw.trim().to_string()
                } else {
                    Path::new(&context.cwd)
                        .join(raw.trim())
                        .display()
                        .to_string()
                }
            }
            _ => context.cwd.clone(),
        };
        let port = context
            .argv
            .options
            .get("port")
            .and_then(|value| value.as_u64())
            .and_then(|value| u16::try_from(value).ok());
        let host = context
            .argv
            .options
            .get("host")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let hmr = context
            .argv
            .options
            .get("hmr")
            .and_then(|value| value.as_bool());
        let open = context
            .argv
            .options
            .get("open")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let mode = context
            .argv
            .options
            .get("mode")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let mut request = bridge.compiler_dev_request(cwd, port);
        request.params["host"] = serde_json::json!(host);
        request.params["hmr"] = serde_json::json!(hmr);
        request.params["open"] = serde_json::json!(open);
        request.params["mode"] = serde_json::json!(mode);
        request.params["config"] = serde_json::json!(config_path);
        request.params["path"] = serde_json::json!(path);
        request
    }
}
