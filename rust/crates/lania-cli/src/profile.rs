//! CLI 输出配置（OutputProfile）与本地化辅助。
//!
//! 这里把输出行为的来源合并在一起：
//! - 用户偏好（preferences）
//! - 项目配置（lan.config.ui.output / lan.config.ui.progress）
//! - 命令行默认值（fallback）
//!
//! 输出 profile 决定了：
//! - JSON / JSONL / Human
//! - events 是 buffered 还是 stream
//! - 是否输出进度条
//! - 是否包含 host_state / bridge_exchange 等调试信息

use clap::error::ErrorKind as ClapErrorKind;
use lania_config::LanConfigSnapshot;
use lania_logger::CliMessageRenderer;
use lania_preferences::UserPreferences;

pub(crate) fn cli_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub(crate) fn normalize_os_args(args: impl IntoIterator<Item = std::ffi::OsString>) -> Vec<String> {
    args.into_iter()
        .filter_map(|arg| arg.into_string().ok())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputMode {
    Json,
    Jsonl,
    Human,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventMode {
    Buffered,
    Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgressMode {
    Spinner,
    Bar,
    None,
}

#[derive(Debug, Clone)]
pub(crate) struct OutputProfile {
    pub(crate) mode: OutputMode,
    pub(crate) events: EventMode,
    pub(crate) pretty: bool,
    pub(crate) include_host_state: bool,
    pub(crate) include_bridge_exchange: bool,
    pub(crate) progress: ProgressMode,
    pub(crate) locale: String,
}

impl Default for OutputProfile {
    fn default() -> Self {
        Self {
            mode: OutputMode::Json,
            events: EventMode::Buffered,
            pretty: false,
            include_host_state: true,
            include_bridge_exchange: true,
            progress: ProgressMode::Spinner,
            locale: "en".into(),
        }
    }
}

pub(crate) fn cli_text<'a>(locale: &str, en: &'a str, zh: &'a str) -> &'a str {
    if locale == "zh" {
        zh
    } else {
        en
    }
}

pub(crate) fn cli_message_renderer(
    locale: &str,
    preferences: &UserPreferences,
    profile: &OutputProfile,
) -> CliMessageRenderer {
    CliMessageRenderer::default()
        .with_locale(locale)
        .with_timestamps(preferences.log_timestamps && matches!(profile.mode, OutputMode::Human))
}

pub(crate) fn output_profile_from_sources(
    config: Option<&LanConfigSnapshot>,
    preferences: &UserPreferences,
) -> OutputProfile {
    let mut profile = OutputProfile {
        mode: match preferences.output_mode.as_str() {
            "stream" => OutputMode::Jsonl,
            "human" => OutputMode::Human,
            _ => OutputMode::Json,
        },
        events: if preferences.output_mode == "stream" {
            EventMode::Stream
        } else {
            EventMode::Buffered
        },
        ..OutputProfile::default()
    };
    if let Some(config) = config {
        if config_has_raw_key(&config.ui.output.raw, "mode") {
            profile.mode = match config.ui.output.mode.as_str() {
                "jsonl" => OutputMode::Jsonl,
                "human" => OutputMode::Human,
                _ => OutputMode::Json,
            };
        }
        if config_has_raw_key(&config.ui.output.raw, "events") {
            profile.events = if config.ui.output.events == "stream" {
                EventMode::Stream
            } else {
                EventMode::Buffered
            };
        }
        if config_has_raw_key(&config.ui.output.raw, "pretty") {
            profile.pretty = config.ui.output.pretty;
        }
        if config_has_raw_key(&config.ui.output.raw, "includeHostState") {
            profile.include_host_state = config.ui.output.include_host_state;
        }
        if config_has_raw_key(&config.ui.output.raw, "includeBridgeExchange") {
            profile.include_bridge_exchange = config.ui.output.include_bridge_exchange;
        }
        if config_has_raw_key(&config.ui.progress.raw, "style") {
            profile.progress = match config.ui.progress.style.as_str() {
                "none" => ProgressMode::None,
                "bar" => ProgressMode::Bar,
                _ => ProgressMode::Spinner,
            };
        }
    }
    profile
}

fn config_has_raw_key(value: &serde_json::Value, key: &str) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key(key))
}

pub(crate) fn resolve_effective_locale(
    config: Option<&LanConfigSnapshot>,
    preferences: &UserPreferences,
) -> String {
    // Priority:
    // 1) lan.config ui.locale (project override)
    // 2) global ~/.lania/preferences.json
    // 3) default "en"
    let from_config = config.and_then(|cfg| cfg.ui.locale.clone());
    let from_global = Some(preferences.locale.clone());
    let raw = from_config.or(from_global).unwrap_or_else(|| "en".into());
    lania_preferences::normalize_locale(&raw)
}

pub(crate) fn cli_parse_error_message(
    binary_name: &str,
    raw_args: &[String],
    error: &clap::Error,
    locale: &str,
) -> String {
    match error.kind() {
        ClapErrorKind::InvalidSubcommand | ClapErrorKind::UnknownArgument => {
            let invocation = if raw_args.len() > 1 {
                format!("{binary_name} {}", raw_args[1..].join(" "))
            } else {
                binary_name.to_string()
            };
            if locale == "zh" {
                format!("未知命令：`{invocation}`")
            } else {
                format!("unknown command: `{invocation}`")
            }
        }
        _ => error.to_string(),
    }
}

pub(crate) fn localized_user_message(locale: &str, message: &str) -> String {
    if locale != "zh" {
        return message.to_string();
    }

    if let Some(field) = message
        .strip_prefix("unknown ")
        .and_then(|rest| rest.strip_suffix(" field"))
    {
        return format!("未知字段：{field}");
    }
    if let Some(field) = message.strip_prefix("unknown lan.config field `") {
        return format!("未知 lan.config 字段：`{}`", field.trim_end_matches('`'));
    }
    if let Some(field) = message.strip_prefix("unknown extensions field `") {
        return format!("未知 extensions 字段：`{}`", field.trim_end_matches('`'));
    }
    if let Some(field) = message.strip_prefix("unknown schemaDiscovery field `") {
        return format!(
            "未知 schemaDiscovery 字段：`{}`",
            field.trim_end_matches('`')
        );
    }
    if let Some(field) = message.strip_prefix("unknown ui field `") {
        return format!("未知 ui 字段：`{}`", field.trim_end_matches('`'));
    }
    if let Some(field) = message.strip_prefix("unknown ui.output field `") {
        return format!("未知 ui.output 字段：`{}`", field.trim_end_matches('`'));
    }
    if let Some(field) = message.strip_prefix("unknown ui.progress field `") {
        return format!("未知 ui.progress 字段：`{}`", field.trim_end_matches('`'));
    }
    if let Some(field) = message.strip_prefix("unknown ui.interaction field `") {
        return format!(
            "未知 ui.interaction 字段：`{}`",
            field.trim_end_matches('`')
        );
    }
    if let Some(field) = message.strip_prefix("unknown commands field `") {
        return format!("未知 commands 字段：`{}`", field.trim_end_matches('`'));
    }
    if let Some(field) = message.strip_prefix("unknown hook binding field `") {
        return format!("未知 hook 绑定字段：`{}`", field.trim_end_matches('`'));
    }
    if let Some(field) = message.strip_prefix("unknown release field `") {
        return format!("未知 release 字段：`{}`", field.trim_end_matches('`'));
    }
    if let Some(field) = message.strip_prefix("unknown release step field `") {
        return format!("未知 release 步骤字段：`{}`", field.trim_end_matches('`'));
    }
    if message == "lan.config root must be an object" {
        return "lan.config 根节点必须是对象".into();
    }
    if message == "lintTools must be an array" {
        return "lintTools 必须是数组".into();
    }
    if message == "plugins must be an array" {
        return "plugins 必须是数组".into();
    }
    if message == "lintTools entries must be strings" {
        return "lintTools 数组项必须是字符串".into();
    }
    if message == "trusted plugin source must be `package` or `local_path`" {
        return "受信任插件来源必须是 `package` 或 `local_path`".into();
    }
    if let Some(rest) = message.strip_prefix("config version ") {
        return format!("配置版本 {rest}");
    }
    if let Some(path) = message.strip_prefix("git repository not ready in ") {
        return format!("Git 仓库未就绪：{path}");
    }
    if message == "unable to determine git branch for sync" {
        return "无法确定用于 sync 的 Git 分支".into();
    }
    if message == "unable to determine git remote for sync" {
        return "无法确定用于 sync 的 Git 远端".into();
    }
    if message == "unable to determine git remote for push" {
        return "无法确定用于 push 的 Git 远端".into();
    }
    if message == "unable to determine git branch for push" {
        return "无法确定用于 push 的 Git 分支".into();
    }
    if message == "commitizen.run returned no message" {
        return "commitizen.run 未返回提交信息".into();
    }
    if let Some(rest) = message.strip_prefix("commitlint rejected commit message: ") {
        return format!("提交信息未通过 commitlint 校验：{rest}");
    }
    if let Some(remote) = message.strip_prefix("git remote `") {
        let remote = remote.trim_end_matches("` does not exist");
        if message.ends_with("` does not exist") {
            return format!("Git 远端 `{remote}` 不存在");
        }
    }
    if let Some(branch) = message.strip_prefix("git branch `") {
        let branch = branch.trim_end_matches("` does not exist locally or remotely");
        if message.ends_with("` does not exist locally or remotely") {
            return format!("Git 分支 `{branch}` 在本地和远端都不存在");
        }
    }
    if message == "sync commit requires a commit message" {
        return "sync 提交需要提供提交信息".into();
    }
    if message == "release preflight failed: git repository is not ready" {
        return "release 预检查失败：Git 仓库未就绪".into();
    }
    if message == "release preflight failed: package profile requires package.json" {
        return "release 预检查失败：package 模式要求存在 package.json".into();
    }
    if message == "release preflight failed: current branch is unknown for git push" {
        return "release 预检查失败：执行 Git push 时当前分支未知".into();
    }
    if message == "release version is required" {
        return "release 需要提供版本号".into();
    }
    if let Some(rest) =
        message.strip_prefix("release version stage requires package.json script `version`: ")
    {
        return format!("release 版本阶段需要 package.json 中存在 `version` 脚本：{rest}");
    }
    if message == "release publish_or_deploy stage is enabled but no publish or deploy command is available" {
        return "已启用 release publish_or_deploy 阶段，但未配置 publish 或 deploy 命令".into();
    }
    if message == "release post_check stage is enabled but no url or command is configured" {
        return "已启用 release post_check 阶段，但未配置 url 或 command".into();
    }
    if message == "git push requested but no branch is available" {
        return "已请求执行 Git push，但当前没有可用分支".into();
    }
    if let Some(rest) = message.strip_prefix("release step `") {
        if let Some((name, tail)) = rest.split_once("` requires package.json script `") {
            let script = tail.trim_end_matches('`');
            return format!("release 步骤 `{name}` 需要 package.json 中存在脚本 `{script}`");
        }
    }
    if message == "template.list returned no templates" {
        return "template.list 未返回模板列表".into();
    }
    if message == "template.getQuestions returned no questions" {
        return "template.getQuestions 未返回问题列表".into();
    }
    if message == "template.getDependencies returned no payload" {
        return "template.getDependencies 未返回有效结果".into();
    }
    if message == "template.getOutputTasks returned no tasks" {
        return "template.getOutputTasks 未返回任务列表".into();
    }
    if message == "template.render returned no files" {
        return "template.render 未返回文件列表".into();
    }
    if message == "template file path missing" {
        return "模板文件缺少路径".into();
    }
    if message == "template file content missing" {
        return "模板文件缺少内容".into();
    }
    if message == "addTemplate.render returned no payload" {
        return "addTemplate.render 未返回有效结果".into();
    }
    if message == "addTemplate.render returned no content" {
        return "addTemplate.render 未返回内容".into();
    }
    if message == "create prompt did not resolve template" {
        return "create 交互未解析出模板".into();
    }
    if message == "create prompt did not resolve project name" {
        return "create 交互未解析出项目名".into();
    }
    if let Some(name) = message.strip_prefix("unknown template: ") {
        return format!("未知模板：{name}");
    }
    if message == "add prompt did not resolve template" {
        return "add 交互未解析出模板".into();
    }
    if message == "add prompt did not resolve target" {
        return "add 交互未解析出目标位置".into();
    }
    if message
        == "current directory is not empty; use `--directory <name>` to create in a child directory"
    {
        return "当前目录非空；请使用 `--directory <name>` 在子目录中创建".into();
    }
    if let Some(code) = message.strip_prefix("package manager command failed with exit code ") {
        return format!("包管理器命令执行失败，退出码为 {code}");
    }
    if message == "template question missing name" {
        return "模板问题缺少名称".into();
    }
    if message == "template question choice missing label" {
        return "模板问题选项缺少标签".into();
    }
    if message == "config.loadLan returned no payload" {
        return "config.loadLan 未返回有效结果".into();
    }
    if message == "target path must be relative" {
        return "目标路径必须是相对路径".into();
    }
    if message == "target path must not traverse parent directories" {
        return "目标路径不能穿越父目录".into();
    }
    if message == "hook files item must be object" {
        return "hook files 数组项必须是对象".into();
    }
    if message == "hook file path missing" {
        return "hook 文件缺少路径".into();
    }
    if message == "hook file content missing" {
        return "hook 文件缺少内容".into();
    }

    message.to_string()
}
