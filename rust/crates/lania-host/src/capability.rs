//! 宿主 capability 注册表，以及按名称解析能力的基础类型。
//!
//! 主要导出：new、CapabilitySnapshot、CapabilityContainer、CapabilityName、CapabilityResolver、CapabilityRegistrar。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityName {
    Logger,
    Config,
    Prompt,
    Exec,
    Fs,
    Git,
    PackageManager,
    Task,
    Progress,
    Compiler,
    Lint,
    Template,
    NodeBridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySnapshot {
    pub name: CapabilityName,
    pub provider: String,
    pub description: String,
}

pub trait CapabilityResolver {
    // 这里只暴露 capability 的“声明快照”，而不是具体服务实例。
    // 这说明 capability 系统的职责更偏“可发现性/可观测性”，
    // 真正的执行能力仍由 `CommandExecutionContext` 里的具体服务字段提供。
    fn get(&self, name: CapabilityName) -> Option<&CapabilitySnapshot>;
    fn all(&self) -> Vec<CapabilitySnapshot>;
}

pub trait CapabilityRegistrar {
    fn register(&mut self, snapshot: CapabilitySnapshot);
}

#[derive(Debug, Default)]
pub struct CapabilityContainer {
    // 用 `BTreeMap` 而不是 `HashMap`，主要是为了让遍历输出稳定有序，
    // 这样 summary/debug/test 快照更容易比较。
    items: BTreeMap<CapabilityName, CapabilitySnapshot>,
}

impl CapabilityContainer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CapabilityRegistrar for CapabilityContainer {
    fn register(&mut self, snapshot: CapabilitySnapshot) {
        // 后注册同名 capability 会覆盖旧值。
        // 这让宿主可以先注册内建能力，再允许后续阶段用更具体的 provider 描述覆盖元数据。
        self.items.insert(snapshot.name, snapshot);
    }
}

impl CapabilityResolver for CapabilityContainer {
    fn get(&self, name: CapabilityName) -> Option<&CapabilitySnapshot> {
        self.items.get(&name)
    }

    fn all(&self) -> Vec<CapabilitySnapshot> {
        self.items.values().cloned().collect()
    }
}
