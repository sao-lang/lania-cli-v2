//! 命令实现：全部语义都落在 `runex` 的 Rust API 上，
//! 这里只负责「参数校验 + 输出形态」。

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use runex::api::{
    describe_provider, provider_for, supported_language_ids, DiagnosticLevel, InstallEvent,
    Language, Platform, Runex, RuntimeHome, ShimHost,
};
use serde_json::{json, Value};

use crate::cli::{Cli, Command};

pub fn run(cli: Cli) -> Result<u8> {
    let platform = Platform::detect().context("cannot detect the current platform")?;
    let home = match &cli.home {
        Some(dir) => RuntimeHome::at(dir.clone()),
        None => RuntimeHome::resolve()?,
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // 独立二进制的 shim 必须调用 `runex`（而不是 `lan env`），并带上自身路径兜底。
    let api = Runex::new(home, platform)
        .with_cwd(cwd.clone())
        .with_shim_host(ShimHost::binary("runex", std::env::current_exe().ok()));

    match &cli.command {
        Command::Install { tool, version } => install(&api, tool, version, cli.json, cli.dry_run),
        Command::Use {
            tool,
            version,
            project,
        } => use_version(
            &api,
            &cwd,
            tool,
            version.as_deref(),
            *project,
            cli.json,
            cli.dry_run,
        ),
        Command::Remove { tool, version } => remove(&api, tool, version, cli.json, cli.dry_run),
        Command::List { installed } => list(&api, *installed, cli.json),
        Command::Info { tool } => info(&api, tool, cli.json),
        Command::Versions { tool, available } => {
            versions(&api, tool, *available, cli.json, cli.dry_run)
        }
        Command::Which { name, .. } => which(&api, name, cli.json),
        Command::Init { shell } => init(&api, shell.as_deref(), cli.json),
        Command::Doctor => doctor(&api, cli.json),
        Command::Ui {
            snapshot,
            cols,
            rows,
            down,
            no_versions,
            locale,
        } => ui(
            &api,
            *snapshot,
            *cols,
            *rows,
            *down,
            *no_versions,
            locale.as_deref(),
            cli.json,
        ),
    }
}

fn install(api: &Runex, tool: &str, version: &str, json: bool, dry_run: bool) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    let provider = provider_for(id, api.platform())
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

    // `latest`：取上游清单里最新的稳定版（与 nvm `install node` 的直觉一致）。
    let resolved = if version.eq_ignore_ascii_case("latest") {
        resolve_latest(api, id)?
    } else {
        version.to_string()
    };

    if let Some(reason) = runex::api::platform_limitation(id, api.platform()) {
        bail!("cannot install {id} on this platform: {reason}");
    }

    let outcome = api
        .install(id, &resolved, &mut |event| report_event(event))
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
    api: &Runex,
    cwd: &std::path::Path,
    tool: &str,
    version: Option<&str>,
    project: bool,
    json: bool,
    dry_run: bool,
) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    // 省略版本时读项目版本文件（nvm 的 `nvm use` 习惯）。
    let requested = match version {
        Some(value) => value.to_string(),
        None => read_project_version(language, cwd)?,
    };

    if dry_run {
        return emit(
            json,
            json!({"ok": true, "dryRun": true, "action": "use", "tool": id, "version": requested}),
            || println!("would point {id} at {requested}"),
        );
    }
    let path = api
        .use_version(id, &requested)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    // 前缀（`22`）会被解析到具体补丁版本，报告里必须显示落地的那个版本。
    let resolved = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| requested.clone());

    let mut pinned = Value::Null;
    if project {
        // 项目级 pin：只写该语言的第一个版本文件（与 `lan env use --project` 一致）。
        let file = language
            .version_files()
            .first()
            .copied()
            .with_context(|| format!("{id} has no project version file"))?;
        let target = cwd.join(file);
        std::fs::write(&target, format!("{resolved}\n"))
            .with_context(|| format!("failed to write {}", target.display()))?;
        pinned = json!(target.display().to_string());
    }

    emit(
        json,
        json!({
            "ok": true,
            "tool": id,
            "version": resolved,
            "requested": requested,
            "path": path.display().to_string(),
            "pinnedFile": pinned,
        }),
        || {
            println!("{id} -> {resolved} ({})", path.display());
            if project {
                println!("pinned project version file");
            }
        },
    )
}

fn remove(api: &Runex, tool: &str, version: &str, json: bool, dry_run: bool) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    if dry_run {
        return emit(
            json,
            json!({"ok": true, "dryRun": true, "action": "remove", "tool": id, "version": version}),
            || println!("would remove {id} {version}"),
        );
    }
    let path = api
        .remove(id, version)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    emit(
        json,
        json!({"ok": true, "tool": id, "version": version, "path": path.display().to_string()}),
        || println!("removed {}", path.display()),
    )
}

fn list(api: &Runex, installed: bool, json: bool) -> Result<u8> {
    if installed {
        let rows = api
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

    let tools = api.scan();
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

fn info(api: &Runex, tool: &str, json: bool) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    let status = api.status(id).map_err(|error| anyhow::anyhow!("{error}"))?;
    let provider = describe_provider(id, api.platform());
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

fn versions(api: &Runex, tool: &str, available: bool, json: bool, dry_run: bool) -> Result<u8> {
    let language = ensure_language(tool)?;
    let id = language.id();
    let installed = api
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
        api.available_versions(id)
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

fn which(api: &Runex, name: &str, json: bool) -> Result<u8> {
    let path = api
        .which(name)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    emit(
        json,
        json!({"name": name, "path": path.display().to_string()}),
        || println!("{}", path.display()),
    )
}

fn init(api: &Runex, shell: Option<&str>, json: bool) -> Result<u8> {
    let shell = shell.map(str::to_string).unwrap_or_else(default_shell);
    let snippet = api.init_snippet(&shell);
    emit(
        json,
        json!({"shell": shell, "home": api.home().root().display().to_string(), "snippet": snippet}),
        || print!("{snippet}"),
    )
}

fn doctor(api: &Runex, json: bool) -> Result<u8> {
    let diagnostics = api.diagnose().map_err(|error| anyhow::anyhow!("{error}"))?;
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

#[allow(clippy::too_many_arguments)]
fn ui(
    api: &Runex,
    snapshot: bool,
    cols: Option<u16>,
    rows: Option<u16>,
    down: Option<usize>,
    no_versions: bool,
    locale: Option<&str>,
    json: bool,
) -> Result<u8> {
    let locale = locale.map(str::to_string).unwrap_or_else(default_locale);
    if snapshot {
        let down = down.unwrap_or(0);
        let frame = api
            .tui_snapshot(
                cols.unwrap_or(100),
                rows.unwrap_or(28),
                down,
                !no_versions,
                &locale,
            )
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        return emit(
            json,
            json!({"kind": "runex_ui_snapshot", "frame": frame, "down": down}),
            || print!("{frame}"),
        );
    }

    api.run_tui(&locale)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    emit(
        json,
        json!({"kind": "runex_ui", "status": "closed"}),
        || println!("closed"),
    )
}

fn ensure_language(tool: &str) -> Result<Language> {
    Language::parse_id(tool).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported tool `{tool}`; supported: {}",
            supported_language_ids().join(", ")
        )
    })
}

/// 取上游清单里最新的稳定版（`install <tool>` 的默认行为）。
fn resolve_latest(api: &Runex, id: &str) -> Result<String> {
    let versions = api
        .available_versions(id)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    versions
        .iter()
        .find(|tag| !tag.prerelease)
        .map(|tag| tag.raw.clone())
        .with_context(|| format!("{id} has no stable release upstream"))
}

/// 从项目版本文件（`.nvmrc` / `.python-version` / `go.mod` …）读出版本约束。
fn read_project_version(language: Language, cwd: &std::path::Path) -> Result<String> {
    let id = language.id();
    if let Some((file, version)) = language.read_version_file(cwd) {
        eprintln!("using {file} -> {version}");
        return Ok(version);
    }
    bail!(
        "{id} has no version file in {}; pass a version explicitly (looked for: {})",
        cwd.display(),
        language.version_files().join(", ")
    )
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

fn default_locale() -> String {
    for key in ["RUNEX_LOCALE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            if value.to_ascii_lowercase().starts_with("zh") {
                return "zh".to_string();
            }
        }
    }
    "en".to_string()
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
