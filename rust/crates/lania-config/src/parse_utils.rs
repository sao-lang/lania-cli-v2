//! 配置解析阶段用到的小型工具函数。
//!
//! 这类函数通常不复杂，但价值在于“统一约定”：
//! - 例如把 JSON object 统一转成 `BTreeMap`
//! - 保证后续 normalize / validate / snapshot 构造看到的是稳定的数据形态

use std::collections::BTreeMap;

use crate::{anyhow, Result, Value};

pub(crate) fn as_object_map(value: Value) -> Result<BTreeMap<String, Value>> {
    // serde_json::Value/Object 内部使用 Map（实现细节取决于特性/版本），
    // 这里统一转换为 BTreeMap：按 key 排序后遍历顺序稳定，且返回 owned 数据便于后续加工。
    Ok(value
        .as_object()
        .ok_or_else(|| anyhow!("config field must be an object"))?
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect())
}
