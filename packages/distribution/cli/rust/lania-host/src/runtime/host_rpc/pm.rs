use std::path::Path;

use anyhow::{anyhow, Result};
use lania_pm::PackageManager;
use serde_json::{json, Value};

use super::{
    payload_cwd, payload_optional_str, payload_required_str, payload_strings, HostPayload,
    HostRpcAdapter, HostRpcResponse,
};

/// 包管理器相关的 RPC（`host.pm.*`）保持为“读取/计划（read/plan）导向”。
///
/// 本模块负责：
/// - 根据 cwd / language / 显式 manager 选择包管理器
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
            let manager = resolve_manager(adapter, payload)?;
            Ok((
                json!({
                    "manager": manager.id(),
                    "language": manager.language(),
                }),
                Vec::new(),
            ))
        }
        "host.pm.spec" => {
            let manager = resolve_manager(adapter, payload)?;
            Ok((
                serde_json::to_value(adapter.package_manager.spec(manager))?,
                Vec::new(),
            ))
        }
        "host.pm.supportedManagers" => {
            let managers = adapter
                .package_manager
                .supported_managers()
                .into_iter()
                .map(|manager| manager.id().to_string())
                .collect::<Vec<_>>();
            Ok((json!({ "managers": managers }), Vec::new()))
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
        "host.pm.command.init" => {
            let manager = resolve_manager(adapter, payload)?;
            Ok((
                serde_json::to_value(adapter.package_manager.init_command(manager)?)?,
                Vec::new(),
            ))
        }
        "host.pm.command.installAll" => {
            let manager = resolve_manager(adapter, payload)?;
            Ok((
                serde_json::to_value(adapter.package_manager.install_all_command(manager)?)?,
                Vec::new(),
            ))
        }
        "host.pm.command.install" => {
            let manager = resolve_manager(adapter, payload)?;
            let packages = payload_strings(payload, "packages");
            let dev = payload.get("dev").and_then(Value::as_bool).unwrap_or(false);
            Ok((
                serde_json::to_value(
                    adapter
                        .package_manager
                        .install_command(manager, &packages, dev)?,
                )?,
                Vec::new(),
            ))
        }
        "host.pm.command.remove" => {
            let manager = resolve_manager(adapter, payload)?;
            let packages = payload_strings(payload, "packages");
            Ok((
                serde_json::to_value(adapter.package_manager.remove_command(manager, &packages)?)?,
                Vec::new(),
            ))
        }
        "host.pm.command.update" => {
            let manager = resolve_manager(adapter, payload)?;
            let packages = payload_strings(payload, "packages");
            Ok((
                serde_json::to_value(adapter.package_manager.update_command(manager, &packages)?)?,
                Vec::new(),
            ))
        }
        "host.pm.command.publish" => {
            let manager = resolve_manager(adapter, payload)?;
            let tag = payload_optional_str(payload, "tag");
            Ok((
                serde_json::to_value(
                    adapter
                        .package_manager
                        .publish_command(manager, tag.as_deref())?,
                )?,
                Vec::new(),
            ))
        }
        "host.pm.command.addDependencyCommands" => {
            let manager = resolve_manager(adapter, payload)?;
            let dependencies = payload_strings(payload, "dependencies");
            let dev_dependencies = payload_strings(payload, "devDependencies");
            let commands = adapter.package_manager.add_dependency_commands(
                manager,
                &dependencies,
                &dev_dependencies,
            )?;
            Ok((json!({ "commands": commands }), Vec::new()))
        }
        "host.pm.command.runScript" => {
            let manager = resolve_manager(adapter, payload)?;
            let cwd = payload_cwd(payload);
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
                    .run_script_command(manager, &script, &extra_args)?
            };
            Ok((serde_json::to_value(command)?, Vec::new()))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}

fn resolve_manager(adapter: &HostRpcAdapter, payload: &HostPayload) -> Result<PackageManager> {
    if let Some(raw) = payload_optional_str(payload, "manager") {
        return PackageManager::parse(&raw).ok_or_else(|| {
            anyhow!(
                "unknown package manager: {} (supported: {})",
                raw,
                supported_manager_ids()
            )
        });
    }

    let cwd = payload_cwd(payload);
    if let Some(language) = payload_optional_str(payload, "language") {
        return adapter
            .package_manager
            .detect_for_language(cwd, &language)
            .ok_or_else(|| {
                anyhow!("unsupported language for package manager detection: {language}")
            });
    }

    Ok(adapter.package_manager.detect_from_cwd(Path::new(cwd)))
}

fn supported_manager_ids() -> String {
    PackageManager::all()
        .iter()
        .map(|manager| manager.id())
        .collect::<Vec<_>>()
        .join(", ")
}
