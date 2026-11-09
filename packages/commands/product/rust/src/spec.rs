//! `command-product` 的命令规格（CommandSpec）定义。
//!
//! 说明：
//! - 这里仅定义 CLI 的“形状”（命令/子命令/参数/选项/示例/帮助文案），不做任何运行逻辑。
//! - 原 5 个包的 spec 构造函数迁到这里：`DevCommandPlugin::spec()` -> `dev_spec()`，
//!   build 的 spec 逻辑迁为 `build_spec()` / `product_build_spec()` 等。
//! - 原 5 个根 spec 的 `name` 与 handler_id 保持不变，只改挂载位置与示例里的命令拼写
//!   （`lan dev ...` -> `lan workspace dev ...`）。
//! - 各子命令不再携带旧包各自的 config 子树（随旧包消亡），workspace 的 config
//!   由根 spec 统一挂载（见 `crate::config`）。
//!
//! 约定：
//! - product 开发态命令统一走 `lan product dev`，product 子树仍挂在共享 product 根下。

use lania_command::{ArgSpec, CommandSpec, Example, OptionSpec, ValueKind};

use crate::{
    DOCTOR_PRODUCT_HANDLER_ID, INSPECT_PRODUCT_HANDLER_ID, PACK_PRODUCT_HANDLER_ID,
    PRODUCT_BUILD_HANDLER_ID, PRODUCT_DEV_HANDLER_ID, PUBLISH_PRODUCT_HANDLER_ID,
};

// workspace root + 5 个降级子命令 + product 子树 + template 的 spec 构造。
//
// 这里仅描述“用户能怎么用”：
// - 有哪些命令/子命令
// - 支持哪些参数/选项
// - help 里展示哪些示例
//
// 运行时行为不在这里实现：
// - 标准 `dev` 的 bridge 请求见 `dev.rs`
// - `product dev` 的 once/watch 运行时见 `watch.rs`
// - handler 分发见 `handlers.rs` / `dev.rs` / `build.rs` / `lint.rs` / `create.rs` / `add.rs`

/// workspace 根命令：本身不可执行（bare 时由 root handler 给出提示），
/// 5 个原顶级命令与 template/config 挂在其下。

// ── product 子命令 spec（从 workspace 包迁入，handler_id 冻结值不变）──

pub(crate) fn product_dev_spec() -> CommandSpec {
    // product dev 是“本地转发执行”模式：
    // - 重新执行当前 `lan` 二进制（re-exec），避免在进程内重写一套命令分发
    // - 注入 `LANIA_PRODUCT_ROOT` + `LANIA_RUNTIME_MODE=development`
    // - 透传用户的 product 子命令与参数
    CommandSpec::new(
        "dev",
        "Run a local product in development mode",
        PRODUCT_DEV_HANDLER_ID,
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

pub(crate) fn product_build_spec() -> CommandSpec {
    CommandSpec::new(
        "build",
        "Build a minimal product snapshot for packaging",
        PRODUCT_BUILD_HANDLER_ID,
    )
    .with_options(vec![
        product_path_option(),
        string_option(
            "output-dir",
            Some('o'),
            "Override the product build output directory",
            Some(".lania/build/product"),
        ),
        clean_option("Clean the product build output directory before writing"),
    ])
    .with_examples(vec![Example {
        command: "lan product build --output-dir .lania/build/product".into(),
        description: "Create a minimal product snapshot for later pack/publish steps".into(),
    }])
}

pub(crate) fn product_pack_spec() -> CommandSpec {
    CommandSpec::new(
        "pack",
        "Assemble a minimal install-root layout from a built product snapshot",
        PACK_PRODUCT_HANDLER_ID,
    )
    .with_options(vec![
        product_path_option(),
        string_option(
            "build-dir",
            None,
            "Product build directory created by `lan product build`",
            Some(".lania/build/product"),
        ),
        string_option(
            "output-dir",
            Some('o'),
            "Override the product pack output directory",
            Some(".lania/pack/product/install-root"),
        ),
        clean_option("Clean the product pack output directory before writing"),
    ])
    .with_examples(vec![Example {
        command: "lan product pack --build-dir .lania/build/product".into(),
        description: "Create a local install-root layout for validation before publish".into(),
    }])
}

pub(crate) fn product_publish_spec() -> CommandSpec {
    CommandSpec::new(
        "publish",
        "Assemble a publish-ready npm package layout from a packed product",
        PUBLISH_PRODUCT_HANDLER_ID,
    )
    .with_options(vec![
        product_path_option(),
        string_option(
            "pack-dir",
            None,
            "Product pack directory created by `lan product pack`",
            Some(".lania/pack/product/install-root"),
        ),
        string_option(
            "output-dir",
            Some('o'),
            "Override the product publish output directory",
            Some(".lania/publish/product/npm-package"),
        ),
        string_option("dist-tag", Some('t'), "Registry dist-tag for publish planning", None),
        string_option("channel", Some('C'), "Release channel for publish planning", None),
        string_option(
            "registry",
            None,
            "Override npm registry for publish planning/execution",
            None,
        ),
        string_option(
            "platform-binaries-dir",
            None,
            "Directory containing staged per-platform `lania-cli` binaries",
            None,
        ),
        string_option(
            "platform-binary-paths",
            None,
            "JSON object mapping platform keys to injected binary paths",
            None,
        ),
        OptionSpec {
            long: "execute".into(),
            short: None,
            help: "Execute npm publish steps from the generated publish manifest".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "dry-run".into(),
            short: None,
            help: "Execute publish steps with npm --dry-run".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "yes".into(),
            short: Some('y'),
            help: "Confirm real npm publish execution when not using --dry-run".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "resume".into(),
            short: None,
            help: "Resume publish execution from completed manifest steps".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        string_option("otp", None, "Pass npm OTP to publish execution", None),
        string_option("npm-bin", None, "Override npm executable for publish execution", None),
        string_option(
            "max-retries",
            None,
            "Retry count for transient npm publish failures",
            None,
        ),
        string_option(
            "retry-delay-ms",
            None,
            "Delay between publish retries in milliseconds",
            None,
        ),
        OptionSpec {
            long: "rollback-on-failure".into(),
            short: None,
            help: "Execute npm unpublish commands after partial publish failure".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        clean_option("Clean the product publish output directory before writing"),
    ])
    .with_examples(vec![
        Example {
            command: "lan product publish --pack-dir .lania/pack/product/install-root".into(),
            description:
                "Create a publish-ready npm package artifact without pushing to a registry"
                    .into(),
        },
        Example {
            command: "lan product publish --dist-tag next --channel next".into(),
            description: "Emit a registry-ready publish manifest for the next channel".into(),
        },
        Example {
            command:
                "lan product publish --platform-binaries-dir /tmp/lania-cli-platforms".into(),
            description:
                "Auto-discover platform binaries from a staging root instead of listing each path"
                    .into(),
        },
        Example {
            command:
                "lan product publish --platform-binary-paths '{\"linux-x64\":\"/tmp/lania-cli-linux-x64\"}'"
                    .into(),
            description: "Inject per-platform binary paths for non-host bundle staging".into(),
        },
        Example {
            command: "lan product publish --execute --dry-run".into(),
            description: "Execute publish-manifest steps through npm publish --dry-run".into(),
        },
        Example {
            command: "lan product publish --execute --dry-run --npm-bin /tmp/fake-npm".into(),
            description: "Execute publish-manifest steps with a custom npm binary".into(),
        },
        Example {
            command: "lan product publish --execute --yes --registry http://localhost:4873"
                .into(),
            description: "Rehearse a real publish flow against a local test registry".into(),
        },
    ])
}

pub(crate) fn product_inspect_spec() -> CommandSpec {
    CommandSpec::new(
        "inspect",
        "Inspect product config, schema discovery, and local artifacts",
        INSPECT_PRODUCT_HANDLER_ID,
    )
    .with_options(vec![product_path_option(), inspect_compat_option()])
    .with_examples(vec![
        Example {
            command: "lan product inspect --path ./products/acme-cli".into(),
            description: "Inspect the current product config, schema roots, and local build state"
                .into(),
        },
        Example {
            command: "lan product inspect --path ./products/acme-cli --compat".into(),
            description: "Inspect product compatibility snapshot (framework/protocol/product)"
                .into(),
        },
    ])
}

pub(crate) fn product_doctor_spec() -> CommandSpec {
    CommandSpec::new(
        "doctor",
        "Run product doctor diagnostics, including compatibility checks",
        DOCTOR_PRODUCT_HANDLER_ID,
    )
    .with_options(vec![product_path_option()])
    .with_examples(vec![Example {
        command: "lan product doctor --path ./products/acme-cli".into(),
        description:
            "Run product diagnostics with compatibility, artifact, and schema checks".into(),
    }])
}

/// bin 形态的完整命令树：product root + 6 个子命令（独立二进制的 clap 依据）。
pub fn product_full_spec() -> CommandSpec {
    let mut root = lania_host::product_root_spec();
    root.subcommands.push(product_dev_spec());
    root.subcommands.push(product_build_spec());
    root.subcommands.push(product_pack_spec());
    root.subcommands.push(product_publish_spec());
    root.subcommands.push(product_inspect_spec());
    root.subcommands.push(product_doctor_spec());
    root
}

// ── spec 辅助（从 workspace spec.rs 复制）──

fn product_path_option() -> OptionSpec {
    string_option(
        "path",
        None,
        "Product root path (defaults to current working directory)",
        None,
    )
}


fn inspect_compat_option() -> OptionSpec {
    OptionSpec {
        long: "compat".into(),
        short: None,
        help: "Include compatibility snapshot and write compat-report.json (experimental)".into(),
        value_kind: ValueKind::Bool,
        default_value: None,
        choices: vec![],
        negatable: false,
    }
}


fn clean_option(help: &str) -> OptionSpec {
    OptionSpec {
        long: "clean".into(),
        short: None,
        help: help.into(),
        value_kind: ValueKind::Bool,
        default_value: None,
        choices: vec![],
        negatable: true,
    }
}


fn string_option(
    long: &str,
    short: Option<char>,
    help: &str,
    default_value: Option<&str>,
) -> OptionSpec {
    OptionSpec {
        long: long.into(),
        short,
        help: help.into(),
        value_kind: ValueKind::String,
        default_value: default_value.map(Into::into),
        choices: vec![],
        negatable: false,
    }
}

