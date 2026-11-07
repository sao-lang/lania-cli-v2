//! `runtime` 设施：项目级动态命令与脚手架的执行宿主（文档 §5.6 F.1）。
//!
//! 主要导出：RUNTIME_PACKAGE_NAME、parse_runtime_args、RuntimeAction。
//!
//! 设计要点：
//! - 它**不是一条 named 命令**，而是可扩展宿主设施：承载 `lan.config.*` 里声明的
//!   `runtimeCommands`（动态命令）与脚手架封装出来的命令；
//! - 动态命令的解析由 node-bridge 负责（`commands.resolveDynamic`），Rust 侧只做
//!   「解析结果 → 命令注册表」的接线，与总包 `lania-cli` 走同一条路径；
//! - 因此这里的输出/退出码与总包跑动态命令**逐字节一致**（同一份 node-bridge + host-core）。

use std::collections::BTreeMap;

/// npm 包名：core 的 Phase 2 动态回退据此判断设施是否已安装。
pub const RUNTIME_PACKAGE_NAME: &str = "@lania-cli-v2/runtime";

/// `lan-runtime` 支持的三种动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeAction {
    /// 列出当前项目的动态命令（JSON 输出，供 launcher 合并 help）。
    List,
    /// 判定某个命令名是否为项目动态命令（JSON 输出，供 launcher 决定路由）。
    Resolve(String),
    /// 直接执行一条动态命令。
    Execute(Vec<String>),
}

/// 解析 `lan-runtime` 的 argv。
///
/// 约定：
/// - `--list` → 列出动态命令；
/// - `--resolve <cmd>` → 判定命令；
/// - 其余 → 执行 `<cmd> [args...]`。
pub fn parse_runtime_args(args: &[String]) -> RuntimeAction {
    match args.first().map(String::as_str) {
        Some("--list") => RuntimeAction::List,
        Some("--resolve") => RuntimeAction::Resolve(
            args.get(1).cloned().unwrap_or_default(),
        ),
        _ => RuntimeAction::Execute(args.to_vec()),
    }
}

/// 动态命令的解析结果（JSON 契约，供 launcher 消费）。
///
/// 字段与 launcher 的期望保持一致：
/// - `found`：是否命中项目动态命令；
/// - `command`：命令名；
/// - `mount`：挂载点（来自 `runtimeCommands[].mount`）；
/// - `handler`：handler 描述（plugin + method 或 async fn 形态）。
pub fn resolve_payload(
    command: &str,
    found: bool,
    handler_id: Option<&str>,
) -> serde_json::Value {
    let mut map = BTreeMap::new();
    map.insert("found".to_string(), serde_json::json!(found));
    map.insert("command".to_string(), serde_json::json!(command));
    map.insert(
        "handlerId".to_string(),
        match handler_id {
            Some(value) => serde_json::json!(value),
            None => serde_json::Value::Null,
        },
    );
    serde_json::Value::Object(map.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parses_list_action() {
        assert_eq!(parse_runtime_args(&args(&["--list"])), RuntimeAction::List);
    }

    #[test]
    fn parses_resolve_action() {
        assert_eq!(
            parse_runtime_args(&args(&["--resolve", "ops"])),
            RuntimeAction::Resolve("ops".to_string()),
        );
    }

    #[test]
    fn defaults_to_execute_action() {
        assert_eq!(
            parse_runtime_args(&args(&["ops", "ping"])),
            RuntimeAction::Execute(args(&["ops", "ping"])),
        );
    }

    #[test]
    fn resolve_payload_reports_miss() {
        let payload = resolve_payload("ops", false, None);
        assert_eq!(payload["found"], serde_json::json!(false));
    }

    #[test]
    fn resolve_payload_reports_hit() {
        let payload = resolve_payload("ops", true, Some("command.ops"));
        assert_eq!(payload["found"], serde_json::json!(true));
        assert_eq!(payload["handlerId"], serde_json::json!("command.ops"));
    }
}
