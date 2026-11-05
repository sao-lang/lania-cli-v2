//! 命令行定义：保持 nvm 的肌肉记忆（install / use / ls / ls-remote），
//! 同时用 `<tool>` 参数把「多运行时」显式化。
//!
//! 帮助文本也走 `runex` 的 locale：`main` 在解析前先扫一遍 `--locale` /
//! `RUNEX_HOME/config.json`，再用 [`localized_command`] 构造对应语言的 clap 命令。

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use runex::api::RuntimeHome;
use runex::Locale;

#[derive(Debug, Parser)]
#[command(
    name = "runex",
    version,
    about = "Self-managed multi-runtime version manager",
    long_about = "Install, switch and remove language runtimes without nvm/volta/pyenv.\n\
                  Everything lives under $RUNEX_HOME (default ~/.runex); PATH only needs\n\
                  $RUNEX_HOME/shims prepended once (`runex init`).",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Override the runtime root (same as setting RUNEX_HOME).
    #[arg(long, global = true, value_name = "DIR")]
    pub home: Option<PathBuf>,

    /// Print machine-readable JSON on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Show what would happen without touching the store.
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Human language for CLI / TUI output: en / zh.
    ///
    /// Overrides this run only; the default comes from `runex locale`
    /// (or `RUNEX_LOCALE` / `LANG` when unset).
    #[arg(long, global = true, value_name = "LOCALE")]
    pub locale: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Download, verify and install a runtime version.
    ///
    /// `version` may be a prefix (`20`) or `latest` (omit it for the latest stable).
    #[command(alias = "i")]
    Install {
        /// Language id, e.g. node / python / go.
        tool: String,
        /// Version / prefix / `latest`. Defaults to the latest stable.
        #[arg(default_value = "latest")]
        version: String,
    },

    /// Point the current version of a runtime at an installed version.
    ///
    /// Without a version it reads the project version file (`.nvmrc` etc.).
    #[command(alias = "u")]
    Use {
        tool: String,
        version: Option<String>,
        /// Also write the project version file (e.g. .nvmrc).
        #[arg(long)]
        project: bool,
    },

    /// Delete a runex-managed version.
    #[command(aliases = ["rm", "uninstall"])]
    Remove { tool: String, version: String },

    /// List runtimes and their detected versions.
    #[command(alias = "ls")]
    List {
        /// Only show versions managed by runex.
        #[arg(long)]
        installed: bool,
    },

    /// Show details for one runtime.
    Info { tool: String },

    /// List installed versions; `--available` also queries upstream.
    #[command(alias = "ls-remote")]
    Versions {
        tool: String,
        #[arg(long)]
        available: bool,
    },

    /// Resolve the binary an entrypoint currently points at (used by shims).
    #[command(alias = "where")]
    Which {
        name: String,
        /// Accepted for compatibility with the shim protocol; output is identical.
        #[arg(long, hide = true)]
        resolve: bool,
    },

    /// Print the shell snippet that puts $RUNEX_HOME/shims on PATH.
    Init {
        /// bash / zsh / fish / powershell.
        #[arg(long)]
        shell: Option<String>,
    },

    /// Show or set the default human language for CLI and TUI.
    ///
    /// With no value it prints the current choice; `auto` clears it and
    /// follows `RUNEX_LOCALE` / `LC_ALL` / `LANG` again.
    #[command(alias = "lang")]
    Locale {
        /// en / zh / auto.
        value: Option<String>,
    },

    /// Diagnose pointers, shims and PATH wiring.
    Doctor,

    /// Clean the download cache (installed runtimes are not removed).
    Gc {
        /// Remove every cached artifact, not just ones older than --days.
        #[arg(long)]
        all: bool,
        /// Only remove cache entries older than this many days (default 7).
        #[arg(long, default_value = "7")]
        days: u64,
    },

    /// Open the terminal UI (or render one non-interactive snapshot).
    Ui {
        /// Print one frame instead of starting the interactive TUI.
        #[arg(long)]
        snapshot: bool,
        /// Snapshot width (default 100).
        #[arg(long)]
        cols: Option<u16>,
        /// Snapshot height (default 28).
        #[arg(long)]
        rows: Option<u16>,
        /// Snapshot only: simulate pressing Down N times.
        #[arg(long)]
        down: Option<usize>,
        /// Skip version probing for a fast, layout-only frame.
        #[arg(long = "no-versions")]
        no_versions: bool,
    },
}

/// 按 `locale` 构造 clap 命令；英文直接返回 derive 生成的原文。
pub fn localized_command(locale: Locale) -> clap::Command {
    let mut command = Cli::command();
    if !locale.is_zh() {
        return command;
    }

    command = command
        .about("自管的运行时版本管理器")
        .long_about(
            "不依赖 nvm / volta / pyenv 安装、切换、删除语言运行时。\n\
             所有内容都在 $RUNEX_HOME（默认 ~/.runex）下；PATH 只需要加入\n\
             $RUNEX_HOME/shims 一次（`runex init` 会打印片段）。",
        )
        .mut_arg("home", |arg| {
            arg.help("覆盖运行时根目录（等价于设置 RUNEX_HOME）")
        })
        .mut_arg("json", |arg| arg.help("在 stdout 输出机器可读的 JSON"))
        .mut_arg("dry_run", |arg| arg.help("只显示将要执行的动作，不写盘"))
        .mut_arg("locale", |arg| {
            arg.help("CLI / TUI 的人类语言：en / zh").long_help(
                "只覆盖本次运行；默认值来自 `runex locale`（未设置时跟随 \
                     RUNEX_LOCALE / LANG）。",
            )
        });

    command = command
        .mut_subcommand("install", |sub| {
            sub.about("下载、校验并安装一个运行时版本。")
                .long_about("`version` 可以是前缀（`20`）或 `latest`（省略时取最新稳定版）。")
                .mut_arg("tool", |arg| arg.help("语言 id，例如 node / python / go"))
                .mut_arg("version", |arg| {
                    arg.help("版本 / 前缀 / `latest`，默认最新稳定版")
                })
        })
        .mut_subcommand("use", |sub| {
            sub.about("把某个运行时的当前版本指向已安装版本。")
                .long_about("省略版本时读取项目版本文件（`.nvmrc` 等）。")
                .mut_arg("tool", |arg| arg.help("语言 id"))
                .mut_arg("version", |arg| arg.help("版本；省略时读项目版本文件"))
                .mut_arg("project", |arg| {
                    arg.help("同时写入项目版本文件（如 .nvmrc）")
                })
        })
        .mut_subcommand("remove", |sub| {
            sub.about("删除由 runex 管理的版本。")
                .mut_arg("tool", |arg| arg.help("语言 id"))
                .mut_arg("version", |arg| arg.help("要删除的版本"))
        })
        .mut_subcommand("list", |sub| {
            sub.about("列出运行时及其探测到的版本。")
                .mut_arg("installed", |arg| arg.help("只显示由 runex 管理的版本"))
        })
        .mut_subcommand("info", |sub| {
            sub.about("查看单个运行时的详情。")
                .mut_arg("tool", |arg| arg.help("语言 id"))
        })
        .mut_subcommand("versions", |sub| {
            sub.about("列出已安装版本；`--available` 同时查询上游。")
                .mut_arg("tool", |arg| arg.help("语言 id"))
                .mut_arg("available", |arg| arg.help("同时查询上游可用版本"))
        })
        .mut_subcommand("which", |sub| {
            sub.about("解析入口当前指向的真实二进制（shim 使用）。")
                .mut_arg("name", |arg| arg.help("入口名，例如 node"))
        })
        .mut_subcommand("init", |sub| {
            sub.about("打印把 $RUNEX_HOME/shims 加入 PATH 的 shell 片段。")
                .mut_arg("shell", |arg| arg.help("bash / zsh / fish / powershell"))
        })
        .mut_subcommand("locale", |sub| {
            sub.about("查看或设置 CLI 与 TUI 的默认语言。")
                .long_about(
                    "不带值时显示当前选择；`auto` 清除设置，重新跟随 \
                     RUNEX_LOCALE / LC_ALL / LANG。",
                )
                .mut_arg("value", |arg| arg.help("en / zh / auto"))
        })
        .mut_subcommand("doctor", |sub| {
            sub.about("体检 current 指针、shim 与 PATH。")
        })
        .mut_subcommand("gc", |sub| {
            sub.about("清理下载缓存（不删除已安装版本）。")
                .mut_arg("all", |arg| {
                    arg.help("删除所有缓存产物，而不只是超过 --days 的")
                })
                .mut_arg("days", |arg| arg.help("只删除超过这么多天的缓存（默认 7）"))
        })
        .mut_subcommand("ui", |sub| {
            sub.about("打开终端 TUI（或渲染一帧非交互快照）。")
                .mut_arg("snapshot", |arg| arg.help("只打印一帧，不启动交互界面"))
                .mut_arg("cols", |arg| arg.help("快照宽度（默认 100）"))
                .mut_arg("rows", |arg| arg.help("快照高度（默认 28）"))
                .mut_arg("down", |arg| arg.help("仅快照：模拟按 N 次 Down"))
                .mut_arg("no_versions", |arg| arg.help("跳过版本探测，只验证布局"))
        });

    command
}

/// 把 clap 渲染好的帮助文本里剩下的英文结构词换成中文。
///
/// `Usage` / `Options` / `Arguments` / `Print help` 由 clap 在 build 时注入，
/// 无法通过 `mut_arg` 修改；这里在打印前统一替换，顺序上先替换长串。
pub fn localize_help_output(text: &str, locale: Locale) -> String {
    if !locale.is_zh() {
        return text.to_string();
    }
    text.replace(
        "Print help (see a summary with '-h')",
        "打印帮助（用 -h 看摘要）",
    )
    .replace(
        "Print help (see more with '--help')",
        "打印帮助（用 --help 看完整说明）",
    )
    .replace("Print help", "打印帮助")
    .replace("Print version", "打印版本")
    .replace("Usage: ", "用法：")
    .replace("Usage:", "用法：")
    .replace("Commands:", "命令:")
    .replace("Arguments:", "参数:")
    .replace("Options:", "选项:")
}

/// 在 clap 解析之前找出本次运行的 locale，决定帮助文本用哪种语言。
///
/// 只扫描 `--locale` / `--home`（含 `--opt=value` 形式），不解析整个命令行；
/// 真正的参数校验仍交给 clap。
pub fn help_locale(args: &[OsString]) -> Locale {
    let explicit = flag_value(args, "--locale");
    let home = flag_value(args, "--home")
        .map(RuntimeHome::at)
        .or_else(|| RuntimeHome::resolve().ok());
    match home {
        Some(home) => Locale::resolve_with_home(explicit.as_deref(), &home),
        None => Locale::resolve(explicit.as_deref()),
    }
}

fn flag_value(args: &[OsString], name: &str) -> Option<String> {
    let inline = format!("{name}=");
    let mut index = 1usize;
    while index < args.len() {
        let text = args[index].to_string_lossy();
        if let Some(value) = text.strip_prefix(&inline) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
        if text == name {
            if let Some(value) = args.get(index + 1) {
                return Some(value.to_string_lossy().to_string());
            }
        }
        index += 1;
    }
    None
}
