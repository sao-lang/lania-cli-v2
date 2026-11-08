//! `command-workspace` 的命令规格（CommandSpec）定义。
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
use lania_workflows::declared_template_names;

use crate::{
    ADD_HANDLER_ID, BUILD_HANDLER_ID, CREATE_HANDLER_ID, DEV_HANDLER_ID, DOCTOR_PRODUCT_HANDLER_ID,
    HANDLER_ID, INSPECT_PRODUCT_HANDLER_ID, LINT_CHECK_HANDLER_ID, LINT_FIX_HANDLER_ID,
    LINT_HANDLER_ID, PACK_PRODUCT_HANDLER_ID, PRODUCT_BUILD_HANDLER_ID, PRODUCT_DEV_HANDLER_ID,
    PUBLISH_PRODUCT_HANDLER_ID,
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
pub(crate) fn workspace_root_spec() -> CommandSpec {
    CommandSpec::new(
        "workspace",
        "Workspace development workflows (dev/build/lint/create/add)",
        HANDLER_ID,
    )
    .with_subcommand(crate::config::config_spec())
    .with_examples(vec![
        Example {
            command: "lan workspace dev".into(),
            description: "Run the default development workflow".into(),
        },
        Example {
            command: "lan workspace build".into(),
            description: "Create a production build".into(),
        },
        Example {
            command: "lan workspace lint".into(),
            description: "Run lint checks".into(),
        },
        Example {
            command: "lan workspace create --name demo-app".into(),
            description: "Create a project from the default built-in template".into(),
        },
        Example {
            command: "lan workspace add --list".into(),
            description: "Show generators available for the detected framework".into(),
        },
    ])
}

// ─────────────────────────────────────────────────────────────
// dev（原 packages/commands/dev）
// ─────────────────────────────────────────────────────────────

/// 标准 `lan workspace dev`（走 node-bridge）。原 dev 根 spec，name/handler_id 不变。
/// bin 形态的完整命令树：workspace root + 全部子命令。
///
/// 宿主形态走 registry 的 `mount_subcommand`（逐个挂载、幂等）；
/// 独立二进制的 clap 由本函数一次性提供——`spec()` 必须返回完整树，
/// 否则 `lan workspace dev` 等转发 argv 会 clap 解析失败。
pub(crate) fn workspace_full_spec() -> CommandSpec {
    let mut root = workspace_root_spec();
    root.subcommands.push(dev_spec());
    root.subcommands.push(build_spec());
    root.subcommands.push(lint_spec());
    root.subcommands.push(create_spec());
    root.subcommands.push(add_spec());
    root.subcommands.push(crate::create_template::TemplateCommandSpec::spec());
    root.subcommands.push(crate::config::config_spec());
    let mut product = lania_host::product_root_spec();
    product.subcommands.push(product_dev_spec());
    product.subcommands.push(product_build_spec());
    product.subcommands.push(product_pack_spec());
    product.subcommands.push(product_publish_spec());
    product.subcommands.push(product_inspect_spec());
    product.subcommands.push(product_doctor_spec());
    root.subcommands.push(product);
    root
}

pub(crate) fn dev_spec() -> CommandSpec {
    CommandSpec::new(
        "dev",
        "Start the project development workflow",
        DEV_HANDLER_ID,
    )
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
        OptionSpec {
            long: "runtime".into(),
            short: None,
            help: "Override runtime (web|python|go|rust|dart)".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![
                "web".into(),
                "python".into(),
                "go".into(),
                "rust".into(),
                "dart".into(),
            ],
            negatable: false,
        },
        OptionSpec {
            long: "quiet-app".into(),
            short: None,
            help: "Hide application logs (channel: app) after the dev server is ready".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
    ])
    .with_examples(vec![
        Example {
            command: "lan workspace dev".into(),
            description: "Run the default development workflow".into(),
        },
        Example {
            command: "lan workspace dev --port 3001 --open --mode development".into(),
            description: "Run the development workflow on a custom port".into(),
        },
        Example {
            command: "lan workspace dev --quiet-app".into(),
            description: "Run dev without streaming the app's own runtime logs".into(),
        },
    ])
}

/// `lan product dev`：product 开发态命令（挂在共享 product 根下，拼写不变）。
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

// ─────────────────────────────────────────────────────────────
// build/pack/publish/inspect/doctor（原 packages/commands/build）
// ─────────────────────────────────────────────────────────────

/// 标准 `lan workspace build`：走 compiler build workflow（bridge 侧实现）。
pub(crate) fn build_spec() -> CommandSpec {
    CommandSpec::new(
        "build",
        "Run the project build workflow",
        BUILD_HANDLER_ID,
    )
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
        OptionSpec {
            long: "runtime".into(),
            short: None,
            help: "Override runtime (web|python|go|rust|dart)".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![
                "web".into(),
                "python".into(),
                "go".into(),
                "rust".into(),
                "dart".into(),
            ],
            negatable: false,
        },
    ])
    .with_examples(vec![
        Example {
            command: "lan workspace build".into(),
            description: "Create a production build".into(),
        },
        Example {
            command: "lan workspace build --watch --mode development".into(),
            description: "Run the build workflow in watch mode".into(),
        },
    ])
}

/// `lan product build`：产出 product snapshot（后续 pack/publish 的输入）。
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

/// `lan product pack`：把 build 产物组装成 install-root 布局。
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

/// `lan product publish`：组装/执行面向 npm 的发布流程。
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

/// `lan product inspect`：检查 product config、schema 发现与本地产物。
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

/// `lan product doctor`：product 诊断（含兼容性检查）。
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

// ─────────────────────────────────────────────────────────────
// lint（原 packages/commands/lint）
// ─────────────────────────────────────────────────────────────

/// 标准 `lan workspace lint`：check/fix 子命令保留挂在 lint_spec 下。
pub(crate) fn lint_spec() -> CommandSpec {
    CommandSpec::new("lint", "Run project lint checks", LINT_HANDLER_ID)
        .with_options(lint_shared_options())
        .with_subcommands(vec![lint_check_spec(), lint_fix_spec()])
        .with_examples(vec![
            Example {
                command: "lan workspace lint".into(),
                description: "Run lint checks".into(),
            },
            Example {
                command: "lan workspace lint fix --concurrency 2".into(),
                description: "Run lint in fix mode with a custom worker limit".into(),
            },
        ])
}

pub(crate) fn lint_check_spec() -> CommandSpec {
    CommandSpec::new("check", "Run lint in check mode", LINT_CHECK_HANDLER_ID)
        .with_options(lint_shared_options())
}

pub(crate) fn lint_fix_spec() -> CommandSpec {
    CommandSpec::new("fix", "Run lint in fix mode", LINT_FIX_HANDLER_ID)
        .with_options(lint_shared_options())
}

fn lint_shared_options() -> Vec<OptionSpec> {
    vec![
        OptionSpec {
            long: "fix".into(),
            short: Some('f'),
            help: "Run lint in fix mode".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        // Legacy compatibility: allow selecting adaptors by name (comma-separated).
        OptionSpec {
            long: "linters".into(),
            short: None,
            help: "Comma-separated linter adaptors to run (eslint,oxlint,prettier,oxfmt,stylelint,textlint,ruff,black,mypy,flake8,golangci-lint,go-vet,clippy,dart-analyze)".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "concurrency".into(),
            short: Some('c'),
            help: "Run lint adaptors with a bounded concurrency level".into(),
            value_kind: ValueKind::Number,
            default_value: Some("4".into()),
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "grouped-output".into(),
            short: None,
            help: "Print lint results grouped by adaptor".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
    ]
}

// ─────────────────────────────────────────────────────────────
// create（原 packages/commands/create）
// ─────────────────────────────────────────────────────────────

/// 只有 Node bridge 模板运行时才能渲染的模板；与 Rust 兜底清单合并成 `--template` 候选。
const BRIDGE_TEMPLATES: &[&str] = &["toolkit", "toolkit-monorepo", "nestjs", "express"];

/// 标准 `lan workspace create`。template 子命令上移到 workspace 根
/// （`lan workspace template`），不再挂在 create 之下。
pub(crate) fn create_spec() -> CommandSpec {
    CommandSpec::new(
        "create",
        "Create a new project from a template",
        CREATE_HANDLER_ID,
    )
    .with_args(vec![ArgSpec {
        name: "path".into(),
        required: false,
        multiple: false,
        help: "Project path (use \".\" for current directory)".into(),
    }])
    .with_options(vec![
        OptionSpec {
            long: "name".into(),
            short: Some('n'),
            help: "Project name".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "template".into(),
            short: Some('t'),
            help: "Template name".into(),
            value_kind: ValueKind::String,
            default_value: None,
            // 候选由「bridge 专属模板 + Rust 兜底模板」合并而成：
            // 前者需要模板运行时，后者在没有 bridge 时仍可渲染。
            // 兜底清单只有 `lania_workflows::declared_template_names` 一处定义。
            choices: BRIDGE_TEMPLATES
                .iter()
                .map(|name| (*name).to_string())
                .chain(declared_template_names())
                .collect(),
            negatable: false,
        },
        OptionSpec {
            long: "package-manager".into(),
            short: Some('p'),
            help: "Package manager".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![
                "npm".into(),
                "pnpm".into(),
                "yarn".into(),
                "bun".into(),
                "pip".into(),
                "uv".into(),
                "poetry".into(),
                "go".into(),
                "cargo".into(),
                "dart".into(),
                "pub".into(),
            ],
            negatable: false,
        },
        // Legacy compatibility: create in a child directory (maps to v2 path semantics).
        OptionSpec {
            long: "directory".into(),
            short: None,
            help: "Legacy alias for project path (create inside the given directory)".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "git".into(),
            short: Some('g'),
            help: "Initialize git repository".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        // Legacy compatibility: invert of --git (matches legacy create behavior).
        OptionSpec {
            long: "skip-git".into(),
            short: None,
            help: "Legacy alias for --no-git".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        // Legacy compatibility: skip package manager steps.
        OptionSpec {
            long: "skip-install".into(),
            short: None,
            help: "Skip package manager init/install steps".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        // Legacy compatibility: allow templates to branch by language (e.g. js/ts).
        OptionSpec {
            long: "language".into(),
            short: None,
            help: "Preferred project language (forwarded to template context)".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "lint-tools".into(),
            short: None,
            help: "Comma-separated lint tools forwarded to the template".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "dry-run".into(),
            short: Some('d'),
            help: "Plan file writes and install commands without executing".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "preview".into(),
            short: Some('P'),
            help: "Preview rendered template files and imply dry-run".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
    ])
    .with_examples(vec![
        Example {
            command: "lan workspace create --name demo-app".into(),
            description: "Create a project from the default built-in template".into(),
        },
        Example {
            command: "lan workspace create .".into(),
            description: "Create a project in current directory".into(),
        },
        Example {
            command: "lan workspace create --template toolkit --package-manager pnpm".into(),
            description: "Create a toolkit template with pnpm".into(),
        },
        Example {
            command:
                "lan workspace create --name svc --template go-service --package-manager go"
                    .into(),
            description: "Scaffold a Go service with the Go toolchain".into(),
        },
        Example {
            command: "lan workspace create --template spa-vue --preview".into(),
            description: "Preview the rendered template files without writing".into(),
        },
    ])
}

// ─────────────────────────────────────────────────────────────
// add（原 packages/commands/add）
// ─────────────────────────────────────────────────────────────

/// 标准 `lan workspace add`：往既有 workspace 里添加生成内容。
pub(crate) fn add_spec() -> CommandSpec {
    CommandSpec::new(
        "add",
        "Add generated content into an existing workspace",
        ADD_HANDLER_ID,
    )
    .with_args(vec![
        ArgSpec {
            name: "generator".into(),
            required: false,
            multiple: false,
            help: "Generator name, e.g. `resource` (nestjs/gin/fastapi/axum) or `component`"
                .into(),
        },
        // clap 要求 option 名与位置参数名唯一，而 `--name` 已经占用 `name`，
        // 因此位置参数用 `generator-name`，在 `build_input` 里映射回名称。
        ArgSpec {
            name: "generator-name".into(),
            required: false,
            multiple: false,
            help: "Base name used to derive class names and file paths".into(),
        },
    ])
    .with_options(vec![
        OptionSpec {
            long: "name".into(),
            short: Some('n'),
            help: "Base name when adding a generated component or module".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "template".into(),
            short: Some('t'),
            help: "Generator name or a legacy template name (rfc/v3/eslint/...)".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "target".into(),
            short: Some('d'),
            help: "Relative target path or directory".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        // Legacy compatibility: accept the old option name for target paths.
        OptionSpec {
            long: "filepath".into(),
            short: None,
            help: "Legacy alias for target path or directory".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "list".into(),
            short: Some('l'),
            help: "List available generators grouped by framework, then exit".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "force".into(),
            short: Some('f'),
            help: "Overwrite conflicting files".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
    ])
    .with_examples(vec![
        Example {
            command: "lan workspace add resource users".into(),
            description: "Generate a multi-file NestJS resource (module/controller/service/dto/entity/spec)"
                .into(),
        },
        Example {
            command: "lan workspace add component Button --target src/components".into(),
            description: "Add a framework-agnostic UI component".into(),
        },
        Example {
            command: "lan workspace add --list".into(),
            description: "Show generators available for the detected framework".into(),
        },
    ])
}

// ─────────────────────────────────────────────────────────────
// product spec 共享的 option 构造辅助
// ─────────────────────────────────────────────────────────────

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
