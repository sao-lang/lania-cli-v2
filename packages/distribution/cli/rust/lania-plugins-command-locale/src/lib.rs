//! `config` command: manage global CLI preferences **and** the project `lan.config.ts`.
//!
//! 两类作用域刻意共用一个命令（`docs/design/multi-runtime-bridge.zh-CN.md` §6.5.8）：
//!
//! - **全局**（`~/.lan/preferences.json`）：`lan config` / `get` / `set locale zh`；
//! - **项目**（`lan.config.ts`）：`init` / `doctor` / `sync`，以及 `set <配置路径> <值>`。
//!
//! `set` 的派发规则是「先看是不是全局键」：全局键只有 3 个（`locale` / `log.timestamps` /
//! `output.mode`），其余一律按项目配置路径处理（写盘前用 `lania_config::known_config_fields`
//! 校验顶层字段）。这样既保住既有用法，又不会把两种语义混在一个 key 上。
mod project_config;

use std::{fs, path::PathBuf};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use lania_command::{ArgSpec, CommandContext, CommandSpec, Example, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
    plugin::{
        register_builtin_command_handlers, Plugin, PluginKind, PluginMeta, PluginSetupContext,
    },
};
use lania_logger::LogLevel;
use lania_command_config::{
    effective_value_with_source, load_main_overrides, save_main_override, ConfigValueSource,
};
use lania_package_config::{load_package_config, package_config_path, ConfigKey};
use lania_preferences::{
    load_preferences, normalize_locale, normalize_output_mode, save_preferences, UserPreferences,
};
use serde_json::json;

pub const HANDLER_ID: &str = "command.config";

/// 命令名，必须等于 `spec().name`（§5.4 D.1 冻结导出面 / §10.3 CI 断言）。
pub const COMMAND_NAME: &str = "config";
pub const GET_HANDLER_ID: &str = "command.config.get";
pub const SET_HANDLER_ID: &str = "command.config.set";
pub const INIT_HANDLER_ID: &str = "command.config.init";
pub const DOCTOR_HANDLER_ID: &str = "command.config.doctor";
pub const SYNC_HANDLER_ID: &str = "command.config.sync";
/// 子包配置管理（`lan config packages ...`）：跨包视角查看/编辑各命令子包的包级偏好。
pub const PACKAGES_HANDLER_ID: &str = "command.config.packages";
/// 一键覆盖（`lan config apply ...`）：把全局配置下发到全部已安装命令子包。
pub const APPLY_HANDLER_ID: &str = "command.config.apply";

#[derive(Debug, Default)]
pub struct ConfigCommandPlugin;

/// 命令双形态的唯一契约（§5.4 D.1）：总包 `lan` 与独立二进制 `lan-config`
/// 都用它装配同一个插件。
pub fn factory() -> Box<dyn lania_host::plugin::Plugin> {
    Box::new(ConfigCommandPlugin)
}

/// 冻结导出面：`spec()` 作为自由函数，供 `--describe` 与 known-commands.json 使用。
pub fn spec() -> lania_command::CommandSpec {
    ConfigCommandPlugin::spec()
}

struct ConfigRootHandler;
struct ConfigGetHandler;
struct ConfigSetHandler;
struct ConfigInitHandler;
struct ConfigDoctorHandler;
struct ConfigSyncHandler;
struct ConfigPackagesHandler;
struct ConfigApplyHandler;

impl ConfigCommandPlugin {
    pub fn spec() -> CommandSpec {
        CommandSpec::new("config", "Manage global CLI preferences", HANDLER_ID)
            .with_examples(vec![
                Example {
                    command: "lan config".into(),
                    description: "Show current global CLI configuration".into(),
                },
                Example {
                    command: "lan config get locale".into(),
                    description: "Read the current locale".into(),
                },
                Example {
                    command: "lan config set locale zh".into(),
                    description: "Switch CLI locale to Chinese".into(),
                },
                Example {
                    command: "lan config set log.timestamps true".into(),
                    description: "Enable timestamps for human-readable logs".into(),
                },
                Example {
                    command: "lan config set output.mode json".into(),
                    description: "Switch terminal output to JSON mode".into(),
                },
                Example {
                    command: "lan config set output.mode human".into(),
                    description: "Switch terminal output to human mode".into(),
                },
                Example {
                    command: "lan config init".into(),
                    description: "Create a lan.config.ts for the current project".into(),
                },
                Example {
                    command: "lan config doctor".into(),
                    description: "Diagnose the project lan.config (read-only)".into(),
                },
                Example {
                    command: "lan config set runtime go".into(),
                    description: "Pin the project runtime in lan.config.ts".into(),
                },
                Example {
                    command: "lan config sync".into(),
                    description: "Suggest project config entries that are missing".into(),
                },
            ])
            .with_subcommands(vec![
                Self::get_spec(),
                Self::set_spec(),
                Self::init_spec(),
                Self::doctor_spec(),
                Self::sync_spec(),
                Self::packages_spec(),
                Self::apply_spec(),
            ])
    }

    fn get_spec() -> CommandSpec {
        CommandSpec::new("get", "Read global CLI configuration", GET_HANDLER_ID)
            .with_args(vec![
                ArgSpec {
                    name: "key".into(),
                    required: false,
                    multiple: false,
                    help: "Config key: locale | log.timestamps | output.mode".into(),
                },
            ])
            .with_options(vec![OptionSpec {
                long: "package".into(),
                short: Some('p'),
                help: "Read a command subpackage's own preferences (~/.lania/config/<pkg>.json)".into(),
                value_kind: ValueKind::String,
                default_value: None,
                choices: vec![],
                negatable: false,
            }])
    }

    fn set_spec() -> CommandSpec {
        CommandSpec::new(
            "set",
            "Update global CLI configuration or the project lan.config",
            SET_HANDLER_ID,
        )
        .with_args(vec![
            ArgSpec {
                name: "key".into(),
                required: true,
                multiple: false,
                help: "Global key (locale | log.timestamps | output.mode) or project config path \
                       (runtime | lintTools | runtimes.go.build | ...)"
                    .into(),
            },
            ArgSpec {
                name: "value".into(),
                required: true,
                multiple: false,
                help: "Config value (lists accept comma-separated values or JSON)".into(),
            },
        ])
        .with_options(vec![OptionSpec {
            long: "package".into(),
            short: Some('p'),
            help: "Write a command subpackage's own preferences (~/.lania/config/<pkg>.json)".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        }])
    }

    fn init_spec() -> CommandSpec {
        CommandSpec::new(
            "init",
            "Create lan.config.ts for the current project",
            INIT_HANDLER_ID,
        )
        .with_options(vec![OptionSpec {
            long: "force".into(),
            short: Some('f'),
            help: "Overwrite lan.config.ts even when a config file already exists".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        }])
    }

    fn doctor_spec() -> CommandSpec {
        CommandSpec::new(
            "doctor",
            "Diagnose the project lan.config (read-only)",
            DOCTOR_HANDLER_ID,
        )
    }

    fn sync_spec() -> CommandSpec {
        CommandSpec::new(
            "sync",
            "Suggest (or write) the project config entries that are missing",
            SYNC_HANDLER_ID,
        )
        .with_options(vec![OptionSpec {
            long: "write".into(),
            short: None,
            help: "Write the suggestions when the config is pure data".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        }])
    }

    /// 跨包配置视图（主包统一管理面）：列出所有子包的包级配置文件。
    fn packages_spec() -> CommandSpec {
        CommandSpec::new(
            "packages",
            "Show every command subpackage's config keys and values",
            PACKAGES_HANDLER_ID,
        )
    }

    /// 一键覆盖：把全局偏好下发到子包配置文件。
    fn apply_spec() -> CommandSpec {
        CommandSpec::new(
            "apply",
            "Apply global preferences to command subpackage configs",
            APPLY_HANDLER_ID,
        )
        .with_options(vec![OptionSpec {
            long: "package".into(),
            short: Some('p'),
            help: "Only apply to one subpackage (short name or npm package name)".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        }])
    }

    fn is_zh(locale: &str) -> bool {
        locale == "zh"
    }

    fn localized<'a>(locale: &str, en: &'a str, zh: &'a str) -> &'a str {
        if Self::is_zh(locale) {
            zh
        } else {
            en
        }
    }

    fn read_arg(context: &CommandContext, locale: &str, name: &str) -> Result<String> {
        context
            .argv
            .args
            .get(name)
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    Self::localized(
                        locale,
                        &format!("missing required argument: {name}"),
                        &format!("缺少必填参数：{name}"),
                    )
                )
            })
    }

    /// 读取长选项（`--package <pkg>`），缺省返回 None。
    fn read_option(context: &CommandContext, name: &str) -> Option<String> {
        context
            .argv
            .options
            .get(name)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    /// 把用户可能写出的包名形态统一成子包配置文件的短命名空间。
    ///
    /// `@lania-cli-v2/cmd-env` / `cmd-env` / `lan-env` /
    /// `lania-plugins-command-env` / `cmd-env-win32-x64` 都归一为 `env`，
    /// 保证主包 `lan config set --package` 写的正是子包自己读取的那份文件。
    fn normalize_package_name(raw: &str) -> String {
        let mut value = raw.trim().rsplit('/').next().unwrap_or(raw.trim()).to_string();
        for prefix in ["cmd-", "lan-", "lania-plugins-command-"] {
            if let Some(rest) = value.strip_prefix(prefix) {
                value = rest.to_string();
                break;
            }
        }
        for suffix in ["-win32-x64", "-linux-x64", "-darwin-arm64"] {
            if let Some(rest) = value.strip_suffix(suffix) {
                value = rest.to_string();
                break;
            }
        }
        value.to_ascii_lowercase()
    }

    /// 子包配置读写的公共前置：先归一化包名，再做路径穿越防护校验。
    fn package_scope(context: &CommandContext) -> Result<String> {
        let raw = Self::read_option(context, "package")
            .ok_or_else(|| anyhow!("missing required option: --package <pkg>"))?;
        let pkg = Self::normalize_package_name(&raw);
        lania_package_config::validate_package_name(&pkg)?;
        Ok(pkg)
    }

    fn normalize_key(key: &str) -> Option<&'static str> {
        match key.trim() {
            "locale" => Some("locale"),
            "log.timestamp" | "log.timestamps" => Some("log.timestamps"),
            "output" | "output.mode" => Some("output.mode"),
            _ => None,
        }
    }

    fn config_output(prefs: &UserPreferences) -> serde_json::Value {
        json!({
            "kind": "config",
            "scope": "global",
            "path": lania_preferences::preferences_path().map(|p| p.display().to_string()),
            "config": {
                "locale": prefs.locale,
                "log": {
                    "timestamps": prefs.log_timestamps,
                },
                "output": {
                    "mode": prefs.output_mode,
                }
            },
            "exitCode": 0,
        })
    }

    fn config_value_output(key: &str, prefs: &UserPreferences) -> serde_json::Value {
        let value = match key {
            "locale" => json!(prefs.locale),
            "log.timestamps" => json!(prefs.log_timestamps),
            "output.mode" => json!(prefs.output_mode),
            _ => serde_json::Value::Null,
        };
        json!({
            "kind": "config_value",
            "scope": "global",
            "path": lania_preferences::preferences_path().map(|p| p.display().to_string()),
            "key": key,
            "value": value,
            "exitCode": 0,
        })
    }

    fn invalid_key_error(locale: &str, key: &str) -> anyhow::Error {
        anyhow!(
            "{}",
            Self::localized(
                locale,
                &format!("unknown config key: `{key}`"),
                &format!("未知配置项：`{key}`"),
            )
        )
    }

    fn parse_bool(locale: &str, raw: &str) -> Result<bool> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => bail!(
                "{}",
                Self::localized(
                    locale,
                    "invalid boolean value; expected true/false",
                    "布尔值无效，需要传入 true/false",
                )
            ),
        }
    }

    fn apply_config_value(
        locale: &str,
        prefs: &mut UserPreferences,
        key: &str,
        value: &str,
    ) -> Result<&'static str> {
        match Self::normalize_key(key) {
            Some("locale") => {
                prefs.locale = normalize_locale(value);
                Ok("locale")
            }
            Some("log.timestamps") => {
                prefs.log_timestamps = Self::parse_bool(locale, value)?;
                Ok("log.timestamps")
            }
            Some("output.mode") => {
                prefs.output_mode = normalize_output_mode(value);
                Ok("output.mode")
            }
            _ => Err(Self::invalid_key_error(locale, key)),
        }
    }
}

impl Plugin for ConfigCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-config".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![CapabilityName::Logger],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command_handlers(
            ctx,
            "command-config",
            "config",
            Self::spec(),
            vec![
                (HANDLER_ID, Box::new(ConfigRootHandler)),
                (GET_HANDLER_ID, Box::new(ConfigGetHandler)),
                (SET_HANDLER_ID, Box::new(ConfigSetHandler)),
                (INIT_HANDLER_ID, Box::new(ConfigInitHandler)),
                (DOCTOR_HANDLER_ID, Box::new(ConfigDoctorHandler)),
                (SYNC_HANDLER_ID, Box::new(ConfigSyncHandler)),
                (PACKAGES_HANDLER_ID, Box::new(ConfigPackagesHandler)),
                (APPLY_HANDLER_ID, Box::new(ConfigApplyHandler)),
            ],
        )
    }
}

#[async_trait(?Send)]
impl CommandHandler for ConfigRootHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let prefs = load_preferences();
        let output = ConfigCommandPlugin::config_output(&prefs);
        Ok(ctx.complete_template_info(output, 0))
    }
}

#[async_trait(?Send)]
impl CommandHandler for ConfigGetHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        // `--package <pkg>`：主包与子包配置文件隔离——这里只**读**，输出生效值与来源。
        // 生效优先级：主包 preferences 的 packages.<pkg> 覆盖段 → 子包自身
        // ~/.lania/config/<pkg>.json → key 默认值。主包从不写子包文件。
        if ConfigCommandPlugin::read_option(ctx.command(), "package").is_some() {
            let pkg = ConfigCommandPlugin::package_scope(ctx.command())?;
            let config = load_package_config(&pkg)?;
            let path = package_config_path(&pkg)?;
            let overrides = load_main_overrides(&pkg);
            let raw_key = ctx
                .command()
                .argv
                .args
                .get("key")
                .and_then(|value| value.as_str());
            let keys: Vec<ConfigKey> = package_config_keys(&pkg)
                .into_iter()
                .filter(|item| {
                    raw_key.map_or(true, |raw| {
                        item.key == raw
                            || (raw == "log.timestamp" && item.key == "log.timestamps")
                            || (raw == "output" && item.key == "output.mode")
                    })
                })
                .collect();
            let mut rows = Vec::new();
            for key_spec in &keys {
                let (value, source) =
                    effective_value_with_source(&config, &overrides, key_spec);
                rows.push(json!({
                    "key": key_spec.key,
                    "value": value,
                    "source": source.as_str(),
                }));
            }
            let output = json!({
                "kind": "config_get",
                "scope": "package",
                "package": pkg,
                "path": path.map(|p| p.display().to_string()),
                "key": raw_key,
                "effective": rows,
                "overrides": overrides,
                "packageFile": config,
            });
            return Ok(ctx.complete_template_info(output, 0));
        }

        let prefs = load_preferences();
        let output = if let Some(raw_key) = ctx
            .command()
            .argv
            .args
            .get("key")
            .and_then(|value| value.as_str())
        {
            let key = ConfigCommandPlugin::normalize_key(raw_key)
                .ok_or_else(|| ConfigCommandPlugin::invalid_key_error(ctx.locale(), raw_key))?;
            ConfigCommandPlugin::config_value_output(key, &prefs)
        } else {
            ConfigCommandPlugin::config_output(&prefs)
        };
        Ok(ctx.complete_template_info(output, 0))
    }
}

#[async_trait(?Send)]
impl CommandHandler for ConfigSetHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let key = ConfigCommandPlugin::read_arg(ctx.command(), ctx.locale(), "key")?;
        let value = ConfigCommandPlugin::read_arg(ctx.command(), ctx.locale(), "value")?;

        // `--package <pkg>`：主包与子包配置文件**隔离**——主包只能把覆盖值写进
        // 自己的 preferences.json（packages.<pkg>.<key> 段），绝不写子包文件；
        // 子包运行时经 load_main_overrides 优先读到这份覆盖。
        if ConfigCommandPlugin::read_option(ctx.command(), "package").is_some() {
            let pkg = ConfigCommandPlugin::package_scope(ctx.command())?;
            // 与各命令子包 manifest 的 configSchema 同构校验：只接受声明过的键，
            // 并按 enum / bool 类型解析，避免把任意字符串写进覆盖段。
            let key_spec = package_config_keys(&pkg)
                .into_iter()
                .find(|item| {
                    item.key == key
                        || (key == "log.timestamp" && item.key == "log.timestamps")
                        || (key == "output" && item.key == "output.mode")
                })
                .ok_or_else(|| {
                    anyhow!(
                        "unknown package config key `{key}`; run `lan config packages` to see declared keys"
                    )
                })?;
            let parsed = key_spec.parse(&value).map_err(|error| anyhow!("{error}"))?;
            save_main_override(&pkg, &key_spec.key, parsed.clone())?;
            let path = lania_preferences::preferences_path();
            let output = json!({
                "kind": "config_set",
                "scope": "main-override",
                "package": pkg,
                "path": path.map(|p| p.display().to_string()),
                "key": key_spec.key,
                "value": parsed,
            });
            return Ok(ctx.complete_template_info(output, 0));
        }

        // 先看是不是全局键（只有 3 个）；否则按项目配置路径处理。
        let Some(normalized_key) = ConfigCommandPlugin::normalize_key(&key) else {
            return set_project_config(ctx, &key, &value);
        };
        let mut prefs = load_preferences();
        ConfigCommandPlugin::apply_config_value(ctx.locale(), &mut prefs, normalized_key, &value)?;
        save_preferences(&prefs)?;
        let output = ConfigCommandPlugin::config_value_output(normalized_key, &prefs);
        Ok(ctx.complete_template_info(output, 0))
    }
}

/// 所有命令子包共用的包级配置键（与各自 manifest 的 `configSchema` 对齐）。
/// 主包 `--package` 写入只接受这些键，避免任意 key 污染子包配置文件。
fn package_config_keys(pkg: &str) -> Vec<ConfigKey> {
    let mut keys = vec![
        ConfigKey::enumeration("locale", &["zh", "en"], "en"),
        ConfigKey::enumeration("output.mode", &["json", "human"], "json"),
        ConfigKey::boolean("log.timestamps", false),
    ];
    if pkg == "env" {
        keys.push(ConfigKey::enumeration("shim.host", &["lan", "runex"], "lan"));
    }
    keys
}

/// 独立二进制形态下的子包清单：扫描 `~/.lania/config/*.json`。
/// `only` 指定时直接返回该包（文件尚未存在也返回，便于首次创建）。
/// launcher 形态（`lan config packages/apply`）改由 known-commands.json
/// 静态表覆盖全部已安装子包，两条路径写入的是同一份配置文件。
fn package_names(only: Option<&str>) -> Vec<String> {
    if let Some(name) = only {
        return vec![name.to_string()];
    }
    let Ok(Some(dir)) = package_config_path("scan")
        .map(|path| path.and_then(|p| p.parent().map(PathBuf::from)))
    else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_string_lossy().into_owned();
            (path.extension().and_then(|ext| ext.to_str()) == Some("json")
                && lania_package_config::validate_package_name(&stem).is_ok())
            .then_some(stem)
        })
        .collect();
    names.sort();
    names
}

/// 命令的 cwd（`ProjectView` 只借用它，所以必须由调用方持有）。
fn command_cwd(ctx: &CommandExecutionContext<'_>) -> PathBuf {
    let raw = &ctx.command().cwd;
    PathBuf::from(if raw.is_empty() { "." } else { raw.as_str() })
}

/// `lan config set <项目配置路径> <值>`（§6.5.8：只有纯数据配置才写盘）。
fn set_project_config(
    ctx: &CommandExecutionContext<'_>,
    key: &str,
    raw_value: &str,
) -> Result<CommandExecution> {
    let cwd = command_cwd(ctx);
    let view = project_config::ProjectView::new(&cwd, ctx.project_config());
    let value = project_config::parse_value(key, raw_value);
    let plan = project_config::plan_set(&view, key, value.clone()).map_err(|error| {
        // 既不是全局键、也不是项目配置字段：把两类合法取值都列出来，避免“猜”。
        anyhow!("{error}\nglobal keys: locale, log.timestamps, output.mode")
    })?;

    let mut output = json!({
        "kind": "config_set",
        "scope": "project",
        "path": plan.path.display().to_string(),
        "key": key,
        "value": value,
        "written": false,
        "pureData": plan.writable,
    });
    if plan.writable {
        project_config::write_config(&plan.path, &plan.contents)?;
        output["written"] = json!(true);
    } else {
        // 非纯数据：给出完整建议文件内容，用户手动粘贴（§6.5.8 的硬约束）。
        output["suggestion"] = json!(plan.contents);
        output["reason"] = json!("config uses functions/imports; not rewritten");
    }
    Ok(ctx.complete_template_info(output, 0))
}

#[async_trait(?Send)]
impl CommandHandler for ConfigInitHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let cwd = command_cwd(ctx);
        let view = project_config::ProjectView::new(&cwd, ctx.project_config());
        let plan = project_config::plan_init(&view, ctx.bool_option("force"));
        let mut output = json!({
            "kind": "config_init",
            "scope": "project",
            "path": plan.path.display().to_string(),
            "written": false,
            "mode": plan.mode,
            "runtime": plan.runtime.id(),
            "existing": plan
                .existing
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
        });
        if plan.written {
            project_config::write_config(&plan.path, &plan.contents)?;
            output["written"] = json!(true);
        } else {
            output["reason"] = json!("exists");
            output["suggestion"] = json!(plan.contents);
        }
        Ok(ctx.complete_template_info(output, 0))
    }
}

#[async_trait(?Send)]
impl CommandHandler for ConfigDoctorHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let cwd = command_cwd(ctx);
        let view = project_config::ProjectView::new(&cwd, ctx.project_config());
        let (report, issues) = project_config::doctor_report(&view);
        // 只读诊断：问题同时走 logger，人在终端里能直接看到（JSON 里也有结构化副本）。
        let logger = ctx.workflow_services().logger.scoped("command.config");
        for issue in &issues {
            let level = if issue.severity == "error" {
                LogLevel::Warn
            } else {
                LogLevel::Info
            };
            logger.log(
                level,
                format!("config doctor [{}]: {}", issue.code, issue.message),
            );
        }
        Ok(ctx.complete_template_info(report, 0))
    }
}

#[async_trait(?Send)]
impl CommandHandler for ConfigSyncHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let cwd = command_cwd(ctx);
        let view = project_config::ProjectView::new(&cwd, ctx.project_config());
        let plan = project_config::plan_sync(&view);
        let should_write =
            ctx.bool_option("write") && plan.writable && !plan.suggestions.is_empty();
        let mut output = json!({
            "kind": "config_sync",
            "scope": "project",
            "path": plan.path.display().to_string(),
            "written": false,
            "writable": plan.writable,
            "patched": plan.patched,
            "suggestions": plan
                .suggestions
                .iter()
                .map(|suggestion| json!({
                    "key": suggestion.key,
                    "value": suggestion.value,
                    "reason": suggestion.reason,
                }))
                .collect::<Vec<_>>(),
            "snippet": plan.snippet,
        });
        if should_write {
            project_config::write_config(&plan.path, &plan.snippet)?;
            output["written"] = json!(true);
        } else if !plan.writable && !plan.suggestions.is_empty() {
            output["reason"] = json!("config uses functions/imports; not rewritten");
        }
        Ok(ctx.complete_template_info(output, 0))
    }
}

#[async_trait(?Send)]
impl CommandHandler for ConfigPackagesHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let only = ConfigCommandPlugin::read_option(ctx.command(), "package")
            .as_deref()
            .map(ConfigCommandPlugin::normalize_package_name);
        let rows: Vec<serde_json::Value> = package_names(only.as_deref())
            .iter()
            .map(|pkg| {
                let config = load_package_config(pkg).unwrap_or_default();
                json!({
                    "package": pkg,
                    "path": package_config_path(pkg)
                        .ok()
                        .flatten()
                        .map(|p| p.display().to_string()),
                    "keys": package_config_keys(pkg),
                    "config": config,
                    "overrides": load_main_overrides(pkg),
                })
            })
            .collect();
        Ok(ctx.complete_template_info(
            json!({
                "kind": "config_packages",
                "scope": "packages",
                "packages": rows,
            }),
            0,
        ))
    }
}

#[async_trait(?Send)]
impl CommandHandler for ConfigApplyHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let only = ConfigCommandPlugin::read_option(ctx.command(), "package")
            .as_deref()
            .map(ConfigCommandPlugin::normalize_package_name);
        let prefs = load_preferences();
        let targets = package_names(only.as_deref());
        let mut applied = Vec::new();
        for pkg in &targets {
            // 隔离模型：apply 只把全局值写进主包覆盖段（packages.<pkg>），
            // 不再写子包配置文件。
            for key in package_config_keys(pkg) {
                if key.key == "shim.host" {
                    continue;
                }
                let value = match key.key.as_str() {
                    "locale" => json!(prefs.locale),
                    "output.mode" => json!(prefs.output_mode),
                    "log.timestamps" => json!(prefs.log_timestamps),
                    _ => continue,
                };
                save_main_override(pkg, &key.key, value)?;
            }
            applied.push(pkg.clone());
        }
        Ok(ctx.complete_template_info(
            json!({
                "kind": "config_apply",
                "scope": "packages",
                "applied": applied,
                "hint": if applied.is_empty() {
                    "no subpackage config files found; use `lan config apply` to cover every installed subpackage"
                } else {
                    ""
                },
            }),
            0,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigCommandPlugin;
    use lania_preferences::UserPreferences;

    // ── is_zh ─────────────────────────────────────────────────

    #[test]
    fn is_zh_returns_true_for_zh() {
        assert!(ConfigCommandPlugin::is_zh("zh"));
    }

    #[test]
    fn is_zh_returns_false_for_en() {
        assert!(!ConfigCommandPlugin::is_zh("en"));
    }

    // ── localized ─────────────────────────────────────────────

    #[test]
    fn localized_returns_zh_when_locale_is_zh() {
        assert_eq!(
            ConfigCommandPlugin::localized("zh", "hello", "你好"),
            "你好"
        );
    }

    #[test]
    fn localized_returns_en_when_locale_is_not_zh() {
        assert_eq!(
            ConfigCommandPlugin::localized("en", "hello", "你好"),
            "hello"
        );
    }

    // ── normalize_key ─────────────────────────────────────────

    #[test]
    fn normalize_key_returns_known_keys() {
        assert_eq!(ConfigCommandPlugin::normalize_key("locale"), Some("locale"));
        assert_eq!(
            ConfigCommandPlugin::normalize_key("log.timestamps"),
            Some("log.timestamps")
        );
        assert_eq!(
            ConfigCommandPlugin::normalize_key("log.timestamp"),
            Some("log.timestamps")
        );
        assert_eq!(
            ConfigCommandPlugin::normalize_key("output.mode"),
            Some("output.mode")
        );
        assert_eq!(
            ConfigCommandPlugin::normalize_key("output"),
            Some("output.mode")
        );
    }

    #[test]
    fn normalize_key_returns_none_for_unknown() {
        assert_eq!(ConfigCommandPlugin::normalize_key("unknown"), None);
        assert_eq!(ConfigCommandPlugin::normalize_key(""), None);
        assert_eq!(ConfigCommandPlugin::normalize_key("  "), None);
    }

    // ── parse_bool ────────────────────────────────────────────

    #[test]
    fn parse_bool_accepts_true_values() {
        assert!(ConfigCommandPlugin::parse_bool("en", "true").unwrap());
        assert!(ConfigCommandPlugin::parse_bool("en", "1").unwrap());
        assert!(ConfigCommandPlugin::parse_bool("zh", "yes").unwrap());
        assert!(ConfigCommandPlugin::parse_bool("en", "on").unwrap());
    }

    #[test]
    fn parse_bool_accepts_false_values() {
        assert!(!ConfigCommandPlugin::parse_bool("en", "false").unwrap());
        assert!(!ConfigCommandPlugin::parse_bool("en", "0").unwrap());
        assert!(!ConfigCommandPlugin::parse_bool("zh", "no").unwrap());
        assert!(!ConfigCommandPlugin::parse_bool("en", "off").unwrap());
    }

    #[test]
    fn parse_bool_rejects_invalid_values() {
        assert!(ConfigCommandPlugin::parse_bool("en", "maybe").is_err());
        assert!(ConfigCommandPlugin::parse_bool("en", "").is_err());
    }

    // ── apply_config_value ────────────────────────────────────

    #[test]
    fn apply_config_value_sets_locale() {
        let mut prefs = UserPreferences::default();
        ConfigCommandPlugin::apply_config_value("en", &mut prefs, "locale", "zh-CN").unwrap();
        assert_eq!(prefs.locale, "zh");
    }

    #[test]
    fn apply_config_value_sets_log_timestamps() {
        let mut prefs = UserPreferences::default();
        ConfigCommandPlugin::apply_config_value("en", &mut prefs, "log.timestamps", "true")
            .unwrap();
        assert!(prefs.log_timestamps);
    }

    #[test]
    fn apply_config_value_sets_output_mode() {
        let mut prefs = UserPreferences::default();
        ConfigCommandPlugin::apply_config_value("en", &mut prefs, "output.mode", "human").unwrap();
        assert_eq!(prefs.output_mode, "human");
    }

    #[test]
    fn apply_config_value_rejects_unknown_key() {
        let mut prefs = UserPreferences::default();
        let result = ConfigCommandPlugin::apply_config_value("en", &mut prefs, "unknown", "val");
        assert!(result.is_err());
    }

    #[test]
    fn apply_config_value_rejects_invalid_bool() {
        let mut prefs = UserPreferences::default();
        let result =
            ConfigCommandPlugin::apply_config_value("en", &mut prefs, "log.timestamps", "bad");
        assert!(result.is_err());
    }

    // ── invalid_key_error ─────────────────────────────────────

    #[test]
    fn invalid_key_error_includes_key_name() {
        let err = ConfigCommandPlugin::invalid_key_error("en", "foo");
        let msg = format!("{err}");
        assert!(msg.contains("foo"));
    }

    #[test]
    fn invalid_key_error_is_localized_in_zh() {
        let err = ConfigCommandPlugin::invalid_key_error("zh", "foo");
        let msg = format!("{err}");
        assert!(msg.contains("未知配置项"));
    }

    // ── config_output ─────────────────────────────────────────

    #[test]
    fn config_output_contains_config_section() {
        let prefs = UserPreferences::default();
        let output = ConfigCommandPlugin::config_output(&prefs);
        assert_eq!(output["kind"], "config");
        assert_eq!(output["config"]["locale"], "en");
        assert_eq!(output["config"]["output"]["mode"], "json");
    }
}


#[cfg(test)]
mod frozen_export_surface {
    //! 冻结导出面守卫（§5.4 D.1 / §10.3 CI 断言）。
    use super::*;

    #[test]
    fn command_name_matches_spec() {
        assert_eq!(spec().name, COMMAND_NAME);
    }

    #[test]
    fn handler_id_is_frozen() {
        assert_eq!(spec().handler_id, HANDLER_ID);
    }

    #[test]
    fn factory_builds_the_plugin() {
        let _plugin = factory();
    }
}
