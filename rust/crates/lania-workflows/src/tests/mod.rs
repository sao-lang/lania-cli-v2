//! workflow 测试模块入口。
//!
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use lania_config::{
    ReleaseGitConfig, ReleasePostCheckConfig, ReleaseProfile, ReleaseStepConfig,
    ReleaseVerifyConfig, ReleaseVersioningConfig,
};
use lania_exec::ExecService;
use lania_fs::FsService;
use lania_git::GitService;
use lania_node_bridge::NodeBridgeClient;
use lania_pm::{PackageManager, PackageManagerService};
use lania_progress::ProgressService;
use lania_prompt::PromptService;
use lania_task::TaskService;
use serde_json::Value;

use super::*;
use crate::release::{
    execute_release_plan, merge_release_state, read_release_state, release_state_from_plan,
    ReleasePlan,
};

mod common;
mod create_add;
mod generate_api;
mod generate_module;
mod release;
mod sync;

use common::*;
