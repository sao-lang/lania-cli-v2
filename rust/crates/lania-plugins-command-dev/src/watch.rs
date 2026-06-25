//! `lan product dev` 的运行时实现（once + watch）。
//!
//! 设计动机：
//! - product dev 本质是“转发执行”：让用户在开发期用同一个 `lan` 二进制跑 product 命令。
//! - 为了尽量贴近真实安装后的行为，这里选择“re-exec 当前 CLI”而不是在进程内直接调用。
//!
//! watch 模式实现策略：
//! - 优先尝试用 `notify` 做文件系统事件监听（低延迟、低 CPU）
//! - 如果 notify 初始化失败或平台不支持，则回退到轮询 fingerprint（更稳但更耗）
//! - 为避免编辑器一次保存触发多次事件，对 notify 信号做 debounce
//!
//! 环境变量约定（对子进程生效）：
//! - `LANIA_PRODUCT_ROOT`：指向 product workspace root
//! - `LANIA_RUNTIME_MODE=development`：强制 development 解析规则
//! - `LANIA_CLI_DEV_PRODUCT=1`：标记位，便于诊断/日志区分

use anyhow::{bail, Result};
use lania_command::CommandContext;
use lania_host::{execution::CommandExecution, execution::CommandExecutionContext};
use serde_json::json;
use std::{
    env, fs,
    hash::{Hash, Hasher},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub(crate) struct ProductDevOptions {
    pub(crate) product_root: String,
    pub(crate) forwarded_args: Vec<String>,
    pub(crate) watch: bool,
    pub(crate) poll_interval: Duration,
}

// product dev 运行时：
// - 解析“需要转发执行的 product 命令参数”与 watch 相关选项
// - 非 watch 模式：只执行一次
// - watch 模式：监控 product 目录变化，自动重启/重新运行
//
// 子进程环境变量：
// - `LANIA_PRODUCT_ROOT`：告诉运行时 product workspace root 在哪里
// - `LANIA_RUNTIME_MODE=development`：强制使用 development 解析规则
// - `LANIA_CLI_DEV_PRODUCT=1`：标记位，便于诊断/下游工具识别“这是 dev product 子进程”

pub(crate) fn resolve_product_dev_options(
    command: &CommandContext,
    locale: &str,
) -> Result<ProductDevOptions> {
    let raw_path = command
        .argv
        .options
        .get("path")
        .and_then(|value| value.as_str());
    let product_root = match raw_path.map(str::trim) {
        Some(raw) if !raw.is_empty() => {
            let path = Path::new(raw);
            if path.is_absolute() {
                raw.to_string()
            } else {
                Path::new(&command.cwd).join(raw).display().to_string()
            }
        }
        _ => command.cwd.clone(),
    };
    let forwarded_args = command
        .argv
        .args
        .get("args")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    if forwarded_args.is_empty() {
        bail!(
            "{}",
            if locale == "zh" {
                "缺少 product 命令参数，请使用 `lan product dev <command>`"
            } else {
                "missing product command arguments, try `lan product dev <command>`"
            }
        );
    }
    let watch = command
        .argv
        .options
        .get("watch")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let poll_interval_ms = command
        .argv
        .options
        .get("poll-interval-ms")
        .and_then(|value| value.as_u64())
        .unwrap_or(500)
        .max(50);
    Ok(ProductDevOptions {
        product_root,
        forwarded_args,
        watch,
        poll_interval: Duration::from_millis(poll_interval_ms),
    })
}

pub(crate) fn run_product_dev_once(
    ctx: &CommandExecutionContext<'_>,
    options: ProductDevOptions,
) -> Result<CommandExecution> {
    // 这里选择“重新执行当前 CLI（re-exec）”而不是进程内调用：
    // - 行为更贴近真实安装后的二进制（包括 argv 解析、环境变量读取等）
    // - 可以通过 env 注入切换运行模式，而不用在 Rust 侧重写一套命令分发
    let current_exe = env::current_exe()?;
    let current_exe = current_exe.display().to_string();
    let mut child = Command::new(&current_exe);
    child
        .args(&options.forwarded_args)
        .current_dir(&ctx.command().cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LANIA_PRODUCT_ROOT", &options.product_root)
        .env("LANIA_RUNTIME_MODE", "development")
        .env("LANIA_CLI_DEV_PRODUCT", "1");
    let output = child.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let parsed_stdout = serde_json::from_str::<serde_json::Value>(stdout.trim())
        .unwrap_or_else(|_| json!({ "raw": stdout }));

    // 这里的 kind 设为 `product_dev`：
    // - 它不是 bridge 侧的 kind
    // - 因为这条路径不经过 node-bridge，而是 Rust 侧直接执行并封装结果
    Ok(ctx.complete_template_info(
        json!({
            "kind": "product_dev",
            "productRoot": options.product_root,
            "forwardedArgs": options.forwarded_args,
            "status": output.status.code(),
            "stdout": parsed_stdout,
            "stderr": stderr,
            "childExe": current_exe,
        }),
        output.status.code().unwrap_or(lania_host::EXIT_RUNTIME_ERROR),
    ))
}

pub(crate) async fn run_product_dev_watch(
    ctx: &CommandExecutionContext<'_>,
    options: ProductDevOptions,
) -> Result<CommandExecution> {
    let current_exe = env::current_exe()?;
    let current_exe = current_exe.display().to_string();
    let (watcher, mut changes) =
        try_create_product_watcher(&options.product_root).unwrap_or_else(|_| (None, None));
    let mut fingerprint = compute_product_watch_fingerprint(&options.product_root);
    let mut restart_count = 0usize;
    let mut change_count = 0usize;
    let mut last_exit_code: Option<i32> = None;
    let _watcher_guard = watcher;
    let using_notify = changes.is_some();

    loop {
        let mut child = spawn_watch_child(&current_exe, &ctx.command().cwd, &options)?;
        let mut restart_child = false;
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    let _ = child.kill().await;
                    return Ok(build_watch_result(
                        ctx,
                        &options,
                        using_notify,
                        restart_count,
                        change_count,
                        last_exit_code,
                        &current_exe,
                    ));
                }
                status = child.wait() => {
                    let status = status?;
                    last_exit_code = status.code();
                    break;
                }
                Some(_) = async {
                    if let Some(rx) = &mut changes { rx.recv().await } else { None }
                }, if using_notify => {
                    // watcher 事件去抖（debounce）：
                    // - 编辑器一次保存往往会触发多次文件事件
                    // - 不去抖会导致短时间内频繁重启，体验很差
                    tokio::time::sleep(Duration::from_millis(120)).await;
                    while let Some(rx) = &mut changes {
                        if rx.try_recv().is_err() { break; }
                    }
                    change_count += 1;
                    restart_count += 1;
                    eprintln!("{}", restart_message(ctx.locale(), restart_count, true));
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    restart_child = true;
                    break;
                }
                _ = tokio::time::sleep(options.poll_interval), if !using_notify => {
                    // 轮询 fallback：
                    // - 某些平台/文件系统 notify 可能不可用或不稳定
                    // - 这里用“目录 fingerprint”做近似判断：发生变化则重启
                    let next_fingerprint = compute_product_watch_fingerprint(&options.product_root);
                    if next_fingerprint != fingerprint {
                        fingerprint = next_fingerprint;
                        change_count += 1;
                        restart_count += 1;
                        eprintln!("{}", restart_message(ctx.locale(), restart_count, true));
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        restart_child = true;
                        break;
                    }
                }
            }
        }

        if restart_child {
            continue;
        }

        // 子进程自然退出：并不立刻退出 dev watch，而是等待下一次文件变化再重新运行。
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    return Ok(build_watch_result(
                        ctx,
                        &options,
                        using_notify,
                        restart_count,
                        change_count,
                        last_exit_code,
                        &current_exe,
                    ));
                }
                Some(_) = async {
                    if let Some(rx) = &mut changes { rx.recv().await } else { None }
                }, if using_notify => {
                    // “退出后等待变化再重跑”的场景同样需要去抖。
                    tokio::time::sleep(Duration::from_millis(120)).await;
                    while let Some(rx) = &mut changes {
                        if rx.try_recv().is_err() { break; }
                    }
                    change_count += 1;
                    restart_count += 1;
                    eprintln!("{}", restart_message(ctx.locale(), restart_count, false));
                    break;
                }
                _ = tokio::time::sleep(options.poll_interval), if !using_notify => {
                    let next_fingerprint = compute_product_watch_fingerprint(&options.product_root);
                    if next_fingerprint != fingerprint {
                        fingerprint = next_fingerprint;
                        change_count += 1;
                        restart_count += 1;
                        eprintln!("{}", restart_message(ctx.locale(), restart_count, false));
                        break;
                    }
                }
            }
        }
    }
}

fn build_watch_result(
    ctx: &CommandExecutionContext<'_>,
    options: &ProductDevOptions,
    using_notify: bool,
    restart_count: usize,
    change_count: usize,
    last_exit_code: Option<i32>,
    current_exe: &str,
) -> CommandExecution {
    ctx.complete_template_info(
        json!({
            "kind": "product_dev_watch",
            "watchMode": if using_notify { "notify" } else { "poll" },
            "productRoot": options.product_root,
            "forwardedArgs": options.forwarded_args,
            "restartCount": restart_count,
            "changeCount": change_count,
            "lastExitCode": last_exit_code,
            "childExe": current_exe,
            "pollIntervalMs": options.poll_interval.as_millis(),
        }),
        last_exit_code.unwrap_or(lania_host::EXIT_SUCCESS),
    )
}

fn restart_message(locale: &str, restart_count: usize, restarting_running_child: bool) -> String {
    match (locale == "zh", restarting_running_child) {
        (true, true) => {
            format!("[lan product dev] 检测到 product 文件变化，正在重启（第 {restart_count} 次）")
        }
        (true, false) => {
            format!("[lan product dev] 检测到 product 文件变化，正在重新运行（第 {restart_count} 次）")
        }
        (false, true) => {
            format!("[lan product dev] detected product file changes, restarting (restart #{restart_count})")
        }
        (false, false) => {
            format!("[lan product dev] detected product file changes, rerunning (restart #{restart_count})")
        }
    }
}

fn try_create_product_watcher(
    product_root: &str,
) -> Result<(Option<notify::RecommendedWatcher>, Option<mpsc::UnboundedReceiver<()>>)> {
    use notify::{EventKind, Watcher};
    let (tx, rx) = mpsc::unbounded_channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return; };
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
            _ => return,
        }
        if event.paths.iter().any(|path| !should_skip_watch_path(path)) {
            let _ = tx.send(());
        }
    })?;
    // 注意：notify 在部分平台/文件系统上可能初始化失败；调用方会自动回退到轮询模式。
    watcher.watch(Path::new(product_root), notify::RecursiveMode::Recursive)?;
    Ok((Some(watcher), Some(rx)))
}

fn spawn_watch_child(
    current_exe: &str,
    cwd: &str,
    options: &ProductDevOptions,
) -> Result<tokio::process::Child> {
    let mut child = TokioCommand::new(current_exe);
    child
        .args(&options.forwarded_args)
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("LANIA_PRODUCT_ROOT", &options.product_root)
        .env("LANIA_RUNTIME_MODE", "development")
        .env("LANIA_CLI_DEV_PRODUCT", "1");
    Ok(child.spawn()?)
}

pub(crate) fn compute_product_watch_fingerprint(product_root: &str) -> u64 {
    // 这是一个“启发式 fingerprint”：
    // - 不需要加密强度或稳定哈希
    // - 只要在“有意义的文件变化”发生时，结果能变化即可
    let mut state = std::collections::hash_map::DefaultHasher::new();
    collect_product_watch_fingerprint(Path::new(product_root), &mut state);
    state.finish()
}

fn collect_product_watch_fingerprint(path: &Path, state: &mut impl Hasher) {
    if should_skip_watch_path(path) {
        return;
    }
    let Ok(metadata) = fs::metadata(path) else {
        path.display().to_string().hash(state);
        return;
    };
    path.display().to_string().hash(state);
    metadata.len().hash(state);
    metadata.is_file().hash(state);
    if let Ok(modified) = metadata.modified() {
        if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
            duration.as_nanos().hash(state);
        }
    }
    if metadata.is_dir() {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut entries = entries.filter_map(|entry| entry.ok()).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            collect_product_watch_fingerprint(&entry.path(), state);
        }
    }
}

fn should_skip_watch_path(path: &Path) -> bool {
    // 跳过这些目录：
    // - `.git`：改动噪声大，与运行时行为无关
    // - `node_modules`：体积巨大，且安装/构建工具频繁改动
    // - `.lania` / `target`：生成目录，变化不应触发 watch 重启
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        matches!(value.as_ref(), ".git" | "node_modules" | ".lania" | "target")
    })
}
