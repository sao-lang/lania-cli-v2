use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

// Phase 3 CLI e2e 入口。
// 保留单个 integration test 入口文件名，确保 `cargo test --test phase3_e2e`
// 的调用方式不变，同时把具体场景拆到 `phase3_e2e/` 子模块中维护。

#[path = "phase3_e2e/build_and_lint.rs"]
mod build_and_lint;
#[path = "phase3_e2e/common.rs"]
mod common;
#[path = "phase3_e2e/create_and_add.rs"]
mod create_and_add;
#[path = "phase3_e2e/distribution_and_dynamic.rs"]
mod distribution_and_dynamic;
#[path = "phase3_e2e/generate_workflows.rs"]
mod generate_workflows;
#[path = "phase3_e2e/help_and_config.rs"]
mod help_and_config;
#[path = "phase3_e2e/sync_and_release.rs"]
mod sync_and_release;
#[path = "phase3_e2e/template_commands.rs"]
mod template_commands;
