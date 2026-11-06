//! 命令、参数、选项与示例的声明式规范定义。
//!
//! 主要导出：new、with_alias、with_aliases、with_options、with_args、with_examples。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    // `CommandSpec` 是一份纯声明：
    // - 它描述“命令长什么样”
    // - 但不直接持有可执行代码
    //
    // 真正执行时，host runtime 会根据 `handler_id` 去 handler registry 里找实现。
    // 因此可以把它理解成“CLI 协议描述”和“执行实现”之间的桥梁。
    pub name: String,
    pub about: String,
    pub alias: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub args: Vec<ArgSpec>,
    pub options: Vec<OptionSpec>,
    pub examples: Vec<Example>,
    pub subcommands: Vec<CommandSpec>,
    pub handler_id: String,
}

impl CommandSpec {
    pub fn new(
        name: impl Into<String>,
        about: impl Into<String>,
        handler_id: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            about: about.into(),
            alias: None,
            aliases: vec![],
            args: vec![],
            options: vec![],
            examples: vec![],
            subcommands: vec![],
            handler_id: handler_id.into(),
        }
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        // builder 风格 API 的好处是：插件注册命令时可以写出非常接近 DSL 的声明代码。
        self.alias = Some(alias.into());
        self
    }

    pub fn with_aliases<I, S>(mut self, aliases: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.aliases = aliases.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_options(mut self, options: Vec<OptionSpec>) -> Self {
        self.options = options;
        self
    }

    pub fn with_args(mut self, args: Vec<ArgSpec>) -> Self {
        self.args = args;
        self
    }

    pub fn with_examples(mut self, examples: Vec<Example>) -> Self {
        self.examples = examples;
        self
    }

    pub fn with_subcommands(mut self, subcommands: Vec<CommandSpec>) -> Self {
        self.subcommands = subcommands;
        self
    }

    pub fn with_subcommand(mut self, subcommand: CommandSpec) -> Self {
        self.subcommands.push(subcommand);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgSpec {
    // 位置参数（positional args）：
    // 它们不带 `--name` 前缀，解析时依赖命令定义中的顺序与名字。
    pub name: String,
    pub required: bool,
    pub multiple: bool,
    pub help: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionSpec {
    // 选项参数（flags/options）：
    // - `long/short` 决定 CLI 形态
    // - `value_kind/default_value/choices` 决定 clap 该如何解析
    // - `negatable` 允许生成 `--no-xxx`
    pub long: String,
    pub short: Option<char>,
    pub help: String,
    pub value_kind: ValueKind,
    pub default_value: Option<String>,
    pub choices: Vec<String>,
    pub negatable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Example {
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    // 这里不是完整 JSON 类型系统，而是“CLI 输入层需要区分的最小集合”。
    // 更复杂的结构一般会在进入 handler/workflow 后再解释。
    Bool,
    String,
    Number,
    OptionalString,
}
