//! 命令实现：全部语义都落在 `lania-runtime-manager` 的 Store / Provider 上，
//! 这里只负责「参数校验 + 输出形态」。

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use lania_runtime_manager::{
    describe_provider, provider_for, supported_language_ids, DiagnosticLevel, EnvService,
    InstallEvent, Language, Platform, RuntimeHome, RuntimeStore, ShimHost,
};
use serde_json::{json, Value};

use crate::cli::{Cli, Command};

pub fn run(cli: Cli) -> Result<u8> {
    let platform = Platform::detect().context("cannot detect the current platform")?;
    let home = match &cli.home {
        Some(dir) => RuntimeHome::at(dir.clone()),
        None => RuntimeHome::resolve()?,
    };
    // 独立二进制的 shim 必须调用 `runex`（而不是 `lan env`），并带上自身路径兜底。
    let store = RuntimeStore::new(home, platform)
        .with_shim_host(ShimHost::binary("runex", std::env::current_exe().ok()));
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match &cli.command {
        Command::Install { tool, version } => install(&store, tool, version, cli.json, cli.dry_run),
        Command::Use {
            tool,
            version,
            project,
        } => use_version(&store, tool, version, *project, cli.json, cli.dry_run),
        Command::Remove { tool, version } => remove(&store, tool, version, cli.json, cli.dry_run),
        Command::List { installed } => list(&store, &cwd, *installed, cli.json),
        Command::Info { tool } => info(&store, &cwd, tool, cli.json),
        Command::Versions { tool, available } => {
            versions(&store, tool, *available, cli.json, cli.dry_run)
        }
        Command::Which { name, .. } => which(&store, name, cli.json),
        Command::Init { shell } => init(&store, shell.as_deref(), cli.json),
        Command::Doctor => doctor(&store, cli.json),
    }
}

fn install(
    store: &RuntimeStore,
    tool: &str,
    version: &str,
    json: bool,
    dry_run: bool,
) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    let provider = provider_for(id, store.platform())
        .with_context(|| format!("{id} is not self-managed yet"))?;
    let upstream = provider.upstream().to_string();

    if dry_run {
        return emit(
            json,
            json!({
                "ok": true,
                "dryRun": true,
                "action": "install",
                "tool": id,
                "version": version,
                "upstream": upstream,
            }),
            || println!("would install {id} {version} from {upstream}"),
        );
    }

    if let Some(reason) = lania_runtime_manager::provider::platform_limitation(id, store.platform())
    {
        bail!("cannot install {id} on this platform: {reason}");
    }

    let outcome = store
        .install(id, version, &mut |event| report_event(event))
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    for warning in &outcome.warnings {
        eprintln!("warning: {warning}");
    }
    emit(
        json,
        json!({
            "ok": true,
            "tool": outcome.language,
            "version": outcome.version,
            "path": outcome.path.display().to_string(),
            "fromCache": outcome.from_cache,
            "warnings": outcome.warnings,
        }),
        || {
            println!(
                "installed {} {} -> {}",
                outcome.language,
                outcome.version,
                outcome.path.display()
            )
        },
    )
}

fn use_version(
    store: &RuntimeStore,
    tool: &str,
    version: &str,
    project: bool,
    json: bool,
    dry_run: bool,
) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    if dry_run {
        return emit(
            json,
            json!({"ok": true, "dryRun": true, "action": "use", "tool": id, "version": version}),
            || println!("would point {id} at {version}"),
        );
    }
    let path = store
        .use_version(id, version)
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    let mut pinned = Value::Null;
    if project {
        // 项目级 pin：只写该语言的第一个版本文件（与 `lan env use --project` 一致）。
        let file = language
            .version_files()
            .first()
            .copied()
            .with_context(|| format!("{id} has no project version file"))?;
        let target = std::env::current_dir()?.join(file);
        std::fs::write(&target, format!("{version}\n"))
            .with_context(|| format!("failed to write {}", target.display()))?;
        pinned = json!(target.display().to_string());
    }

    emit(
        json,
        json!({
            "ok": true,
            "tool": id,
            "version": version,
            "path": path.display().to_string(),
            "pinnedFile": pinned,
        }),
        || {
            println!("{id} -> {version} ({})", path.display());
            if project {
                println!("pinned project version file");
            }
        },
    )
}

fn remove(
    store: &RuntimeStore,
    tool: &str,
    version: &str,
    json: bool,
    dry_run: bool,
) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    if dry_run {
        return emit(
            json,
            json!({"ok": true, "dryRun": true, "action": "remove", "tool": id, "version": version}),
            || println!("would remove {id} {version}"),
        );
    }
    let path = store
        .remove(id, version)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    emit(
        json,
        json!({"ok": true, "tool": id, "version": version, "path": path.display().to_string()}),
        || println!("removed {}", path.display()),
    )
}

fn list(store: &RuntimeStore, cwd: &std::path::Path, installed: bool, json: bool) -> Result<u8> {
    if installed {
        let rows = store
            .installed_all()
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let items: Vec<Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "tool": row.language,
                    "version": row.version,
                    "current": row.current,
                    "path": row.path,
                })
            })
            .collect();
        return emit(json, json!({"installed": items}), || {
            if rows.is_empty() {
                println!("no managed runtimes yet; try `runex install node 22`");
            }
            for row in &rows {
                println!(
                    "{}{:<10} {:<14} {}",
                    if row.current { "* " } else { "  " },
                    row.language,
                    row.version,
                    row.path
                );
            }
        });
    }

    let tools = EnvService::new().scan(cwd);
    let items: Vec<Value> = tools
        .iter()
        .map(|tool| {
            json!({
                "tool": tool.language,
                "name": tool.name,
                "version": tool.version,
                "installed": tool.installed,
                "source": tool.source,
                "managed": tool.managed,
                "upstream": tool.upstream,
                "installedCount": tool.installed_count,
            })
        })
        .collect();
    emit(json, json!({"tools": items}), || {
        for tool in &tools {
            let state = match (&tool.version, tool.source.as_deref()) {
                (Some(version), Some("lania")) => format!("{version} (managed)"),
                (Some(version), Some(source)) => format!("{version} ({source})"),
                (Some(version), None) => version.clone(),
                (None, _) => {
                    if tool.managed {
                        "not installed".to_string()
                    } else {
                        "-".to_string()
                    }
                }
            };
            println!(
                "{:<10} {:<24} {}",
                tool.language,
                state,
                if tool.managed {
                    "self-managed"
                } else {
                    "detect-only"
                }
            );
        }
    })
}

fn info(store: &RuntimeStore, cwd: &std::path::Path, tool: &str, json: bool) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    let status = EnvService::new().status(language, cwd);
    let provider = describe_provider(id, store.platform());
    let value = json!({
        "tool": id,
        "name": language.display_name(),
        "version": status.version,
        "source": status.source,
        "path": status.binary_path,
        "managed": status.managed,
        "entrypoints": provider.as_ref().map(|info| info.entrypoints.clone()),
        "upstream": provider.as_ref().map(|info| info.upstream.clone()),
        "supported": provider.as_ref().map(|info| info.supported),
        "unsupportedReason": provider.as_ref().and_then(|info| info.unsupported_reason.clone()),
    });
    emit(json, value, || {
        println!("{} ({})", language.display_name(), id);
        println!("  version   : {}", status.version.as_deref().unwrap_or("-"));
        println!("  source    : {}", status.source.as_deref().unwrap_or("-"));
        println!(
            "  path      : {}",
            status.binary_path.as_deref().unwrap_or("-")
        );
        println!(
            "  managed   : {}",
            if status.managed { "yes" } else { "no" }
        );
        if let Some(provider) = &provider {
            println!("  upstream  : {}", provider.upstream);
            println!("  entrypoint: {}", provider.entrypoints.join(", "));
            if let Some(reason) = &provider.unsupported_reason {
                println!("  platform  : unsupported ({reason})");
            }
        }
    })
}

fn versions(
    store: &RuntimeStore,
    tool: &str,
    available: bool,
    json: bool,
    dry_run: bool,
) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    let installed = store
        .installed_versions(id)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let current = installed
        .iter()
        .find(|row| row.current)
        .map(|row| row.version.clone());

    if available && dry_run {
        return emit(
            json,
            json!({"ok": true, "dryRun": true, "tool": id, "installed": installed.len()}),
            || println!("would query upstream versions for {id}"),
        );
    }

    let upstream: Vec<String> = if available {
        store
            .available_versions(id)
            .map_err(|error| anyhow::anyhow!("{error}"))?
            .into_iter()
            .map(|tag| tag.raw)
            .collect()
    } else {
        Vec::new()
    };

    let value = json!({
        "tool": id,
        "current": current,
        "installed": installed.iter().map(|row| json!({
            "version": row.version,
            "current": row.current,
            "path": row.path,
        })).collect::<Vec<_>>(),
        "available": upstream,
    });
    emit(json, value, || {
        println!("installed:");
        if installed.is_empty() {
            println!("  (none)");
        }
        for row in &installed {
            println!("  {}{}", if row.current { "* " } else { "  " }, row.version);
        }
        if available {
            println!("available (upstream):");
            for tag in &upstream {
                println!("  {tag}");
            }
        }
    })
}

fn which(store: &RuntimeStore, name: &str, json: bool) -> Result<u8> {
    let path = store
        .which(name)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    emit(
        json,
        json!({"name": name, "path": path.display().to_string()}),
        || println!("{}", path.display()),
    )
}

fn init(store: &RuntimeStore, shell: Option<&str>, json: bool) -> Result<u8> {
    let shell = shell.map(str::to_string).unwrap_or_else(default_shell);
    let snippet = store.init_snippet(&shell);
    emit(
        json,
        json!({"shell": shell, "home": store.home().root().display().to_string(), "snippet": snippet}),
        || print!("{snippet}"),
    )
}

fn doctor(store: &RuntimeStore, json: bool) -> Result<u8> {
    let diagnostics = store
        .diagnose()
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let items: Vec<Value> = diagnostics
        .iter()
        .map(|item| json!({"level": level_label(item.level), "message": item.message}))
        .collect();
    let has_error = diagnostics
        .iter()
        .any(|item| item.level == DiagnosticLevel::Error);
    emit(json, json!({"diagnostics": items}), || {
        if diagnostics.is_empty() {
            println!("everything looks fine");
        }
        for item in &diagnostics {
            println!("[{}] {}", level_label(item.level), item.message);
        }
    })?;
    Ok(u8::from(has_error))
}

fn ensure_language(tool: &str) -> Result<Language> {
    Language::parse_id(tool).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported tool `{tool}`; supported: {}",
            supported_language_ids().join(", ")
        )
    })
}

fn report_event(event: InstallEvent) {
    match event {
        InstallEvent::Resolving => eprintln!("resolving version..."),
        InstallEvent::Downloading { url, cached } => {
            if cached {
                eprintln!("using cached {}", file_label(&url));
            } else {
                eprintln!("downloading {}", file_label(&url));
            }
        }
        InstallEvent::Verifying => eprintln!("verifying checksum..."),
        InstallEvent::Extracting => eprintln!("extracting..."),
        InstallEvent::Finalizing => eprintln!("laying out..."),
        InstallEvent::Done { version } => eprintln!("installed {version}"),
    }
}

fn file_label(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or(url)
}

fn default_shell() -> String {
    if cfg!(windows) {
        return "powershell".to_string();
    }
    std::env::var("SHELL")
        .ok()
        .and_then(|value| {
            value
                .rsplit('/')
                .next()
                .map(|name| name.trim_end_matches(".exe").to_string())
        })
        .unwrap_or_else(|| "bash".to_string())
}

fn level_label(level: DiagnosticLevel) -> &'static str {
    match level {
        DiagnosticLevel::Info => "info",
        DiagnosticLevel::Warn => "warn",
        DiagnosticLevel::Error => "error",
    }
}

fn emit(json: bool, value: Value, human: impl FnOnce()) -> Result<u8> {
    if json {
        println!("{value}");
    } else {
        human();
    }
    Ok(0)
}
