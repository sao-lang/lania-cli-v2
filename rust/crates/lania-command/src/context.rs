//! 命令执行上下文、解析后参数与 trace 信息的数据结构。
//!
//! 主要导出：ParsedArgv、CommandContext。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedArgv {
    // 为什么把 args 和 options 分开？
    // - 位置参数和命名选项在 CLI 语义上本来就是两类输入
    // - 分开存后，上层既能保留“用户是怎么传的”这层语义，又不必依赖 clap 内部结构
    // - 两边统一用 JSON Value，方便后续交给动态命令 / hooks / bridge 继续透传
    pub args: BTreeMap<String, serde_json::Value>,
    pub options: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContext {
    // `CommandContext` 是“解析完成、准备执行”的命令快照：
    // - `cwd`：命令在哪个目录语义下执行
    // - `argv`：解析后的参数
    // - `handler_id`：最终应该调用哪个 handler
    // - `trace_id`：贯穿日志 / hooks / execution 的关联 id
    pub cwd: String,
    pub argv: ParsedArgv,
    pub handler_id: String,
    pub trace_id: String,
}
