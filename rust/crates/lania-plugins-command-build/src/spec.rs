//! `command-build` 的命令规格（CommandSpec）定义。
//!
//! 为什么要拆出来：
//! - 这类文件很容易“越写越长”（options/examples/subcommands），和运行逻辑混在一起会难维护。
//! - 运行逻辑/桥接请求映射属于别的关注点，放在 `request.rs` / `handlers.rs`。
//!
//! 约定：
//! - 这里仅定义“CLI 长什么样”（命令、参数、help、示例），不做任何执行或 IO。

use lania_command::{CommandSpec, Example, OptionSpec, ValueKind};

use crate::{
    BuildCommandPlugin, DOCTOR_PRODUCT_HANDLER_ID, HANDLER_ID, INSPECT_PRODUCT_HANDLER_ID,
    PACK_PRODUCT_HANDLER_ID, PRODUCT_HANDLER_ID, PUBLISH_PRODUCT_HANDLER_ID,
};

// build/pack/publish/inspect/doctor 及其 product alias 的 spec 构造。
// 保持 crate root (`lib.rs`) 更像“插件入口”，而不是 spec 大杂烩。

impl BuildCommandPlugin {
    pub fn product_build_spec() -> CommandSpec {
        // `lan product build`：产出 product snapshot（后续 pack/publish 的输入）。
        let mut spec = Self::product_spec();
        spec.name = "build".into();
        spec.examples = vec![Example {
            command: "lan product build --output-dir .lania/build/product".into(),
            description: "Create a minimal product snapshot for later pack/publish steps".into(),
        }];
        spec
    }

    pub fn spec() -> CommandSpec {
        // 标准 `lan build`：走 compiler build workflow（bridge 侧实现）。
        CommandSpec::new("build", "Run the project build workflow", HANDLER_ID)
            .with_options(vec![
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
                    long: "watch".into(),
                    short: Some('w'),
                    help: "Keep the builder running in watch mode".into(),
                    value_kind: ValueKind::Bool,
                    default_value: None,
                    choices: vec![],
                    negatable: true,
                },
                OptionSpec {
                    long: "mode".into(),
                    short: Some('m'),
                    help: "Forward a named build mode to the underlying compiler".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec!["development".into(), "production".into()],
                    negatable: false,
                },
                OptionSpec {
                    long: "output-dir".into(),
                    short: Some('o'),
                    help: "Override the build output directory".into(),
                    value_kind: ValueKind::String,
                    default_value: None,
                    choices: vec![],
                    negatable: false,
                },
            ])
            .with_examples(vec![
                Example {
                    command: "lan build".into(),
                    description: "Create a production build".into(),
                },
                Example {
                    command: "lan build --watch --mode development".into(),
                    description: "Run the build workflow in watch mode".into(),
                },
            ])
    }

    pub fn product_spec() -> CommandSpec {
        // product build 的底层 spec，会被重命名后挂到 `lan product build`。
        CommandSpec::new(
            "product",
            "Build a minimal product snapshot for packaging",
            PRODUCT_HANDLER_ID,
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

    pub fn product_pack_spec() -> CommandSpec {
        // `lan product pack`：把 build 产物组装成 install-root 布局。
        let mut spec = Self::pack_product_spec();
        spec.name = "pack".into();
        spec.examples = vec![Example {
            command: "lan product pack --build-dir .lania/build/product".into(),
            description: "Create a local install-root layout for validation before publish".into(),
        }];
        spec
    }

    pub fn pack_product_spec() -> CommandSpec {
        CommandSpec::new(
            "product",
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

    pub fn product_publish_spec() -> CommandSpec {
        // `lan product publish`：组装/执行面向 npm 的发布流程。
        let mut spec = Self::publish_product_spec();
        spec.name = "publish".into();
        spec.examples = vec![
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
        ];
        spec
    }

    pub fn publish_product_spec() -> CommandSpec {
        CommandSpec::new(
            "product",
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

    pub fn product_inspect_spec() -> CommandSpec {
        let mut spec = Self::inspect_product_spec();
        spec.name = "inspect".into();
        spec.examples = vec![
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
        ];
        spec
    }

    pub fn inspect_product_spec() -> CommandSpec {
        CommandSpec::new(
            "product",
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
                description:
                    "Inspect product compatibility snapshot (framework/protocol/product)".into(),
            },
        ])
    }

    pub fn product_doctor_spec() -> CommandSpec {
        let mut spec = Self::doctor_product_spec();
        spec.name = "doctor".into();
        spec.examples = vec![Example {
            command: "lan product doctor --path ./products/acme-cli".into(),
            description:
                "Run product diagnostics with compatibility, artifact, and schema checks".into(),
        }];
        spec
    }

    pub fn doctor_product_spec() -> CommandSpec {
        CommandSpec::new(
            "product",
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
}

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
