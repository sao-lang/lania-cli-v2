//! `command-workspace` 的 dev 部分（原 `packages/commands/dev` 迁入）。
//!
//! 内容：
//! - `DevCommandHandler`：标准 `lan workspace dev`（走 node-bridge 的 compiler.dev）
//! - `ProductDevCommandHandler`：`lan product dev`（re-exec 转发 + watch）
//! - `build_request`：标准 dev 的 bridge request 构建（原 request.rs）
//! - `resolve_dev_exit_code`：bridge 结果 -> 退出码映射

use anyhow::Result;
use async_trait::async_trait;
use lania_command::CommandContext;
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
};
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};
use std::path::Path;


// handler 只负责：
// - 校验需要的 capability（例如 NodeBridge）
// - 组装 bridge request / product-dev options
// - 执行并把结果包装成 `CommandExecution`

pub(crate) struct DevCommandHandler;

/// 非 web runtime 的 adapter 会在 result 里带 `success` 字段。
/// 缺失该字段视为成功（保持 web 既有行为）。
pub(crate) fn resolve_dev_exit_code(result: Option<&serde_json::Value>) -> i32 {
    let success = result
        .and_then(|value| value.get("success"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if success {
        lania_host::EXIT_SUCCESS
    } else {
        lania_host::EXIT_RUNTIME_ERROR
    }
}

#[async_trait(?Send)]
impl CommandHandler for DevCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // 标准 `lan workspace dev`：走 node-bridge 的 compiler.dev workflow。
        ctx.require_capability(CapabilityName::NodeBridge)?;
        // §5.1 验收：双运行时项目要告诉用户我们选了哪个。
        let cli_runtime = ctx
            .command()
            .argv
            .options
            .get("runtime")
            .and_then(|value| value.as_str());
        ctx.log_runtime_decision(cli_runtime);
        // 应用日志默认开启（否则用户看不到服务端接口日志），`--quiet-app` 才关掉。
        ctx.set_quiet_app_logs(ctx.bool_option("quiet-app"));
        let request = build_request(ctx.command(), ctx.node_bridge());
        let run = ctx.call_bridge(request).await?;
        let exit_code = resolve_dev_exit_code(run.exchange.response.result.as_ref());
        Ok(ctx.complete_bridge(run, exit_code))
    }
}

// 标准 `lan workspace dev` 的 CLI -> bridge request 映射。
//
// 这里把解析后的 argv/options 转成 `BridgeRequest`：
// - 把 `--path` 归一化成 cwd（保持旧行为：相对路径基于当前 cwd 拼接）
// - 透传可选参数（host/hmr/open/mode）
// - 保留 `config/path` 字段用于兼容（即使新 bridge 可能不再依赖它们）

pub(crate) fn build_request(
    context: &CommandContext,
    bridge: &NodeBridgeClient,
) -> BridgeRequest {
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
    let runtime = context
        .argv
        .options
        .get("runtime")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    request.params["config"] = serde_json::json!(config_path);
    request.params["path"] = serde_json::json!(path);
    request.params["runtime"] = serde_json::json!(runtime);
    request
}

#[cfg(test)]
mod tests {
    use super::{build_request, resolve_dev_exit_code};
    use lania_command::CommandContext;
    use lania_node_bridge::{BridgeClientConfig, NodeBridgeClient};
    use serde_json::json;

    fn joined_cwd(path: &str) -> String {
        std::path::Path::new("/repo")
            .join(path)
            .display()
            .to_string()
    }

    #[test]
    fn builds_dev_bridge_request_from_context() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("port".into(), json!(4000)),
                    ("path".into(), json!("apps/demo")),
                    ("hmr".into(), json!(false)),
                    ("mode".into(), json!("development")),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.dev".into(),
            trace_id: "trace-1".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = build_request(&context, &bridge);

        assert_eq!(request.method, "compiler.dev");
        assert_eq!(request.params["cwd"], joined_cwd("apps/demo"));
        assert_eq!(request.params["port"], 4000);
        assert_eq!(request.params["hmr"], false);
        assert_eq!(request.params["mode"], "development");
    }

    #[test]
    fn dev_request_forwards_runtime() {
        let context = CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("runtime".into(), json!("go"))].into_iter().collect(),
            },
            handler_id: "command.dev".into(),
            trace_id: "trace-runtime".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = build_request(&context, &bridge);

        assert_eq!(request.params["runtime"], json!("go"));
    }

    #[test]
    fn dev_exit_code_maps_failure_to_runtime_error() {
        use lania_host::{EXIT_RUNTIME_ERROR, EXIT_SUCCESS};

        assert_eq!(
            resolve_dev_exit_code(Some(&json!({ "success": false }))),
            EXIT_RUNTIME_ERROR
        );
        assert_eq!(
            resolve_dev_exit_code(Some(&json!({ "success": true }))),
            EXIT_SUCCESS
        );
        assert_eq!(resolve_dev_exit_code(Some(&json!({}))), EXIT_SUCCESS);
        assert_eq!(resolve_dev_exit_code(None), EXIT_SUCCESS);
    }
}
