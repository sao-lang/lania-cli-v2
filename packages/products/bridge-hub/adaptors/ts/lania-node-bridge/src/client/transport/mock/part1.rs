//! mock transport 的第一部分：实现一批最基础/最常用的 bridge method。
//!
//! 这些 mock handler 的目标是“结构稳定”，不是“行为完全一致”：
//! - 便于在缺少 Node bridge 进程时仍能跑通 CLI 主链路
//! - 便于开发期演示/调试输出格式
//!
//! 约定：不认识的方法会把 request 原样返回给 part2 继续匹配。

use crate::client::NodeBridgeClient;
use crate::protocol::{BridgeExchange, BridgeRequest};

use crate::client::process::{
    mock_build_tool, mock_lan_config_contents, mock_lan_config_value, mock_lint_tools,
};
use crate::protocol::{BridgeEvent, BridgeEventMethod, BridgeResponse};

pub(super) fn handle_part1(
    client: &NodeBridgeClient,
    request: BridgeRequest,
) -> std::result::Result<BridgeExchange, BridgeRequest> {
    let exchange = match request.method.as_str() {
        "bridge.ping" => BridgeExchange {
            response: BridgeResponse {
                id: request.id,
                result: Some(serde_json::json!({
                    "ok": true,
                    "bridgeName": "@lania-cli/node-bridge",
                })),
                error: None,
            },
            events: vec![],
        },
        "bridge.metrics" => BridgeExchange {
            response: BridgeResponse {
                id: request.id,
                result: Some(serde_json::json!({
                    "requests": client.metrics_snapshot().requests_sent,
                    "events": client.metrics_snapshot().events_received,
                    "plugins": ["config", "compiler", "lint", "system", "template", "commitizen", "commitlint"],
                    "rejectedPlugins": [],
                })),
                error: None,
            },
            events: vec![],
        },
        "bridge.subscribe" => BridgeExchange {
            response: BridgeResponse {
                id: request.id,
                result: Some(serde_json::json!({
                    "accepted": true,
                    "mode": "request_response_stream",
                    "events": client.supported_events(),
                })),
                error: None,
            },
            events: vec![],
        },
        "bridge.heartbeat" => BridgeExchange {
            response: BridgeResponse {
                id: request.id,
                result: Some(serde_json::json!({ "ok": true })),
                error: None,
            },
            events: vec![BridgeEvent {
                method: BridgeEventMethod::Heartbeat,
                params: serde_json::json!({ "ts": 0 }),
            }],
        },
        "config.loadLan" => {
            let cwd = request.params["cwd"].as_str().unwrap_or_default();
            let config = mock_lan_config_value(cwd);
            let exists = mock_lan_config_contents(cwd).is_some();
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "cwd": request.params["cwd"],
                        "configPath": exists.then_some("lan.config.js"),
                        "exists": exists,
                        "supportedExtensions": [".js", ".cjs", ".ts"],
                        "config": config,
                        "buildTool": config["buildTool"],
                        "buildAdaptors": config["buildAdaptors"],
                        "lintAdaptors": config["lintAdaptors"],
                        "lintTools": config["lintTools"],
                        "plugins": config["plugins"],
                    })),
                    error: None,
                },
                events: vec![BridgeEvent {
                    method: BridgeEventMethod::Log,
                    params: serde_json::json!({
                        "level": "info",
                        "message": "Loaded lan config snapshot",
                    }),
                }],
            }
        }
        "config.loadTool" => BridgeExchange {
            response: BridgeResponse {
                id: request.id,
                result: Some(serde_json::json!({
                    "cwd": request.params["cwd"],
                    "tool": request.params["tool"],
                    "resolved": true,
                })),
                error: None,
            },
            events: vec![],
        },
        "compiler.dev" => {
            let port = request.params["port"].as_u64().unwrap_or(8089);
            let cwd = request.params["cwd"].as_str().unwrap_or_default();
            let tool = mock_build_tool(cwd);
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "accepted": true,
                        "tool": tool,
                        "mode": "development",
                        "port": port,
                        "longRunning": true,
                    })),
                    error: None,
                },
                events: vec![
                    BridgeEvent {
                        method: BridgeEventMethod::Log,
                        params: serde_json::json!({
                            "level": "info",
                            "message": format!("Starting {tool} dev server"),
                        }),
                    },
                    BridgeEvent {
                        method: BridgeEventMethod::DevUrl,
                        params: serde_json::json!({
                            "url": format!("http://127.0.0.1:{port}"),
                        }),
                    },
                ],
            }
        }
        "compiler.build" => {
            let watch = request.params["watch"].as_bool().unwrap_or(false);
            let cwd = request.params["cwd"].as_str().unwrap_or_default();
            let tool = mock_build_tool(cwd);
            let mut events = vec![
                BridgeEvent {
                    method: BridgeEventMethod::Progress,
                    params: serde_json::json!({
                        "current": 1,
                        "total": 2,
                        "message": format!("Resolving {tool} build graph"),
                    }),
                },
                BridgeEvent {
                    method: BridgeEventMethod::BuildAsset,
                    params: serde_json::json!({
                        "file": "dist/index.js",
                        "bytes": 2048,
                    }),
                },
            ];
            if watch {
                events.push(BridgeEvent {
                    method: BridgeEventMethod::WatchChange,
                    params: serde_json::json!({
                        "path": "src/main.ts",
                    }),
                });
            }
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "accepted": true,
                        "tool": tool,
                        "watch": watch,
                        "mode": request.params["mode"],
                        "outputDir": request.params["outputDir"],
                        "longRunning": watch,
                    })),
                    error: None,
                },
                events,
            }
        }
        "compiler.stop" | "bridge.shutdown" => BridgeExchange {
            response: BridgeResponse {
                id: request.id,
                result: Some(serde_json::json!({
                    "accepted": true,
                    "stopped": true,
                })),
                error: None,
            },
            events: vec![BridgeEvent {
                method: BridgeEventMethod::Shutdown,
                params: serde_json::json!({
                    "reason": "requested",
                }),
            }],
        },
        "lint.run" => {
            let concurrency = request.params["concurrency"].as_u64().unwrap_or(4);
            let fix = request.params["fix"].as_bool().unwrap_or(false);
            let mode = request.params["mode"]
                .as_str()
                .unwrap_or(if fix { "fix" } else { "check" });
            let cwd = request.params["cwd"].as_str().unwrap_or_default();
            let linters = mock_lint_tools(cwd);
            let warnings = if fix { 0 } else { linters.len() as u64 };
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "accepted": true,
                        "mode": mode,
                        "fix": fix,
                        "concurrency": concurrency,
                        "summary": {
                            "errors": 0,
                            "warnings": warnings,
                            "files": 0,
                            "adaptors": linters.len(),
                        },
                        "summaryByAdaptor": linters.iter().map(|adaptor| (
                            adaptor.clone(),
                            serde_json::json!({
                                "errors": 0,
                                "warnings": if fix { 0 } else { 1 },
                                "files": 0,
                                "implementation": "fallback",
                            })
                        )).collect::<serde_json::Map<String, serde_json::Value>>(),
                        "summaryText": format!(
                            "lint {mode}: 0 error(s), {warnings} warning(s), 0 file(s), adaptors={}",
                            linters.join(", ")
                        ),
                        "resultsByAdaptor": linters.iter().map(|adaptor| (
                            adaptor.clone(),
                            serde_json::json!({
                                "adaptor": adaptor,
                                "errors": 0,
                                "warnings": if fix { 0 } else { 1 },
                                "implementation": "fallback",
                            })
                        )).collect::<serde_json::Map<String, serde_json::Value>>(),
                        "exitCode": 0,
                    })),
                    error: None,
                },
                events: std::iter::once(BridgeEvent {
                    method: BridgeEventMethod::Log,
                    params: serde_json::json!({
                        "level": "info",
                        "message": "Running lint adaptors",
                    }),
                })
                .chain(std::iter::once(BridgeEvent {
                    method: BridgeEventMethod::LintStart,
                    params: serde_json::json!({
                        "cwd": cwd,
                        "fix": fix,
                        "concurrency": concurrency,
                        "adaptors": linters,
                    }),
                }))
                .chain(linters.iter().map(|adaptor| BridgeEvent {
                    method: BridgeEventMethod::LintFile,
                    params: serde_json::json!({
                        "adaptor": adaptor,
                        "filePath": ".",
                        "errors": 0,
                        "warnings": if fix { 0 } else { 1 },
                        "implementation": "fallback",
                    }),
                }))
                .chain(linters.iter().map(|adaptor| BridgeEvent {
                    method: BridgeEventMethod::LintResult,
                    params: serde_json::json!({
                        "adaptor": adaptor,
                        "errors": 0,
                        "warnings": if fix { 0 } else { 1 },
                    }),
                }))
                .chain(std::iter::once(BridgeEvent {
                    method: BridgeEventMethod::LintSummary,
                    params: serde_json::json!({
                        "errors": 0,
                        "warnings": warnings,
                        "adaptors": linters,
                        "exitCode": 0,
                    }),
                }))
                .collect(),
            }
        }
        "system.listCommands" => {
            let limit = request.params["limit"].as_u64().unwrap_or(3) as usize;
            let filter = request.params["filter"]
                .as_str()
                .map(|value| value.to_ascii_lowercase());
            let all_matches = request.params["allMatches"].as_bool().unwrap_or(false);
            let include_shell = request.params["includeShell"].as_bool().unwrap_or(true);
            let mut commands = vec![
                serde_json::json!({
                    "name": "node",
                    "path": "/usr/local/bin/node",
                    "directory": "/usr/local/bin",
                    "source": "PATH",
                    "kind": "symlink",
                }),
                serde_json::json!({
                    "name": "npm",
                    "path": "/usr/local/bin/npm",
                    "directory": "/usr/local/bin",
                    "source": "PATH",
                    "kind": "symlink",
                }),
                serde_json::json!({
                    "name": "tsc",
                    "path": "/usr/local/bin/tsc",
                    "directory": "/usr/local/bin",
                    "source": "PATH",
                    "kind": "symlink",
                }),
            ];
            if include_shell {
                commands.extend([
                    serde_json::json!({
                        "name": "cd",
                        "source": "shell_builtin",
                        "kind": "builtin",
                    }),
                    serde_json::json!({
                        "name": "gst",
                        "source": "shell_alias",
                        "kind": "alias",
                        "detail": "git status",
                    }),
                    serde_json::json!({
                        "name": "mkcd",
                        "source": "shell_function",
                        "kind": "function",
                    }),
                ]);
            }
            if all_matches {
                commands.push(serde_json::json!({
                    "name": "tsc",
                    "path": "/opt/homebrew/bin/tsc",
                    "directory": "/opt/homebrew/bin",
                    "source": "PATH",
                    "kind": "symlink",
                }));
            }
            if let Some(filter) = filter {
                commands.retain(|command| {
                    command["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(&filter)
                });
            }
            commands.truncate(limit.max(1));
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "accepted": true,
                        "kind": "system_commands",
                        "scope": "environment",
                        "platform": std::env::consts::OS,
                        "shell": "/bin/zsh",
                        "shellName": "zsh",
                        "shellSupported": include_shell,
                        "includeShell": include_shell,
                        "cwd": request.params["cwd"],
                        "filter": request.params["filter"],
                        "limit": request.params["limit"],
                        "allMatches": all_matches,
                        "notes": if include_shell {
                            serde_json::json!(["Includes PATH executables plus shell builtins, aliases, and functions from the current shell."])
                        } else {
                            serde_json::json!(["Only PATH-resolved executables are included because shell command discovery is disabled."])
                        },
                        "summary": {
                            "pathEntries": 3,
                            "scannedDirs": 3,
                            "unique": 3,
                            "duplicates": if all_matches { 1 } else { 0 },
                            "shellBuiltins": if include_shell { 1 } else { 0 },
                            "shellAliases": if include_shell { 1 } else { 0 },
                            "shellFunctions": if include_shell { 1 } else { 0 },
                            "matched": commands.len(),
                            "returned": commands.len(),
                        },
                        "commands": commands,
                        "duplicates": if all_matches {
                            serde_json::json!([
                                {
                                    "name": "tsc",
                                    "paths": ["/usr/local/bin/tsc", "/opt/homebrew/bin/tsc"]
                                }
                            ])
                        } else {
                            serde_json::json!([])
                        }
                    })),
                    error: None,
                },
                events: vec![BridgeEvent {
                    method: BridgeEventMethod::Log,
                    params: serde_json::json!({
                        "level": "info",
                        "message": format!("Scanned PATH and matched {} terminal command(s)", commands.len()),
                    }),
                }],
            }
        }
        _ => return Err(request),
    };
    Ok(exchange)
}
