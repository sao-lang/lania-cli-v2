//! `command-dev` 的命令规格（CommandSpec）定义。
//!
//! 说明：
//! - 这里仅定义 CLI 的“形状”（命令/子命令/参数/选项/示例/帮助文案）。
//! - 不做任何运行逻辑（不调用 bridge、不做 watch、不做 IO），运行逻辑分别在：
//!   - `request.rs`：`lan dev` -> bridge request 映射
//! - `watch.rs`：`lan product dev` 的 once/watch 运行时
//!   - `handlers.rs`：host handler 入口（把上面两类逻辑串起来）
//!
//! 约定：
//! - product 开发态命令统一走 `lan product dev`，不再保留 `lan dev product`。

use lania_command::{ArgSpec, CommandSpec, Example, OptionSpec, ValueKind};

use crate::{DevCommandPlugin, HANDLER_ID, PRODUCT_HANDLER_ID, PRODUCT_ROOT_HANDLER_ID};

// `dev` 与 `product dev` 的命令规格构造。
//
// 这里仅描述“用户能怎么用”：
// - 有哪些命令/子命令
// - 支持哪些参数/选项
// - help 里展示哪些示例
//
// 运行时行为不在这里实现：
// - 标准 `dev` 的 bridge 请求见 `request.rs`
// - `product dev` 的 once/watch 运行时见 `watch.rs`
// - handler 分发见 `handlers.rs`

impl DevCommandPlugin {
    pub fn product_root_spec() -> CommandSpec {
        // `product` 根命令：用来分组 product 生命周期命令。
        // 它本身不可执行；缺少子命令时由 `handlers::ProductRootCommandHandler` 给出提示。
        CommandSpec::new(
            "product",
            "Product-oriented CLI workflows and distribution commands",
            PRODUCT_ROOT_HANDLER_ID,
        )
        .with_examples(vec![
            Example {
                command: "lan product generate --name \"Acme CLI\" --binary-name acme".into(),
                description: "Scaffold a new CLI product workspace".into(),
            },
            Example {
                command: "lan product dev hello --path ./products/acme-cli".into(),
                description: "Run a local product command in development mode".into(),
            },
            Example {
                command: "lan product inspect --path ./products/acme-cli --compat".into(),
                description: "Inspect product compatibility and local distribution state".into(),
            },
        ])
    }

    pub fn spec() -> CommandSpec {
        // 标准开发流程（走 node-bridge）。这是默认的 `lan dev`。
        CommandSpec::new("dev", "Start the project development workflow", HANDLER_ID)
            .with_options(vec![
                OptionSpec {
                    long: "port".into(),
                    short: Some('p'),
                    help: "Override the development server port".into(),
                    value_kind: ValueKind::Number,
                    default_value: Some("8089".into()),
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "config".into(),
                    short: None,
                    help: "Legacy config path (accepted for compatibility; may be ignored)".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "path".into(),
                    short: None,
                    help: "Legacy project path (accepted for compatibility)".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "host".into(),
                    short: Some('H'),
                    help: "Override the development server host".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
                OptionSpec {
                    long: "hmr".into(),
                    short: None,
                    help: "Enable or disable HMR for supported dev servers".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: true,
                },
                OptionSpec {
                    long: "open".into(),
                    short: Some('o'),
                    help: "Open the browser when the dev server becomes ready".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: true,
                },
                OptionSpec {
                    long: "mode".into(),
                    short: Some('m'),
                    help: "Forward a named dev mode to the underlying compiler".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
            ])
            .with_examples(vec![
                Example {
                    command: "lan dev".into(),
                    description: "Run the default development workflow".into(),
                },
                Example {
                    command: "lan dev --port 3001 --open --mode development".into(),
                    description: "Run the development workflow on a custom port".into(),
                },
            ])
    }

    pub fn product_spec() -> CommandSpec {
        // product dev 是“本地转发执行”模式：
        // - 重新执行当前 `lan` 二进制（re-exec），避免在进程内重写一套命令分发
        // - 注入 `LANIA_PRODUCT_ROOT` + `LANIA_RUNTIME_MODE=development`
        // - 透传用户的 product 子命令与参数
        CommandSpec::new(
            "product",
            "Run a local product in development mode",
            PRODUCT_HANDLER_ID,
        )
        .with_args(vec![ArgSpec {
            name: "args".into(),
            required: false,
            multiple: true,
            help: "Product command and arguments".into(),
        }])
        .with_options(vec![
            OptionSpec {
                long: "path".into(),
                short: None,
                help: "Product root path (defaults to current working directory)".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            },
            OptionSpec {
                long: "watch".into(),
                short: Some('w'),
                help: "Watch product files and restart the forwarded command on change".into(),
                value_kind: ValueKind::Bool,
                default_value: None,
                choices: vec![],
                negatable: true,
            },
            OptionSpec {
                long: "poll-interval-ms".into(),
                short: None,
                help: "Polling interval in milliseconds for product watch mode".into(),
                value_kind: ValueKind::Number,
                default_value: Some("500".into()),
                choices: vec![],
                negatable: false,
            },
        ])
        .with_examples(vec![Example {
            command: "lan product dev --watch ops hello --path ./products/acme-cli".into(),
            description: "Re-run a product command when local product files change".into(),
        }])
    }

    pub fn product_dev_spec() -> CommandSpec {
        // alias：让用户可以写 `lan product dev ...`（通过 mount 到 product 根命令下面）。
        let mut spec = Self::product_spec();
        spec.name = "dev".into();
        spec.about = "Run a local product in development mode".into();
        spec.examples = vec![Example {
            command: "lan product dev --watch ops hello --path ./products/acme-cli".into(),
            description: "Re-run a product command when local product files change".into(),
        }];
        spec
    }
}
