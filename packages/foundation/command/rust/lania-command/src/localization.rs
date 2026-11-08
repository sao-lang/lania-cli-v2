//! Localize built-in command specs for clap help output.
//!
//! We keep the wire `CommandSpec` shape unchanged (still plain strings), and
//! apply best-effort replacements for known built-in handler ids. This keeps
//! dynamic commands backward compatible while allowing bilingual help/copy.
//!
//! 也就是说，本地化发生在“命令声明已经构建好，但还没交给 clap 渲染”这一层：
//! - 不改 `CommandSpec` 的数据模型
//! - 不碰 parser / handler 执行逻辑
//! - 只改 help/about/example 这些面向人的文案
use crate::CommandSpec;

pub fn localize_command_specs(locale: &str, commands: &mut [CommandSpec]) {
    if locale != "zh" {
        // 目前只内建中文覆写；其它语言保持 spec 原始英文文案。
        return;
    }
    for command in commands {
        localize_spec_zh(command);
    }
}

fn localize_spec_zh(spec: &mut CommandSpec) {
    apply_zh_translations(spec);
    for sub in &mut spec.subcommands {
        localize_spec_zh(sub);
    }
}

fn set_option_help(spec: &mut CommandSpec, long: &str, zh: &str) {
    if let Some(opt) = spec.options.iter_mut().find(|o| o.long == long) {
        opt.help = zh.into();
    }
}

fn set_arg_help(spec: &mut CommandSpec, name: &str, zh: &str) {
    if let Some(arg) = spec.args.iter_mut().find(|a| a.name == name) {
        arg.help = zh.into();
    }
}

fn set_example_desc(spec: &mut CommandSpec, command: &str, zh: &str) {
    if let Some(ex) = spec.examples.iter_mut().find(|e| e.command == command) {
        ex.description = zh.into();
    }
}

/// `git sync` / `git commit` / `git push` 共享的选项文案。
fn localize_git_sync_options(spec: &mut CommandSpec, include_push_toggle: bool) {
    set_option_help(spec, "remote", "远程仓库名称");
    set_option_help(spec, "branch", "分支名称");
    set_option_help(spec, "message", "覆盖提交信息");
    set_option_help(spec, "amend", "amend 最近一次提交");
    set_option_help(spec, "force-with-lease", "使用 lease 保护进行强推");
    set_option_help(spec, "dry-run", "仅规划 git 命令，不实际执行");
    set_option_help(spec, "interactive", "通过 commitizen 生成提交信息");
    if include_push_toggle {
        set_option_help(spec, "push", "提交后推送");
    }
}

fn localize_release_shared_options(spec: &mut CommandSpec) {
    set_option_help(spec, "version", "发布版本号");
    set_option_help(spec, "tag", "包发布 dist-tag 覆写");
    set_option_help(
        spec,
        "profile",
        "发布配置：package、web-app、service、custom",
    );
    set_option_help(spec, "env", "目标发布环境");
    set_option_help(spec, "channel", "发布通道或部署通道");
    set_option_help(spec, "from", "从指定阶段开始发布");
    set_option_help(spec, "to", "在指定阶段后停止发布");
    set_option_help(spec, "skip", "要跳过的阶段列表（逗号分隔）");
    set_option_help(spec, "state-file", "发布状态文件路径");
    set_option_help(spec, "publish", "启用包发布阶段");
    set_option_help(spec, "changelog", "启用 changelog 阶段");
    set_option_help(spec, "skip-git", "禁用最终 git commit/tag/push 行为");
    set_option_help(spec, "apply", "执行发布动作，而不是仅输出计划");
    set_option_help(spec, "dry-run", "即使在 run/resume 中也强制只做计划");
    set_option_help(spec, "yes", "执行发布时跳过额外交互确认");
}

fn localize_product_common_options(spec: &mut CommandSpec) {
    set_option_help(spec, "path", "product 根目录路径（默认当前工作目录）");
}

fn localize_lint_options(spec: &mut CommandSpec) {
    set_option_help(spec, "fix", "以修复模式运行 lint");
    set_option_help(
        spec,
        "linters",
        "仅运行指定 linter adaptor（逗号分隔：eslint,oxlint,prettier,oxfmt,stylelint,textlint,ruff,black,mypy,flake8,golangci-lint,go-vet,clippy,dart-analyze）",
    );
    set_option_help(spec, "concurrency", "设置 lint adaptor 并发数上限");
    set_option_help(spec, "grouped-output", "按 adaptor 分组输出 lint 结果");
}

fn localize_env_mutation(spec: &mut CommandSpec) {
    set_arg_help(
        spec,
        "tool",
        "要管理的工具：node/python/go/rust/deno/bun/zig/java/dart/kotlin/scala/julia/d/v/crystal/gleam/haxe/elixir",
    );
    set_arg_help(
        spec,
        "version",
        "目标版本（支持前缀），例如 20 / 3.12 / 1.86.0",
    );
    set_option_help(spec, "yes", "真正执行计划（默认仅演练）");
    set_option_help(spec, "project", "将版本写入项目版本文件");
}

/// 解析包级 config 子树的 handler id：`command.<pkg>.config[.<sub>]`。
///
/// 全局 `lan config`（`command.config` / `command.config.get` 等）第 3 段不是
/// `config`，其它命令（`command.dev.product` …）同理，均不匹配——因此这个分支
/// 只覆盖 11 个命令子包新增的 config 子树，不会误伤既有命令。
fn package_config_handler(handler_id: &str) -> Option<(String, String)> {
    let rest = handler_id.strip_prefix("command.")?;
    let mut parts = rest.split('.');
    let pkg = parts.next()?;
    if pkg.is_empty() || parts.next()? != "config" {
        return None;
    }
    let sub = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return None;
    }
    Some((pkg.to_string(), sub.to_string()))
}

/// 所有 `command.<pkg>.config*` 的统一 zh 翻译：about / option help / arg help /
/// example description 各一份逻辑，按 handler_id 后缀与字段名选取，不给每个包
/// 复制一遍分支。任何兜底文案同样含 CJK，保证覆盖测试通过。
fn localize_package_config_zh(spec: &mut CommandSpec, pkg: &str, sub: &str) {
    spec.about = match sub {
        "" => format!("管理 {pkg} 命令包偏好与全局 CLI 配置"),
        "get" => "读取偏好配置（省略键时显示全部）".into(),
        "set" => "写入偏好配置".into(),
        "init" => "初始化全局偏好并报告项目配置".into(),
        "doctor" => "只读诊断全局偏好与项目配置".into(),
        "sync" => "列出全局与包级作用域的配置同步建议".into(),
        _ => format!("管理 {pkg} 命令包偏好与全局 CLI 配置"),
    };

    for option in &mut spec.options {
        option.help = match option.long.as_str() {
            "global" => "操作全局 CLI 偏好，而非包级配置".into(),
            "force" => "覆盖已存在的偏好文件".into(),
            "write" => "写入建议的配置项".into(),
            _ => "配置命令选项".into(),
        };
    }
    for arg in &mut spec.args {
        arg.help = match arg.name.as_str() {
            "key" => "配置键（locale | output.mode | log.timestamps，或本包特有键）".into(),
            "value" => "配置值（按 configSchema 校验）".into(),
            _ => "配置参数".into(),
        };
    }

    let ex = |suffix: &str| format!("lan {pkg} config{suffix}");
    let translations: Vec<(String, &str)> = vec![
        (ex(""), "查看本包偏好及其存储路径"),
        (ex(" get locale"), "读取偏好（省略 --global 时为包级作用域）"),
        (ex(" set output.mode json"), "写入一条包级偏好"),
        (ex(" --global"), "查看全局 CLI 偏好"),
        (ex(" init"), "初始化全局偏好并报告项目配置"),
        (ex(" doctor"), "只读诊断偏好与项目配置"),
        (ex(" sync"), "列出全局与包级作用域的配置同步建议"),
    ];
    for example in &mut spec.examples {
        example.description = translations
            .iter()
            .find(|(command, _)| command == &example.command)
            .map(|(_, zh)| *zh)
            .unwrap_or("查看或设置配置")
            .into();
    }
}

fn apply_zh_translations(spec: &mut CommandSpec) {
    // 包级 config 子树统一走一份通用翻译，避免 11 个包各自维护一套分支。
    if let Some((pkg, sub)) = package_config_handler(&spec.handler_id) {
        localize_package_config_zh(spec, &pkg, &sub);
        return;
    }
    match spec.handler_id.as_str() {
        // config
        "command.config" => {
            spec.about = "管理全局 CLI 配置与项目 lan.config".into();
            set_example_desc(spec, "lan config", "查看当前全局 CLI 配置");
            set_example_desc(spec, "lan config get locale", "读取当前语言配置");
            set_example_desc(spec, "lan config set locale zh", "切换 CLI 语言为中文");
            set_example_desc(
                spec,
                "lan config set log.timestamps true",
                "开启人类可读日志时间戳",
            );
            set_example_desc(
                spec,
                "lan config set output.mode json",
                "切换终端输出为 JSON 模式",
            );
            set_example_desc(
                spec,
                "lan config set output.mode human",
                "切换终端输出为人类可读模式",
            );
            set_example_desc(spec, "lan config init", "为当前项目创建 lan.config.ts");
            set_example_desc(spec, "lan config doctor", "只读诊断项目 lan.config");
            set_example_desc(
                spec,
                "lan config set runtime go",
                "在 lan.config.ts 里固定项目运行时",
            );
            set_example_desc(spec, "lan config sync", "列出项目配置缺失项的建议");
        }
        "command.config.get" => {
            spec.about = "读取全局 CLI 配置".into();
            set_arg_help(spec, "key", "配置项：locale | log.timestamps | output.mode");
            set_option_help(
                spec,
                "package",
                "读取指定命令子包的包级偏好（~/.lania/config/<pkg>.json）",
            );
        }
        "command.config.set" => {
            spec.about = "更新全局 CLI 配置或项目 lan.config".into();
            set_arg_help(
                spec,
                "key",
                "全局配置项（locale | log.timestamps | output.mode）或项目配置路径（runtime | lintTools | runtimes.go.build | …）",
            );
            set_arg_help(spec, "value", "配置值（列表可用逗号分隔或 JSON）");
            set_option_help(
                spec,
                "package",
                "写入指定命令子包的包级偏好（~/.lania/config/<pkg>.json）",
            );
        }
        "command.config.init" => {
            spec.about = "为当前项目创建 lan.config.ts".into();
            set_option_help(spec, "force", "即使已有配置文件也覆盖 lan.config.ts");
        }
        "command.config.doctor" => {
            spec.about = "只读诊断项目 lan.config".into();
        }
        "command.config.sync" => {
            spec.about = "给出（或写入）项目配置缺失项的建议".into();
            set_option_help(spec, "write", "纯数据配置时直接写入建议内容");
        }
        "command.config.packages" => {
            spec.about = "查看所有命令子包的包级配置".into();
        }
        "command.config.apply" => {
            spec.about = "把全局配置一键下发到命令子包".into();
            set_option_help(spec, "package", "只应用到指定子包（短命令名或 npm 包名）");
        }

        // workspace 根命令（dev/build/lint/create/add 五个原顶级命令的宿主）
        "command.workspace" => {
            spec.about = "Workspace 开发工作流（dev/build/lint/create/add）".into();
            set_example_desc(spec, "lan workspace dev", "运行默认开发工作流");
            set_example_desc(spec, "lan workspace build", "执行一次生产构建");
            set_example_desc(spec, "lan workspace lint", "运行 lint 检查");
            set_example_desc(spec, "lan workspace create --name demo-app", "用内置模板创建项目");
            set_example_desc(spec, "lan workspace add --list", "查看当前框架可用的生成器");
        }

        // dev（现为 `lan workspace dev`）
        "command.dev" => {
            spec.about = "启动项目开发工作流".into();
            set_option_help(spec, "port", "覆盖开发服务器端口");
            set_option_help(spec, "config", "兼容参数：旧版 config 路径（可能被忽略）");
            set_option_help(spec, "path", "兼容参数：旧版项目路径（相对路径会基于 cwd）");
            set_option_help(spec, "host", "覆盖开发服务器 host");
            set_option_help(
                spec,
                "hmr",
                "启用或禁用 HMR（按底层 dev server 支持情况生效）",
            );
            set_option_help(spec, "open", "开发服务器就绪后打开浏览器");
            set_option_help(spec, "mode", "转发 dev mode 到底层编译器");
            set_option_help(spec, "runtime", "覆盖运行时（web|python|go|rust|dart）");
            set_option_help(
                spec,
                "quiet-app",
                "开发服务器就绪后隐藏应用日志（channel: app）",
            );
            set_example_desc(spec, "lan workspace dev", "运行默认开发工作流");
            set_example_desc(
                spec,
                "lan workspace dev --port 3001 --open --mode development",
                "使用自定义端口运行开发工作流",
            );
            set_example_desc(
                spec,
                "lan workspace dev --quiet-app",
                "运行开发工作流但不透出应用自身运行日志",
            );
        }

        // build（现为 `lan workspace build`）
        "command.build" => {
            spec.about = "执行项目构建工作流".into();
            set_option_help(spec, "config", "兼容参数：旧版 config 路径（可能被忽略）");
            set_option_help(spec, "path", "兼容参数：旧版项目路径（相对路径会基于 cwd）");
            set_option_help(spec, "watch", "以 watch 模式保持构建进程运行");
            set_option_help(spec, "mode", "转发构建 mode 到底层编译器");
            set_option_help(spec, "output-dir", "覆盖构建输出目录");
            set_option_help(spec, "runtime", "覆盖运行时（web|python|go|rust|dart）");
            set_example_desc(spec, "lan workspace build", "执行一次生产构建");
            set_example_desc(
                spec,
                "lan workspace build --watch --mode development",
                "以 watch 模式运行构建工作流",
            );
        }

        // lint（现为 `lan workspace lint`）
        "command.lint" => {
            spec.about = "运行项目 lint 检查".into();
            localize_lint_options(spec);
            set_example_desc(spec, "lan workspace lint", "运行 lint 检查");
            set_example_desc(
                spec,
                "lan workspace lint fix --concurrency 2",
                "以修复模式运行 lint，并限制并发数",
            );
        }
        "command.lint.check" => {
            spec.about = "以检查模式运行 lint".into();
            localize_lint_options(spec);
        }
        "command.lint.fix" => {
            spec.about = "以修复模式运行 lint".into();
            localize_lint_options(spec);
        }

        // create
        "command.create" => {
            spec.about = "从模板创建新项目".into();
            set_arg_help(spec, "path", "项目路径（使用 \".\" 表示当前目录）");
            set_option_help(spec, "name", "项目名称");
            set_option_help(spec, "template", "模板名称");
            set_option_help(spec, "package-manager", "包管理器");
            set_option_help(spec, "directory", "兼容参数：旧版目录选项（等同于 path）");
            set_option_help(spec, "git", "初始化 git 仓库");
            set_option_help(spec, "skip-git", "兼容参数：旧版 --no-git");
            set_option_help(spec, "skip-install", "跳过包管理器 init/install 步骤");
            set_option_help(spec, "language", "首选语言（透传给模板上下文）");
            set_option_help(
                spec,
                "lint-tools",
                "逗号分隔的 lint 工具列表（转发给模板上下文）",
            );
            set_option_help(spec, "dry-run", "仅规划文件写入与安装命令，不实际执行");
            set_option_help(spec, "preview", "预览渲染的模板文件（隐含 dry-run）");
            set_example_desc(spec, "lan workspace create --name demo-app", "用内置模板创建项目");
            set_example_desc(spec, "lan workspace create .", "在当前目录创建项目");
            set_example_desc(
                spec,
                "lan workspace create --template toolkit --package-manager pnpm",
                "使用 pnpm 创建 toolkit 模板项目",
            );
            set_example_desc(
                spec,
                "lan workspace create --name svc --template go-service --package-manager go",
                "用 Go 工具链创建 Go 服务项目",
            );
            set_example_desc(
                spec,
                "lan workspace create --template spa-vue --preview",
                "预览渲染结果（不写入文件）",
            );
        }

        // add
        "command.add" => {
            spec.about = "向现有工作区添加生成内容".into();
            set_arg_help(
                spec,
                "generator",
                "生成器名称，例如 `resource`（NestJS）或 `component`",
            );
            set_arg_help(spec, "generator-name", "用于推导类名与文件路径的基础名称");
            set_option_help(spec, "name", "添加组件/模块时使用的基础名称");
            set_option_help(
                spec,
                "template",
                "生成器名称，或旧的模板名（rfc/v3/eslint/...）",
            );
            set_option_help(spec, "target", "目标相对路径或目录");
            set_option_help(spec, "filepath", "兼容参数：旧版目标路径参数名");
            set_option_help(spec, "list", "按框架分组列出可用生成器后退出");
            set_option_help(spec, "force", "覆盖冲突文件");
            set_example_desc(
                spec,
                "lan workspace add resource users",
                "生成 NestJS 多文件资源（module/controller/service/dto/entity/spec）",
            );
            set_example_desc(
                spec,
                "lan workspace add component Button --target src/components",
                "添加一个与框架无关的 UI 组件",
            );
            set_example_desc(spec, "lan workspace add --list", "查看当前框架可用的生成器");
        }

        "command.template" => {
            spec.about = "查看模板列表或某个模板的详情".into();
            set_arg_help(spec, "name", "模板名称");
            set_example_desc(spec, "lan workspace create template", "交互式查看可用模板");
            set_example_desc(spec, "lan workspace create template toolkit", "查看指定模板详情");
        }

        // product root / product workflows
        "command.product" => {
            spec.about = "面向 product 的 CLI 工作流与分发命令".into();
            set_example_desc(
                spec,
                "lan product dev hello --path ./products/acme-cli",
                "以开发模式运行本地 product 命令",
            );
            set_example_desc(
                spec,
                "lan product inspect --path ./products/acme-cli --compat",
                "检查 product 兼容性与本地分发状态",
            );
        }
        "command.dev.product" => {
            spec.about = "以开发模式运行本地 product".into();
            set_arg_help(spec, "args", "product 命令及其参数");
            localize_product_common_options(spec);
            set_option_help(spec, "watch", "监听 product 文件变化并在变更后重启转发命令");
            set_option_help(
                spec,
                "poll-interval-ms",
                "product watch 模式的轮询间隔（毫秒）",
            );
            set_example_desc(
                spec,
                "lan product dev --watch ops hello --path ./products/acme-cli",
                "当本地 product 文件变化时重新运行 product 命令",
            );
        }
        "command.build.product" => {
            spec.about = "构建一个用于打包的最小 product 快照".into();
            localize_product_common_options(spec);
            set_option_help(spec, "output-dir", "覆盖 product 构建输出目录");
            set_option_help(spec, "clean", "写入前清理 product 构建输出目录");
            set_example_desc(
                spec,
                "lan product build --output-dir .lania/build/product",
                "生成供后续 pack/publish 使用的最小 product 快照",
            );
        }
        "command.pack.product" => {
            spec.about = "基于已构建的 product 快照组装最小 install-root 布局".into();
            localize_product_common_options(spec);
            set_option_help(
                spec,
                "build-dir",
                "由 `lan product build` 生成的 product 构建目录",
            );
            set_option_help(spec, "output-dir", "覆盖 product 打包输出目录");
            set_option_help(spec, "clean", "写入前清理 product 打包输出目录");
            set_example_desc(
                spec,
                "lan product pack --build-dir .lania/build/product",
                "生成一个可在发布前本地校验的 install-root 布局",
            );
        }
        "command.publish.product" => {
            spec.about = "基于已打包的 product 组装可发布的 npm 包布局".into();
            localize_product_common_options(spec);
            set_option_help(
                spec,
                "pack-dir",
                "由 `lan product pack` 生成的 product 打包目录",
            );
            set_option_help(spec, "output-dir", "覆盖 product 发布输出目录");
            set_option_help(spec, "dist-tag", "用于发布规划的 registry dist-tag");
            set_option_help(spec, "channel", "用于发布规划的发布通道");
            set_option_help(spec, "registry", "覆盖发布规划或执行使用的 npm registry");
            set_option_help(
                spec,
                "platform-binaries-dir",
                "存放各平台 `lania-cli` 二进制的暂存目录",
            );
            set_option_help(
                spec,
                "platform-binary-paths",
                "将平台标识映射到注入二进制路径的 JSON 对象",
            );
            set_option_help(
                spec,
                "execute",
                "根据生成的 publish manifest 执行 npm publish 步骤",
            );
            set_option_help(spec, "dry-run", "以 npm --dry-run 执行发布步骤");
            set_option_help(spec, "yes", "非 --dry-run 情况下确认真实 npm publish 执行");
            set_option_help(spec, "resume", "从已完成的 manifest 步骤继续发布执行");
            set_option_help(spec, "otp", "为发布执行传入 npm OTP");
            set_option_help(spec, "npm-bin", "覆盖发布执行使用的 npm 可执行文件");
            set_option_help(spec, "max-retries", "瞬时 npm 发布失败时的重试次数");
            set_option_help(spec, "retry-delay-ms", "发布重试之间的延迟毫秒数");
            set_option_help(
                spec,
                "rollback-on-failure",
                "部分发布失败后执行 npm unpublish 回滚",
            );
            set_option_help(spec, "clean", "写入前清理 product 发布输出目录");
            set_example_desc(
                spec,
                "lan product publish --pack-dir .lania/pack/product/install-root",
                "生成一个可发布的 npm 包产物，但不真正推送到 registry",
            );
            set_example_desc(
                spec,
                "lan product publish --dist-tag next --channel next",
                "生成面向 next 通道的 registry 发布 manifest",
            );
            set_example_desc(
                spec,
                "lan product publish --platform-binaries-dir /tmp/lania-cli-platforms",
                "从暂存根目录自动发现平台二进制，而不是逐个手工列出",
            );
            set_example_desc(
                spec,
                "lan product publish --platform-binary-paths '{\"linux-x64\":\"/tmp/lania-cli-linux-x64\"}'",
                "为非当前主机的平台包注入对应二进制路径",
            );
            set_example_desc(
                spec,
                "lan product publish --execute --dry-run",
                "通过 npm publish --dry-run 执行 publish manifest 步骤",
            );
            set_example_desc(
                spec,
                "lan product publish --execute --dry-run --npm-bin /tmp/fake-npm",
                "使用自定义 npm 二进制执行 publish manifest 步骤",
            );
            set_example_desc(
                spec,
                "lan product publish --execute --yes --registry http://localhost:4873",
                "对本地测试 registry 预演一次真实发布流程",
            );
        }
        "command.inspect.product" => {
            spec.about = "检查 product 配置、schema 发现结果与本地产物".into();
            localize_product_common_options(spec);
            set_option_help(
                spec,
                "compat",
                "包含兼容性快照并写出 compat-report.json（实验性）",
            );
            set_example_desc(
                spec,
                "lan product inspect --path ./products/acme-cli",
                "检查当前 product 配置、schema 根目录与本地构建状态",
            );
            set_example_desc(
                spec,
                "lan product inspect --path ./products/acme-cli --compat",
                "检查 product 兼容性快照（framework/protocol/product）",
            );
        }
        "command.doctor.product" => {
            spec.about = "运行 product doctor 诊断，包括兼容性检查".into();
            localize_product_common_options(spec);
            set_example_desc(
                spec,
                "lan product doctor --path ./products/acme-cli",
                "运行包含兼容性、产物和 schema 检查的 product 诊断",
            );
        }

        // tools
        "command.tools" => {
            spec.about = "列出命令、按类型运行文件，或查看本地文件".into();
            set_option_help(spec, "filter", "按子串过滤命令名");
            set_option_help(spec, "limit", "限制返回的命令数量");
            set_option_help(spec, "shell", "包含 shell 内建、别名和函数");
            set_option_help(spec, "all-matches", "显示重复命令名的每个 PATH 匹配项");
            set_option_help(spec, "names-only", "仅返回命令名，不显示详细条目");
            set_option_help(
                spec,
                "group-by-source",
                "按 PATH、shell 内建、别名或函数分组",
            );
            set_option_help(spec, "plain", "以纯文本列表渲染，而不是结构化命令条目");
            set_option_help(spec, "unique", "对命令名去重，并保留首次匹配顺序");
            set_example_desc(
                spec,
                "lan tools",
                "列出 PATH 命令以及 shell 内建、别名和函数",
            );
            set_example_desc(
                spec,
                "lan tools --filter ts",
                "列出命令名中包含 `ts` 的终端命令",
            );
            set_example_desc(
                spec,
                "lan tools --all-matches --filter node",
                "显示命令名中包含 `node` 的所有 PATH 匹配项",
            );
            set_example_desc(
                spec,
                "lan tools --names-only --no-shell",
                "以紧凑列表仅返回 PATH 命令名",
            );
            set_example_desc(
                spec,
                "lan tools --group-by-source",
                "按 PATH、shell 内建、别名和函数分组展示命令",
            );
            set_example_desc(
                spec,
                "lan tools --plain --group-by-source",
                "以纯文本渲染分组后的命令名",
            );
            set_example_desc(
                spec,
                "lan tools --plain --names-only --unique",
                "以纯文本逐行输出去重后的命令名",
            );
            set_example_desc(
                spec,
                "lan tools run ./scripts/demo.ts -- --port 3000",
                "检测文件类型并使用匹配的运行时执行",
            );
            set_example_desc(
                spec,
                "lan tools view ./src/index.ts",
                "显示文件内容及行号，或使用系统应用打开媒体文件",
            );
        }
        "command.tools.list" => {
            spec.about = "列出终端可解析的命令".into();
            set_option_help(spec, "filter", "按子串过滤命令名");
            set_option_help(spec, "limit", "限制返回的命令数量");
            set_option_help(spec, "shell", "包含 shell 内建、别名和函数");
            set_option_help(spec, "all-matches", "显示重复命令名的每个 PATH 匹配项");
            set_option_help(spec, "names-only", "仅返回命令名，不显示详细条目");
            set_option_help(
                spec,
                "group-by-source",
                "按 PATH、shell 内建、别名或函数分组",
            );
            set_option_help(spec, "plain", "以纯文本列表渲染，而不是结构化命令条目");
            set_option_help(spec, "unique", "对命令名去重，并保留首次匹配顺序");
        }
        "command.tools.run" => {
            spec.about = "按检测到的运行时执行代码文件".into();
            set_arg_help(spec, "file", "要执行的代码文件路径");
            set_arg_help(spec, "args", "传给目标文件的附加参数");
        }
        "command.tools.view" => {
            spec.about = "显示文件内容或使用系统应用打开媒体文件".into();
            set_arg_help(spec, "path", "要查看的文件路径");
            set_option_help(spec, "lines", "限制显示的文本行数");
            set_option_help(spec, "start", "从指定的 1-based 行号开始查看");
            set_option_help(spec, "end", "在指定的 1-based 行号结束查看");
            set_option_help(spec, "tail", "显示文本文件的最后 N 行");
            set_option_help(spec, "head", "显示文本文件的前 N 行");
            set_option_help(spec, "grep", "按子串过滤可见文本或目录条目");
            set_option_help(spec, "regex", "按正则表达式过滤可见文本或目录条目");
            set_option_help(spec, "ignore-case", "为 grep 或 regex 启用大小写不敏感匹配");
            set_option_help(spec, "tree", "以递归树形结构渲染目录");
            set_option_help(spec, "max-depth", "限制递归遍历目录的深度");
            set_option_help(spec, "sort", "按名称、大小、时间或扩展名排序目录条目");
            set_option_help(spec, "reverse", "反转目录排序顺序");
            set_option_help(spec, "hidden", "包含隐藏文件和目录");
            set_option_help(spec, "files-only", "查看目录时仅包含文件条目");
            set_option_help(spec, "dirs-only", "查看目录时仅包含目录条目");
            set_option_help(spec, "hex-bytes", "限制二进制十六进制预览的字节数");
            set_option_help(spec, "meta-only", "仅显示文件元信息，不打开外部查看器");
        }

        // git (+ subcommands)
        "command.git" => {
            spec.about = "查看与同步 git 状态".into();
            set_example_desc(spec, "lan git status", "查看工作区状态");
            set_example_desc(
                spec,
                "lan git sync -m \"feat: ship it\"",
                "暂存、提交并推送当前修改",
            );
            set_example_desc(spec, "lan git log --limit 5", "查看最近五条提交");
        }
        "command.git.sync" => {
            spec.about = "暂存、提交并推送当前修改".into();
            localize_git_sync_options(spec, true);
            set_example_desc(
                spec,
                "lan git sync -m \"chore: update workspace\"",
                "暂存、提交并推送当前修改",
            );
            set_example_desc(
                spec,
                "lan git sync --no-push -m \"feat: partial sync\"",
                "仅本地提交，不推送",
            );
        }
        "command.git.commit" => {
            spec.about = "暂存并提交修改".into();
            localize_git_sync_options(spec, true);
            set_example_desc(
                spec,
                "lan git commit -m \"chore: save work\" --no-push",
                "仅创建本地提交",
            );
        }
        "command.git.push" => {
            spec.about = "推送当前分支到远程".into();
            localize_git_sync_options(spec, false);
        }
        "command.git.status" => {
            spec.about = "查看工作区状态".into();
        }
        "command.git.log" => {
            spec.about = "查看最近的提交记录".into();
            set_option_help(spec, "limit", "显示的提交条数");
        }
        "command.git.diff" => {
            spec.about = "查看工作区或暂存区差异".into();
            set_option_help(spec, "staged", "仅查看已暂存的修改");
            set_option_help(spec, "path", "把差异限制在指定路径");
        }
        "command.git.branch" => {
            spec.about = "管理分支".into();
        }
        "command.git.branch.list" => {
            spec.about = "列出本地与远程分支".into();
            set_example_desc(spec, "lan git branch list", "列出本地与远程分支");
        }
        "command.git.branch.create" => {
            spec.about = "创建分支".into();
            set_arg_help(spec, "branch", "分支名称");
        }
        "command.git.branch.switch" => {
            spec.about = "切换到指定分支".into();
            set_arg_help(spec, "branch", "分支名称");
        }
        "command.git.branch.delete" => {
            spec.about = "删除分支".into();
            set_arg_help(spec, "branch", "分支名称");
            set_option_help(spec, "yes", "确认执行删除");
            set_option_help(spec, "force", "即使分支未合并也删除（-D）");
        }
        "command.git.tag" => {
            spec.about = "管理标签".into();
        }
        "command.git.tag.list" => {
            spec.about = "列出标签".into();
        }
        "command.git.tag.create" => {
            spec.about = "创建标签".into();
            set_arg_help(spec, "tag", "标签名称");
            set_option_help(spec, "message", "附注标签的说明文字");
        }
        "command.git.tag.delete" => {
            spec.about = "删除标签".into();
            set_arg_help(spec, "tag", "标签名称");
            set_option_help(spec, "yes", "确认执行删除");
        }
        "command.git.add" => {
            spec.about = "暂存工作区中的全部修改".into();
            set_option_help(spec, "dry-run", "仅规划 git 命令，不实际执行");
        }
        "command.git.restore" => {
            spec.about = "丢弃工作区或暂存区的修改".into();
            set_arg_help(spec, "paths", "要恢复的路径（默认整个工作区）");
            set_option_help(spec, "staged", "仅取消暂存，保留工作区修改");
            set_option_help(spec, "dry-run", "仅规划 git 命令，不实际执行");
        }

        // release (subset: the CLI help strings are still English in specs; translate the spec-level copy)
        "command.release" => {
            spec.about = "发布工作流（规划、执行、恢复）".into();
            localize_release_shared_options(spec);
            set_example_desc(
                spec,
                "lan release --profile web-app --env prod",
                "预览一次发布计划，但不实际执行",
            );
            set_example_desc(
                spec,
                "lan release run --apply --yes --from verify --to finalize",
                "执行指定的发布阶段范围",
            );
        }
        "command.release.plan" => {
            spec.about = "生成发布计划并持久化发布状态".into();
            localize_release_shared_options(spec);
            set_example_desc(
                spec,
                "lan release plan --profile package --version 1.2.3",
                "预览 package 发布阶段",
            );
            set_example_desc(
                spec,
                "lan release plan --profile web-app --env prod --skip version",
                "预览 web 部署计划",
            );
        }
        "command.release.run" => {
            spec.about = "执行发布计划并持久化状态".into();
            localize_release_shared_options(spec);
        }
        "command.release.resume" => {
            spec.about = "恢复一次失败或未完成的发布流程".into();
            localize_release_shared_options(spec);
        }
        "command.release.status" => {
            spec.about = "查看最新发布状态".into();
            set_option_help(spec, "state-file", "发布状态文件路径");
        }

        // env
        "command.env" => {
            spec.about =
                "由 lania 自行安装并切换语言运行时（node/python/go/rust/deno/bun/zig/java/dart/kotlin/scala/julia/d/v/crystal/gleam/haxe/elixir）".into();
            set_example_desc(
                spec,
                "lan env init",
                "打印把 ~/.runex/shims 加入 PATH 的片段",
            );
            set_example_desc(
                spec,
                "lan env install node 20 --yes",
                "从 nodejs.org 下载 node 20.x 并登记到本地",
            );
            set_example_desc(
                spec,
                "lan env use node 20",
                "把 node 的 current 指针指向 20.x 并重新生成 shim",
            );
            set_example_desc(
                spec,
                "lan env list --installed",
                "只列出 lania 自己安装过的版本",
            );
            set_example_desc(
                spec,
                "lan env info node",
                "查看 node：可执行文件路径、来源、上游渠道与项目固定版本",
            );
            set_example_desc(
                spec,
                "lan env which node",
                "输出 node 的 shim 当前指向的真实可执行文件",
            );
            set_example_desc(
                spec,
                "lan env use python 3.12 --project",
                "将 python 3.12 固定到本项目的 .python-version",
            );
            set_example_desc(
                spec,
                "lan env remove node 18 --yes",
                "删除 lania 管理的 node 18 目录",
            );
            set_example_desc(
                spec,
                "lan env doctor",
                "报告缺失的运行时、shim 缺口与 PATH 问题",
            );
            set_example_desc(spec, "lan env ui", "打开终端界面以可视化管理运行时");
            set_example_desc(
                spec,
                "lan env config set shim.host runex",
                "让生成的 shim 改调 `runex which`（env 包级偏好）",
            );
        }
        "command.env.list" => {
            spec.about = "显示所有受支持的工具链及探测到的版本".into();
            set_arg_help(spec, "tool", "按工具或语言名过滤");
            set_option_help(
                spec,
                "installed",
                "只列出 lania 安装的版本（读取 ~/.runex，不联网）",
            );
        }
        "command.env.info" => {
            spec.about = "查看单个工具链的详情".into();
            set_arg_help(
                spec,
                "tool",
                "要查看的工具：node/python/go/rust/deno/bun/zig/java/dart/kotlin/scala/julia/d/v/crystal/gleam/haxe/elixir",
            );
        }
        "command.env.versions" => {
            spec.about = "列出某个工具链已安装的版本".into();
            set_arg_help(
                spec,
                "tool",
                "要列出的工具：node/python/go/rust/deno/bun/zig/java/dart/kotlin/scala/julia/d/v/crystal/gleam/haxe/elixir",
            );
            set_option_help(spec, "available", "同时查询上游可安装的版本");
        }
        "command.env.install" => {
            spec.about = "下载并安装指定版本的运行时".into();
            localize_env_mutation(spec);
        }
        "command.env.use" => {
            spec.about = "切换 current 运行时版本".into();
            localize_env_mutation(spec);
        }
        "command.env.remove" => {
            spec.about = "删除由 lania 管理的运行时版本".into();
            localize_env_mutation(spec);
        }
        "command.env.doctor" => {
            spec.about = "检查缺失的运行时、shim 缺口与 PATH 设置".into();
        }
        "command.env.which" => {
            spec.about = "输出自管入口当前解析到的可执行文件".into();
            set_arg_help(spec, "name", "入口名，例如 node / npm / python3 / go");
            set_option_help(spec, "resolve", "只输出解析后的绝对路径（供 shim 使用）");
        }
        "command.env.init" => {
            spec.about = "打印把 lania 的 shims 目录加入 PATH 的 shell 片段".into();
            set_option_help(
                spec,
                "shell",
                "目标 shell：bash / zsh / fish / powershell（默认自动探测）",
            );
        }
        "command.env.ui" => {
            spec.about = "打开用于管理工具链的终端界面".into();
            set_option_help(spec, "snapshot", "只输出一帧非交互画面，不启动 TUI");
            set_option_help(spec, "cols", "快照宽度（默认 100）");
            set_option_help(spec, "rows", "快照高度（默认 28）");
            set_option_help(spec, "down", "仅快照：模拟按 N 次方向键（用于验证滚动）");
            set_option_help(
                spec,
                "no-versions",
                "仅快照：跳过版本探测，以快速获得纯布局画面",
            );
            set_example_desc(
                spec,
                "lan env ui",
                "打开双栏终端界面（方向键移动，i/u/d 执行操作）",
            );
            set_example_desc(
                spec,
                "lan env ui --snapshot",
                "渲染一帧画面而不进入交互模式",
            );
        }
        // pi（旧名 agent，保留为别名）
        "command.pi" => {
            spec.about = "运行基于 pi 的编码 agent（交互式 REPL 或单次执行）".into();
            set_arg_help(spec, "prompt", "提示词文本；提供后 agent 以单次模式运行");
            set_option_help(spec, "model", "模型 id（例如 claude-sonnet-4-5）");
            set_option_help(spec, "provider", "提供商 id（anthropic/openai/...）");
            set_option_help(spec, "thinking", "推理强度");
            set_option_help(
                spec,
                "approval-mode",
                "工具审批模式：always-ask | write | yolo",
            );
            set_option_help(spec, "print", "单次模式：无位置参数时从 stdin 读取提示词");
            set_option_help(spec, "continue", "续接最近一次会话");
            set_option_help(spec, "resume", "续接指定会话（见 --sessions）");
            set_option_help(spec, "new", "强制新建会话");
            set_option_help(spec, "no-session", "不把对话写入 .lan/agent");
            set_option_help(spec, "sessions", "列出本工作区的会话后退出");
            set_option_help(spec, "json", "每个事件输出一行 JSON");
            set_option_help(spec, "no-tui", "强制使用行式 REPL，不进入全屏 TUI");
            set_option_help(
                spec,
                "snapshot",
                "渲染 TUI 画面到 stdout 后退出（CI / 无终端环境）",
            );
            set_option_help(spec, "verbose", "输出额外诊断信息");
            set_example_desc(spec, "lan pi", "启动交互式会话");
            set_example_desc(
                spec,
                "lan pi \"explain the rust crate layout\"",
                "单次执行提示词",
            );
            set_example_desc(spec, "lan pi -c", "续接上一次会话");
            set_example_desc(spec, "lan pi --sessions", "列出已保存的会话");
            set_example_desc(
                spec,
                "lan pi --approval-mode yolo \"run the test suite\"",
                "单次执行且不弹出审批提示",
            );
        }

        _ => {}
    }
}
