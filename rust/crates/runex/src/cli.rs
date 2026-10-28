//! 命令行定义：保持 nvm 的肌肉记忆（install / use / ls / ls-remote），
//! 同时用 `<tool>` 参数把「多运行时」显式化。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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

    /// Delete a lania-managed version.
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

    /// Diagnose pointers, shims and PATH wiring.
    Doctor,
}
