//! 命令规范、参数模型与解析入口的统一导出。
//!
//! 这个 crate 可以粗略分成四层：
//! - `spec`: 声明命令长什么样
//! - `parser`: 把声明映射到 clap，并解析回 `CommandContext`
//! - `context`: 执行前的结构化参数快照
//! - `localization`: 对内建命令说明文字做本地化修饰
//!
//! `lib.rs` 自己不放业务逻辑，只负责把这几层作为一个对外清晰的 API 面导出。
pub mod context;
pub mod localization;
pub mod parser;
pub mod spec;

pub use context::{CommandContext, ParsedArgv};
pub use localization::localize_command_specs;
pub use parser::{
    apply_legacy_aliases, build_cli, command_context_from_matches, render_builtin_command,
    BuiltinCommandOutput,
};
pub use spec::{ArgSpec, CommandSpec, Example, OptionSpec, ValueKind};
