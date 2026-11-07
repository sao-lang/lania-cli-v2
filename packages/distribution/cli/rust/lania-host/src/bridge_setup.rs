//! 命令二进制的桥接装配统一入口（消除 11 份重复，§5.5 E.1）。
//!
//! 放在 `lania-host` 而不是 `lania-bridge-host`：`lania-bridge-host` 已被宿主依赖，
//! 若它再反向依赖 `HostRuntime` 会形成依赖环；而 `lania-host` 已经同时依赖
//! `lania-bridge-host`（rust_bridge / load_bridge_provider）与 `lania-host-core`
//! （BridgeLaunchSpec / BridgeProvider），是唯一无环的收口位置。
//!
//! 所有命令二进制（`lan-*` 与总包 `lan`）改用这两个入口，避免 B11 短路块与
//! 语言桥登记逻辑在每个 bin 里各写一遍。

use std::env;
use std::path::Path;

use anyhow::Result;
use lania_host_core::BridgeLaunchSpec;

use crate::runtime::HostRuntime;

/// B11：把「判定 bridge 调用 → 跑 stdio 协议循环 → 以 exit code 结束」收口到一处。
///
/// 正常命令返回 `Ok(false)`（未短路，调用方继续装配宿主）；当 argv 命中
/// `__lania_rs_bridge` 时，stdio 循环跑完后由本函数按协议返回的 exit code
/// `std::process::exit`，与原先每个 bin 里的行为一致。
/// 调用方保持 `if lania_host::maybe_run_rust_bridge()? { return Ok(()); }` 即可。
pub fn maybe_run_rust_bridge() -> Result<bool> {
    if !lania_bridge_host::rust_bridge::is_bridge_invocation(env::args_os()) {
        return Ok(false);
    }
    let code = lania_bridge_host::rust_bridge::run_stdio_bridge()?;
    std::process::exit(code);
}

/// 统一装配命令二进制的桥接层：
/// 1. best-effort 登记各语言 1:1 桥（不启动进程，保留既有回落链路）；
/// 2. `load_bridge_provider()` 装配 Hub provider，并 `register_language` 让 Hub
///    不再是空的（声明 capability 后 `call_sync` 可路由）；
/// 3. `host.attach_bridge(provider)` 注入 `bridge_slot`，使其不再是 `None`。
///
/// `include_rust_bridge`：总包入口历史上还登记 `lan-rs-bridge`，命令子包历史上
/// 不登记；保留这处差异，避免改变 Rust 项目 `lint` / `build` 的回落行为。
///
/// 任何一步失败都降级为「纯 1:1 回落」：语言桥是「装了更好、不装也能用」，
/// Hub provider 注入失败也不应阻断命令本身（保证 `smoke-pi` 等行为不退化）。
pub async fn configure_command_bridges(
    host: &mut HostRuntime,
    cwd: &Path,
    include_rust_bridge: bool,
) {
    // 1) 1:1 兼容层：保持原有 best-effort 登记（懒加载，不启动子进程）。
    let _ = host.register_python_bridge(cwd);
    let _ = host.register_go_bridge(cwd);
    let _ = host.register_dart_bridge(cwd);
    if include_rust_bridge {
        let _ = host.register_rust_bridge(cwd);
    }

    // 2)+3) 新总线 provider：装配 → 声明语言能力 → 注入 bridge_slot。
    if let Ok(provider) = lania_bridge_host::load_bridge_provider() {
        for id in command_language_ids(include_rust_bridge) {
            let _ = provider.register_language(
                id,
                BridgeLaunchSpec {
                    program: (*id).to_string(),
                    args: Vec::new(),
                    dir: None,
                },
            );
        }
        host.attach_bridge(provider).await;
    }
}

/// 与 `configure_command_bridges` 登记顺序一致的语言 id（opaque 字符串）。
fn command_language_ids(include_rust_bridge: bool) -> &'static [&'static str] {
    if include_rust_bridge {
        &["python", "go", "dart", "rust"]
    } else {
        &["python", "go", "dart"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn configuring_attaches_bridge_slot_and_keeps_1to1_fallback() {
        let cwd = std::env::temp_dir();
        let mut host = HostRuntime::new();
        assert!(host.bridge_provider().is_none(), "默认未注入");

        configure_command_bridges(&mut host, &cwd, false).await;

        assert!(
            host.bridge_provider().is_some(),
            "命令 bin 配置后 bridge_slot 不再是 None"
        );
        // 1:1 回落链路仍保留：go / dart 为延迟登记，登记本身不启动进程。
        assert!(host.has_runtime_bridge(crate::RuntimeKind::Go));
        assert!(host.has_runtime_bridge(crate::RuntimeKind::Dart));
    }

    #[tokio::test]
    async fn include_rust_bridge_flag_matches_total_package_behavior() {
        let cwd = std::env::temp_dir();
        let mut host = HostRuntime::new();
        configure_command_bridges(&mut host, &cwd, true).await;
        assert!(host.bridge_provider().is_some());
        // 总包历史上额外登记 lan-rs-bridge；命令子包不登记，保持回落差异。
        assert!(host.has_runtime_bridge(crate::RuntimeKind::Rust));
    }
}
