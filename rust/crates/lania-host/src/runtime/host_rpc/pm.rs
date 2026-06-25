use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::{
    payload_cwd, payload_required_str, payload_strings, HostPayload, HostRpcAdapter,
    HostRpcResponse,
};

/// 包管理器相关的 RPC（`host.pm.*`）保持为“读取/计划（read/plan）导向”。
///
/// 本模块负责：
/// - 根据 cwd 探测包管理器（npm/pnpm/yarn/...）
/// - 把 payload 映射为“可执行命令计划”（command plan）
///
/// 注意：
/// - 这里不直接执行命令；真正执行仍然走 `host.exec.*`，以集中策略校验与审计。
pub(super) fn handle_pm_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<HostRpcResponse> {
    match method {
        "host.pm.detect" => {
            let manager = adapter
                .package_manager
                .detect_from_cwd(Path::new(payload_cwd(payload)));
            Ok((json!({ "manager": manager.binary() }), Vec::new()))
        }
        "host.pm.spec" => {
            let manager = adapter
                .package_manager
                .detect_from_cwd(Path::new(payload_cwd(payload)));
            Ok((
                serde_json::to_value(adapter.package_manager.spec(manager))?,
                Vec::new(),
            ))
        }
        "host.pm.supportedManagers" => {
            let supported = adapter
                .package_manager
                .supported_managers()
                .into_iter()
                .map(|manager| manager.binary().to_string())
                .collect::<Vec<_>>();
            Ok((json!({ "managers": supported }), Vec::new()))
        }
        "host.pm.loadPackageJsonSnapshot" => Ok((
            serde_json::to_value(
                adapter
                    .package_manager
                    .load_package_json_snapshot(payload_cwd(payload))?,
            )?,
            Vec::new(),
        )),
        "host.pm.scriptExists" => {
            let script = payload_required_str(payload, "script", method)?;
            Ok((
                json!({
                    "exists": adapter.package_manager.script_exists(payload_cwd(payload), &script)?
                }),
                Vec::new(),
            ))
        }
        "host.pm.command.install" => {
            let manager = adapter
                .package_manager
                .detect_from_cwd(Path::new(payload_cwd(payload)));
            let packages = payload_strings(payload, "packages");
            let dev = payload.get("dev").and_then(Value::as_bool).unwrap_or(false);
            Ok((
                serde_json::to_value(
                    adapter
                        .package_manager
                        .install_command(manager, &packages, dev),
                )?,
                Vec::new(),
            ))
        }
        "host.pm.command.runScript" => {
            let cwd = payload_cwd(payload);
            let manager = adapter.package_manager.detect_from_cwd(Path::new(cwd));
            let script = payload_required_str(payload, "script", method)?;
            let extra_args = payload_strings(payload, "args");
            let checked = payload
                .get("checked")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let command = if checked {
                adapter.package_manager.run_script_command_checked(
                    cwd,
                    manager,
                    &script,
                    &extra_args,
                )?
            } else {
                adapter
                    .package_manager
                    .run_script_command(manager, &script, &extra_args)
            };
            Ok((serde_json::to_value(command)?, Vec::new()))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}
