use std::{fs, time::SystemTime};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::{
    ensure_write_root_allowed, payload_cwd, payload_required_str, resolve_path_from_cwd,
    HostPayload, HostRpcAdapter, HostRpcResponse, HostToolsPolicySnapshot,
};

/// 文件系统相关的 RPC（`host.fs.*`）。
///
/// 这里把“文本 IO 辅助 + writeRoot 强制约束”放在一起：
/// - 父文件保留共享的 policy/path 辅助函数
/// - 本模块聚焦每个 method 的 payload 解析与返回值序列化
pub(super) fn handle_fs_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
    tools_policy: &HostToolsPolicySnapshot,
) -> Result<HostRpcResponse> {
    match method {
        "host.fs.exists" => {
            let path = payload_required_str(payload, "path", method)?;
            Ok((json!({ "exists": adapter.fs.exists(&path) }), Vec::new()))
        }
        "host.fs.read" => {
            let path = payload_required_str(payload, "path", method)?;
            // 当前阶段：先保持为纯文本读取（read_to_string），
            // 后续如果要支持二进制/编码探测，再扩展协议字段。
            let content = fs::read_to_string(&path)
                .map_err(|error| anyhow!("failed to read {}: {error}", path))?;
            Ok((json!({ "content": content }), Vec::new()))
        }
        "host.fs.write" => {
            let cwd = payload_cwd(payload);
            let path = payload_required_str(payload, "path", method)?;
            ensure_write_root_allowed(tools_policy, cwd, &path, method)?;
            let resolved = resolve_path_from_cwd(cwd, &path);
            let content = payload_required_str(payload, "content", method)?;
            let append = payload
                .get("append")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mkdirp = payload
                .get("mkdirp")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if mkdirp {
                if let Some(parent) = resolved.parent() {
                    adapter.fs.ensure_dir(parent)?;
                }
            }
            if append {
                use std::io::Write;
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&resolved)
                    .map_err(|error| {
                        anyhow!("failed to open {} for append: {error}", resolved.display())
                    })?;
                file.write_all(content.as_bytes())
                    .map_err(|error| anyhow!("failed to append {}: {error}", resolved.display()))?;
            } else {
                fs::write(&resolved, content)
                    .map_err(|error| anyhow!("failed to write {}: {error}", resolved.display()))?;
            }
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.fs.mkdirp" => {
            let cwd = payload_cwd(payload);
            let path = payload_required_str(payload, "path", method)?;
            ensure_write_root_allowed(tools_policy, cwd, &path, method)?;
            adapter.fs.ensure_dir(resolve_path_from_cwd(cwd, &path))?;
            Ok((json!({ "ok": true }), Vec::new()))
        }
        "host.fs.remove" => {
            let cwd = payload_cwd(payload);
            let path = payload_required_str(payload, "path", method)?;
            ensure_write_root_allowed(tools_policy, cwd, &path, method)?;
            let resolved = resolve_path_from_cwd(cwd, &path);
            let recursive = payload
                .get("recursive")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !resolved.exists() {
                return Ok((json!({ "ok": true, "removed": false }), Vec::new()));
            }
            let metadata = fs::metadata(&resolved)
                .map_err(|error| anyhow!("failed to stat {}: {error}", resolved.display()))?;
            if metadata.is_dir() {
                if recursive {
                    fs::remove_dir_all(&resolved).map_err(|error| {
                        anyhow!("failed to remove dir {}: {error}", resolved.display())
                    })?;
                } else {
                    fs::remove_dir(&resolved).map_err(|error| {
                        anyhow!("failed to remove dir {}: {error}", resolved.display())
                    })?;
                }
            } else {
                fs::remove_file(&resolved).map_err(|error| {
                    anyhow!("failed to remove file {}: {error}", resolved.display())
                })?;
            }
            Ok((json!({ "ok": true, "removed": true }), Vec::new()))
        }
        "host.fs.readdir" => {
            let path = payload_required_str(payload, "path", method)?;
            let entries = fs::read_dir(&path)
                .map_err(|error| anyhow!("failed to read dir {}: {error}", path))?
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().to_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>();
            Ok((json!({ "entries": entries }), Vec::new()))
        }
        "host.fs.stat" => {
            let path = payload_required_str(payload, "path", method)?;
            let metadata =
                fs::metadata(&path).map_err(|error| anyhow!("failed to stat {}: {error}", path))?;
            let mtime_ms = metadata
                .modified()
                .ok()
                .and_then(|mtime| mtime.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64);
            Ok((
                json!({
                    "isFile": metadata.is_file(),
                    "isDir": metadata.is_dir(),
                    "size": metadata.len(),
                    "mtimeMs": mtime_ms,
                }),
                Vec::new(),
            ))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}
