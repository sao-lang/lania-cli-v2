//! `tools list` 的命令规范、bridge 请求构建与结果整形逻辑。

use anyhow::{anyhow, Result};
use lania_command::{CommandContext, CommandSpec, OptionSpec, ValueKind};
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext},
};
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::{ToolsCommandPlugin, LIST_HANDLER_ID};

pub(super) fn options() -> Vec<OptionSpec> {
    vec![
        OptionSpec {
            long: "filter".into(),
            short: Some('f'),
            help: "Filter command names by substring".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "limit".into(),
            short: Some('n'),
            help: "Limit the number of returned commands".into(),
            value_kind: ValueKind::Number,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "shell".into(),
            short: None,
            help: "Include shell builtins, aliases, and functions".into(),
            value_kind: ValueKind::Bool,
            default_value: Some("true".into()),
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "all-matches".into(),
            short: Some('a'),
            help: "Show every PATH match for duplicate command names".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "names-only".into(),
            short: None,
            help: "Return only command names instead of detailed entries".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "group-by-source".into(),
            short: Some('g'),
            help: "Group commands by PATH, shell builtin, alias, or function".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "plain".into(),
            short: Some('p'),
            help: "Render a plain text list instead of structured command entries".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
        OptionSpec {
            long: "unique".into(),
            short: Some('u'),
            help: "Deduplicate command names while preserving their first match order".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: true,
        },
    ]
}

pub(super) fn spec() -> CommandSpec {
    CommandSpec::new("list", "List terminal-resolvable commands", LIST_HANDLER_ID)
        .with_options(options())
}

pub(super) fn build_request(context: &CommandContext, bridge: &NodeBridgeClient) -> BridgeRequest {
    let filter = context
        .argv
        .options
        .get("filter")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let limit = context
        .argv
        .options
        .get("limit")
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok());
    let all_matches = context
        .argv
        .options
        .get("all-matches")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let include_shell = context
        .argv
        .options
        .get("shell")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    bridge.system_list_commands_request(
        context.cwd.clone(),
        filter,
        limit,
        all_matches,
        include_shell,
    )
}

pub(super) async fn execute(ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
    ctx.require_capability(CapabilityName::NodeBridge)?;
    let request = ToolsCommandPlugin::build_request(ctx.command(), ctx.node_bridge());
    let run = ctx.call_bridge(request).await?;
    let mut result = run
        .exchange
        .response
        .result
        .clone()
        .ok_or_else(|| anyhow!("system.listCommands returned no payload"))?;
    let names_only = ctx
        .command()
        .argv
        .options
        .get("names-only")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let group_by_source = ctx
        .command()
        .argv
        .options
        .get("group-by-source")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let plain = ctx
        .command()
        .argv
        .options
        .get("plain")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let unique = ctx
        .command()
        .argv
        .options
        .get("unique")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    result = transform_result(result, names_only, group_by_source, plain, unique);
    Ok(ctx.complete_template_info(result, 0))
}

fn transform_result(
    mut result: Value,
    names_only: bool,
    group_by_source: bool,
    plain: bool,
    unique: bool,
) -> Value {
    // Keep flag application order stable so combined options compose predictably.
    if group_by_source {
        result = group_commands_by_source(result);
    }
    if unique {
        result = dedupe_result_by_name(result);
    }
    if names_only {
        result = keep_names_only(result);
    }
    if plain {
        result = render_plain_result(result);
    }
    result
}

fn dedupe_result_by_name(result: Value) -> Value {
    match result.get("kind").and_then(Value::as_str).unwrap_or("") {
        "system_commands" => dedupe_system_commands(result),
        "system_command_groups" => dedupe_grouped_commands(result),
        "system_command_names" => dedupe_name_list(result),
        _ => result,
    }
}

fn dedupe_system_commands(mut result: Value) -> Value {
    let mut seen = HashSet::new();
    if let Some(commands) = result.get_mut("commands").and_then(Value::as_array_mut) {
        commands.retain(|command| {
            command
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| seen.insert(name.to_string()))
        });
    }
    result
}

fn dedupe_grouped_commands(mut result: Value) -> Value {
    let mut seen = HashSet::new();
    if let Some(groups) = result.get_mut("groups").and_then(Value::as_object_mut) {
        for (key, _) in ordered_group_keys_with_titles() {
            if let Some(items) = groups.get_mut(key).and_then(Value::as_array_mut) {
                items.retain(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| seen.insert(name.to_string()))
                });
            }
        }
    }
    result
}

fn dedupe_name_list(mut result: Value) -> Value {
    let mut seen = HashSet::new();
    if let Some(names) = result.get_mut("names").and_then(Value::as_array_mut) {
        names.retain(|name| {
            name.as_str()
                .is_some_and(|item| seen.insert(item.to_string()))
        });
    }
    result
}

fn keep_names_only(result: Value) -> Value {
    let names = collect_names(&result);
    json!({
        "accepted": result["accepted"],
        "kind": "system_command_names",
        "scope": result["scope"],
        "platform": result["platform"],
        "shell": result["shell"],
        "shellName": result["shellName"],
        "shellSupported": result["shellSupported"],
        "includeShell": result["includeShell"],
        "cwd": result["cwd"],
        "filter": result["filter"],
        "limit": result["limit"],
        "allMatches": result["allMatches"],
        "summary": result["summary"],
        "names": names,
    })
}

fn group_commands_by_source(result: Value) -> Value {
    let mut path = Vec::new();
    let mut shell_builtin = Vec::new();
    let mut shell_alias = Vec::new();
    let mut shell_function = Vec::new();

    if let Some(commands) = result.get("commands").and_then(Value::as_array) {
        for command in commands {
            match command
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("PATH")
            {
                "PATH" => path.push(command.clone()),
                "shell_builtin" => shell_builtin.push(command.clone()),
                "shell_alias" => shell_alias.push(command.clone()),
                "shell_function" => shell_function.push(command.clone()),
                _ => path.push(command.clone()),
            }
        }
    }

    json!({
        "accepted": result["accepted"],
        "kind": "system_command_groups",
        "scope": result["scope"],
        "platform": result["platform"],
        "shell": result["shell"],
        "shellName": result["shellName"],
        "shellSupported": result["shellSupported"],
        "includeShell": result["includeShell"],
        "cwd": result["cwd"],
        "filter": result["filter"],
        "limit": result["limit"],
        "allMatches": result["allMatches"],
        "summary": result["summary"],
        "groups": {
            "path": path,
            "shell_builtin": shell_builtin,
            "shell_alias": shell_alias,
            "shell_function": shell_function,
        },
        "duplicates": result["duplicates"],
    })
}

fn render_plain_result(result: Value) -> Value {
    let text = match result.get("kind").and_then(Value::as_str).unwrap_or("") {
        "system_command_groups" => render_plain_groups(&result),
        "system_command_names" => collect_names(&result).join("\n"),
        _ => collect_names(&result).join("\n"),
    };

    json!({
        "accepted": result["accepted"],
        "kind": "system_command_plain",
        "scope": result["scope"],
        "platform": result["platform"],
        "shell": result["shell"],
        "shellName": result["shellName"],
        "shellSupported": result["shellSupported"],
        "includeShell": result["includeShell"],
        "cwd": result["cwd"],
        "filter": result["filter"],
        "limit": result["limit"],
        "allMatches": result["allMatches"],
        "summary": result["summary"],
        "text": text,
    })
}

fn render_plain_groups(result: &Value) -> String {
    let mut sections = Vec::new();
    for (key, title) in ordered_group_keys_with_titles() {
        let names = collect_group_names(result, key);
        if names.is_empty() {
            continue;
        }
        sections.push(format!("{title}\n{}", names.join("\n")));
    }
    sections.join("\n\n")
}

fn collect_names(result: &Value) -> Vec<String> {
    if let Some(names) = result.get("names").and_then(Value::as_array) {
        return names
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect();
    }
    if let Some(commands) = result.get("commands").and_then(Value::as_array) {
        return commands
            .iter()
            .filter_map(|item| {
                item.get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect();
    }
    if result.get("groups").and_then(Value::as_object).is_some() {
        let mut names = Vec::new();
        for (key, _) in ordered_group_keys_with_titles() {
            names.extend(collect_group_names(result, key));
        }
        return names;
    }
    Vec::new()
}

fn collect_group_names(result: &Value, key: &str) -> Vec<String> {
    result
        .get("groups")
        .and_then(|groups| groups.get(key))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .map(|name| name.to_string())
                })
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

fn ordered_group_keys_with_titles() -> [(&'static str, &'static str); 4] {
    [
        ("path", "PATH"),
        ("shell_builtin", "Shell Builtins"),
        ("shell_alias", "Shell Aliases"),
        ("shell_function", "Shell Functions"),
    ]
}

#[cfg(test)]
mod tests {
    use lania_node_bridge::{BridgeClientConfig, NodeBridgeClient};
    use serde_json::{json, Value};

    use super::transform_result;
    use crate::{ToolsCommandPlugin, HANDLER_ID};

    #[test]
    fn builds_tools_bridge_request_from_context() {
        let context = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("filter".into(), json!("ts")),
                    ("limit".into(), json!(25)),
                    ("all-matches".into(), json!(true)),
                    ("shell".into(), json!(false)),
                    ("names-only".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: HANDLER_ID.into(),
            trace_id: "trace-tools".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = ToolsCommandPlugin::build_request(&context, &bridge);

        assert_eq!(request.method, "system.listCommands");
        assert_eq!(request.params["cwd"], "/repo");
        assert_eq!(request.params["filter"], "ts");
        assert_eq!(request.params["limit"], 25);
        assert_eq!(request.params["allMatches"], true);
        assert_eq!(request.params["includeShell"], false);
    }

    #[test]
    fn transforms_result_into_grouped_and_plain_shapes() {
        let source = json!({
            "accepted": true,
            "kind": "system_commands",
            "scope": "environment",
            "platform": "darwin",
            "shell": "/bin/zsh",
            "shellName": "zsh",
            "shellSupported": true,
            "includeShell": true,
            "cwd": "/repo",
            "filter": Value::Null,
            "limit": Value::Null,
            "allMatches": false,
            "summary": { "returned": 4 },
            "commands": [
                { "name": "node", "source": "PATH" },
                { "name": "cd", "source": "shell_builtin" },
                { "name": "gst", "source": "shell_alias" },
                { "name": "mkcd", "source": "shell_function" }
            ],
            "duplicates": []
        });

        let grouped = transform_result(source.clone(), false, true, false, false);
        assert_eq!(grouped["kind"], "system_command_groups");
        assert_eq!(grouped["groups"]["path"][0]["name"], "node");
        assert_eq!(grouped["groups"]["shell_builtin"][0]["name"], "cd");

        let names_only = transform_result(source.clone(), true, false, false, false);
        assert_eq!(names_only["kind"], "system_command_names");
        assert_eq!(names_only["names"], json!(["node", "cd", "gst", "mkcd"]));

        let plain = transform_result(source, false, true, true, false);
        assert_eq!(plain["kind"], "system_command_plain");
        assert!(plain["text"].as_str().unwrap_or_default().contains("PATH"));
        assert!(plain["text"]
            .as_str()
            .unwrap_or_default()
            .contains("Shell Aliases"));

        let plain_names_only = transform_result(grouped, true, false, true, false);
        assert_eq!(plain_names_only["kind"], "system_command_plain");
        assert_eq!(plain_names_only["text"], json!("node\ncd\ngst\nmkcd"));
    }

    #[test]
    fn deduplicates_names_while_preserving_first_seen_order() {
        let source = json!({
            "accepted": true,
            "kind": "system_commands",
            "scope": "environment",
            "platform": "darwin",
            "shell": "/bin/zsh",
            "shellName": "zsh",
            "shellSupported": true,
            "includeShell": true,
            "cwd": "/repo",
            "filter": Value::Null,
            "limit": Value::Null,
            "allMatches": true,
            "summary": { "returned": 5 },
            "commands": [
                { "name": "node", "source": "PATH" },
                { "name": "tsc", "source": "PATH" },
                { "name": "node", "source": "shell_alias" },
                { "name": "tsc", "source": "shell_function" },
                { "name": "tsx", "source": "PATH" }
            ],
            "duplicates": []
        });

        let names_only = transform_result(source.clone(), true, false, false, true);
        assert_eq!(names_only["names"], json!(["node", "tsc", "tsx"]));

        let grouped_plain = transform_result(source, true, true, true, true);
        assert_eq!(grouped_plain["kind"], "system_command_plain");
        assert_eq!(grouped_plain["text"], json!("node\ntsc\ntsx"));
    }
}
