//! 可选独立 Hub 调试入口（`standalone-hub` feature，默认不构建）。
//!
//! 正式部署形态是进程内嵌：命令二进制静态链接 `lania-bridge-host`。
//! 本 bin 只用于观察 Hub 端点表/诊断，不参与任何命令链路。

use std::io::{self, BufRead};

use lania_bridge_host::BridgeHost;

fn main() -> io::Result<()> {
    let mut host = BridgeHost::new()?;
    let mut cli = host.attach("lan-hub-cli");
    cli.announce_str(&["hub.inspect"], &["event.heartbeat"]);
    host.pump_until_idle(100);

    eprintln!("[lan-hub] embedded asset root: {}", host.asset_root().display());
    eprintln!("[lan-hub] endpoints: {:?}", host.hub().endpoint_ids());
    eprintln!("[lan-hub] press Enter (or close stdin) to stop");

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    host.pump_until_idle(100);
    let diagnostics = host.diagnostics().clone();
    eprintln!(
        "[lan-hub] frames_received={} requests_routed={} responses_routed={} events_delivered={}",
        diagnostics.frames_received,
        diagnostics.requests_routed,
        diagnostics.responses_routed,
        diagnostics.events_delivered,
    );
    Ok(())
}
