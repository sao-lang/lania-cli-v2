//! `config` command: manage global CLI preferences.
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use lania_command::{ArgSpec, CommandContext, CommandSpec, Example};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
    plugin::{
        register_builtin_command_handlers, Plugin, PluginKind, PluginMeta, PluginSetupContext,
    },
};
use lania_preferences::{
    load_preferences, normalize_locale, normalize_output_mode, save_preferences, UserPreferences,
};
use serde_json::json;

pub const HANDLER_ID: &str = "command.config";
pub const GET_HANDLER_ID: &str = "command.config.get";
pub const SET_HANDLER_ID: &str = "command.config.set";

#[derive(Debug, Default)]
pub struct ConfigCommandPlugin;

struct ConfigRootHandler;
struct ConfigGetHandler;
struct ConfigSetHandler;

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
                    command: "lan config set output.mode stream".into(),
                    description: "Switch terminal output to stream mode".into(),
                },
                Example {
                    command: "lan config set output.mode human".into(),
                    description: "Switch terminal output to human mode".into(),
                },
            ])
            .with_subcommands(vec![Self::get_spec(), Self::set_spec()])
    }

    fn get_spec() -> CommandSpec {
        CommandSpec::new("get", "Read global CLI configuration", GET_HANDLER_ID).with_args(vec![
            ArgSpec {
                name: "key".into(),
                required: false,
                multiple: false,
                help: "Config key: locale | log.timestamps | output.mode".into(),
            },
        ])
    }

    fn set_spec() -> CommandSpec {
        CommandSpec::new("set", "Update global CLI configuration", SET_HANDLER_ID).with_args(vec![
            ArgSpec {
                name: "key".into(),
                required: true,
                multiple: false,
                help: "Config key: locale | log.timestamps | output.mode".into(),
            },
            ArgSpec {
                name: "value".into(),
                required: true,
                multiple: false,
                help: "Config value".into(),
            },
        ])
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
        let mut prefs = load_preferences();
        let normalized_key =
            ConfigCommandPlugin::apply_config_value(ctx.locale(), &mut prefs, &key, &value)?;
        save_preferences(&prefs)?;
        let output = ConfigCommandPlugin::config_value_output(normalized_key, &prefs);
        Ok(ctx.complete_template_info(output, 0))
    }
}
