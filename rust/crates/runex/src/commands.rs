//! 命令实现：全部语义都落在 `runex` 的 Rust API 上，
//! 这里只负责「参数校验 + 输出形态」。

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use runex::api::{
    describe_provider, provider_for, supported_language_ids, DiagnosticLevel, InstallEvent,
    Language, Platform, Runex, RuntimeHome, ShimHost,
};
use runex::Locale;
use serde_json::{json, Value};

use crate::cli::{Cli, Command};

/// CLI 进程内生效的 locale；`run` 在解析参数后设置一次。
static LOCALE: OnceLock<Locale> = OnceLock::new();

fn tr(en: &'static str, zh: &'static str) -> &'static str {
    LOCALE.get().copied().unwrap_or_default().tr(en, zh)
}

fn is_zh() -> bool {
    LOCALE.get().copied().unwrap_or_default().is_zh()
}

pub fn run(cli: Cli) -> Result<u8> {
    let locale = Locale::resolve(cli.locale.as_deref());
    let _ = LOCALE.set(locale);
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
        } => ui(&api, *snapshot, *cols, *rows, *down, *no_versions, cli.json),
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
            || {
                if is_zh() {
                    println!("将安装 {id} {version}，来源 {upstream}");
                } else {
                    println!("would install {id} {version} from {upstream}");
                }
            },
        );
    }

    // `latest`：取上游清单里最新的稳定版（与 nvm `install node` 的直觉一致）。
    let resolved = if version.eq_ignore_ascii_case("latest") {
        if is_zh() {
            eprintln!("正在解析 {id} 最新稳定版...");
        } else {
            eprintln!("resolving latest {id} version...");
        }
        resolve_latest(api, id)?
    } else {
        version.to_string()
    };

    if let Some(reason) = runex::api::platform_limitation(id, api.platform()) {
        if is_zh() {
            bail!("当前平台无法安装 {id}：{reason}");
        }
        bail!("cannot install {id} on this platform: {reason}");
    }

    let mut reporter = ProgressReporter::default();
    let outcome = api.install(id, &resolved, &mut |event| reporter.event(event));
    reporter.finish();
    let outcome = outcome.map_err(|error| anyhow::anyhow!("{error}"))?;

    for warning in &outcome.warnings {
        if is_zh() {
            eprintln!("警告：{warning}");
        } else {
            eprintln!("warning: {warning}");
        }
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
            if is_zh() {
                println!(
                    "已安装 {} {} -> {}",
                    outcome.language,
                    outcome.version,
                    outcome.path.display()
                );
            } else {
                println!(
                    "installed {} {} -> {}",
                    outcome.language,
                    outcome.version,
                    outcome.path.display()
                );
            }
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
            || {
                if is_zh() {
                    println!("将把 {id} 切换到 {requested}");
                } else {
                    println!("would point {id} at {requested}");
                }
            },
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
                println!(
                    "{}",
                    tr("pinned project version file", "已写入项目版本文件")
                );
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
            || {
                if is_zh() {
                    println!("将删除 {id} {version}");
                } else {
                    println!("would remove {id} {version}");
                }
            },
        );
    }
    let path = api
        .remove(id, version)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    emit(
        json,
        json!({"ok": true, "tool": id, "version": version, "path": path.display().to_string()}),
        || {
            if is_zh() {
                println!("已删除 {}", path.display());
            } else {
                println!("removed {}", path.display());
            }
        },
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
                println!(
                    "{}",
                    tr(
                        "no managed runtimes yet; try `runex install node 22`",
                        "还没有自管运行时，试试 `runex install node 22`"
                    )
                );
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
                (Some(version), Some("lania")) if is_zh() => format!("{version} (自管)"),
                (Some(version), Some("lania")) => format!("{version} (managed)"),
                (Some(version), Some(source)) => format!("{version} ({source})"),
                (Some(version), None) => version.clone(),
                (None, _) => {
                    if tool.managed {
                        tr("not installed", "未安装").to_string()
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
                    tr("self-managed", "自管")
                } else {
                    tr("detect-only", "仅探测")
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
        println!(
            "  {}: {}",
            tr("version   ", "版本      "),
            status.version.as_deref().unwrap_or("-")
        );
        println!(
            "  {}: {}",
            tr("source    ", "来源      "),
            status.source.as_deref().unwrap_or("-")
        );
        println!(
            "  {}: {}",
            tr("path      ", "路径      "),
            status.binary_path.as_deref().unwrap_or("-")
        );
        println!(
            "  {}: {}",
            tr("managed   ", "自管      "),
            if status.managed {
                tr("yes", "是")
            } else {
                tr("no", "否")
            }
        );
        if let Some(provider) = &provider {
            println!(
                "  {}: {}",
                tr("upstream  ", "上游      "),
                provider.upstream
            );
            println!(
                "  {}: {}",
                tr("entrypoint", "入口      "),
                provider.entrypoints.join(", ")
            );
            if let Some(reason) = &provider.unsupported_reason {
                println!(
                    "  {}: {} ({reason})",
                    tr("platform  ", "平台      "),
                    tr("unsupported", "不支持")
                );
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
            || {
                if is_zh() {
                    println!("将查询 {id} 的上游版本");
                } else {
                    println!("would query upstream versions for {id}");
                }
            },
        );
    }

    let upstream: Vec<String> = if available {
        if is_zh() {
            eprintln!("正在查询 {id} 的上游版本...");
        } else {
            eprintln!("querying upstream versions for {id}...");
        }
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
        println!("{}", tr("installed:", "已安装："));
        if installed.is_empty() {
            println!("  {}", tr("(none)", "（无）"));
        }
        for row in &installed {
            println!("  {}{}", if row.current { "* " } else { "  " }, row.version);
        }
        if available {
            println!("{}", tr("available (upstream):", "上游可用版本："));
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
            println!("{}", tr("everything looks fine", "一切正常"));
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
    json: bool,
) -> Result<u8> {
    let locale = LOCALE.get().copied().unwrap_or_default().code();
    if snapshot {
        let down = down.unwrap_or(0);
        let frame = api
            .tui_snapshot(
                cols.unwrap_or(100),
                rows.unwrap_or(28),
                down,
                !no_versions,
                locale,
            )
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        return emit(
            json,
            json!({"kind": "runex_ui_snapshot", "frame": frame, "down": down}),
            || print!("{frame}"),
        );
    }

    api.run_tui(locale)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    emit(
        json,
        json!({"kind": "runex_ui", "status": "closed"}),
        || println!("{}", tr("closed", "已关闭")),
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
        if is_zh() {
            eprintln!("使用 {file} -> {version}");
        } else {
            eprintln!("using {file} -> {version}");
        }
        return Ok(version);
    }
    if is_zh() {
        bail!(
            "{id} 在 {} 中没有版本文件；请显式指定版本（已查找：{}）",
            cwd.display(),
            language.version_files().join(", ")
        );
    }
    bail!(
        "{id} has no version file in {}; pass a version explicitly (looked for: {})",
        cwd.display(),
        language.version_files().join(", ")
    )
}

#[derive(Default)]
struct ProgressReporter {
    /// 当前是否有一行用 `\r` 覆盖的下载进度。
    line_open: bool,
    last_bytes: u64,
    last_at: Option<Instant>,
    /// 指数平滑后的字节/秒，避免进度条速度抖动。
    speed: f64,
}

impl ProgressReporter {
    fn event(&mut self, event: InstallEvent) {
        match event {
            InstallEvent::Resolving => self.stage(tr("resolving version...", "正在解析版本...")),
            InstallEvent::Downloading { url, cached } => {
                if cached {
                    if is_zh() {
                        self.stage(&format!("使用缓存 {}", file_label(&url)));
                    } else {
                        self.stage(&format!("using cached {}", file_label(&url)));
                    }
                } else if is_zh() {
                    self.stage(&format!("正在下载 {}", file_label(&url)));
                } else {
                    self.stage(&format!("downloading {}", file_label(&url)));
                }
            }
            InstallEvent::DownloadProgress {
                url,
                downloaded,
                total,
            } => self.progress(&url, downloaded, total),
            InstallEvent::Verifying => self.stage(tr("verifying checksum...", "正在校验校验和...")),
            InstallEvent::Extracting => self.stage(tr("extracting...", "正在解压...")),
            InstallEvent::Finalizing => self.stage(tr("laying out...", "正在整理目录...")),
            InstallEvent::Done { version } => {
                if is_zh() {
                    self.stage(&format!("已安装 {version}"));
                } else {
                    self.stage(&format!("installed {version}"));
                }
            }
        }
    }

    fn stage(&mut self, message: &str) {
        self.finish();
        eprintln!("{message}");
    }

    fn progress(&mut self, url: &str, downloaded: u64, total: Option<u64>) {
        let now = Instant::now();
        if let Some(last_at) = self.last_at {
            let elapsed = now.duration_since(last_at).as_secs_f64();
            if elapsed > 0.0 && downloaded >= self.last_bytes {
                let instant = (downloaded - self.last_bytes) as f64 / elapsed;
                self.speed = if self.speed == 0.0 {
                    instant
                } else {
                    self.speed * 0.7 + instant * 0.3
                };
            }
        }
        self.last_bytes = downloaded;
        self.last_at = Some(now);

        let speed = if self.speed > 0.0 {
            format!("  {}/s", human_bytes(self.speed as u64))
        } else {
            String::new()
        };
        let label = file_label(url);
        let line = match total {
            Some(total) if total > 0 => {
                let eta = if self.speed > 0.0 {
                    format!(
                        "  ETA {}",
                        format_eta((total.saturating_sub(downloaded)) as f64 / self.speed)
                    )
                } else {
                    String::new()
                };
                format!(
                    "\r{label}  {} / {}  ({:.0}%){speed}{eta}",
                    human_bytes(downloaded),
                    human_bytes(total),
                    downloaded as f64 * 100.0 / total as f64
                )
            }
            _ => format!("\r{label}  {}{speed}", human_bytes(downloaded)),
        };
        eprint!("{line}");
        let _ = std::io::stderr().flush();
        self.line_open = true;
    }

    fn finish(&mut self) {
        if self.line_open {
            eprintln!();
            self.line_open = false;
        }
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_eta(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
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
