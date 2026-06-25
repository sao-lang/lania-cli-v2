//! `CommandSpec` <-> clap 的映射与 argv 解析。
//!
//! 这层的职责是把运行时收集到的 `CommandSpec` 转成 clap 命令树，并在 clap 解析后：
//! - 还原出“用户真正调用的子命令路径”（`Vec<String>`）
//! - 收集位置参数与选项参数，产出 `ParsedArgv`
//! - 根据路径找到最终要执行的 `handler_id`，构造 `CommandContext`
//!
//! 额外约定：
//! - `--no-<flag>` 只对 `negatable=true` 的 bool option 生效，并会被规范化成 `<flag>=false`。
//! - `help/version` 等内建命令绕过 clap 的 matches 解析，直接输出文本/JSON。

use std::io::Cursor;

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{json, Value};

use crate::{ArgSpec, CommandContext, CommandSpec, ParsedArgv, ValueKind};

pub struct BuiltinCommandOutput {
    pub output: String,
    pub exit_code: i32,
}

pub fn build_cli(
    binary_name: &'static str,
    about: &'static str,
    version: &'static str,
    commands: &[CommandSpec],
    locale: &str,
) -> Command {
    // 注意：这里仅根据 specs 构建“命令树结构”，并不做任何 handler 绑定。
    let mut root = Command::new(binary_name)
        .about(about)
        .version(version)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .disable_help_flag(true)
        .subcommand_required(false)
        .help_template(help_template(locale));
    root = root
        .subcommand_help_heading(if locale == "zh" { "命令" } else { "Commands" })
        .arg(version_arg(locale))
        .arg(help_arg(locale));
    // 每个 `CommandSpec` 都会递归转换成一个 clap `Command`。
    // 这一步只负责“把命令树长出来”，真正的 handler 绑定在 host runtime 的 registry 中。
    for spec in commands {
        root = root.subcommand(to_clap_command(spec, locale));
    }
    root
}

pub fn apply_legacy_aliases(commands: &mut [CommandSpec]) {
    for command in commands {
        apply_legacy_aliases_to_spec(command, &[]);
    }
}

pub fn render_builtin_command(
    binary_name: &'static str,
    about: &'static str,
    version: &'static str,
    commands: &[CommandSpec],
    args: &[String],
    runtime_error_exit_code: i32,
    locale: &str,
) -> Option<BuiltinCommandOutput> {
    // 这里的输出是给二进制入口直接打印的，因此要包含末尾换行。
    let command = detect_builtin_command(args)?;
    match command {
        BuiltinCommand::Version => Some(BuiltinCommandOutput {
            output: format!("{version}\n"),
            exit_code: 0,
        }),
        BuiltinCommand::Help { path } => Some(render_help_command(
            binary_name,
            about,
            version,
            commands,
            &path,
            runtime_error_exit_code,
            locale,
        )),
    }
}

pub fn command_context_from_matches(
    commands: &[CommandSpec],
    matches: &ArgMatches,
    cwd: impl Into<String>,
    trace_id: impl Into<String>,
) -> Option<CommandContext> {
    // `path` 是子命令链路，例如：`["generate", "api", "plan"]`。
    let (path, argv) = parse_subcommand_with_spec(commands, matches)?;
    // handler_id 查找基于“子命令路径”，而不是 clap 的内部结构。
    // 这样动态命令注入后，只要 `CommandSpec` 变了，这里也能自然跟着工作。
    let handler_id = lookup_handler_id(commands, &path)?;
    Some(CommandContext {
        cwd: cwd.into(),
        argv,
        handler_id,
        trace_id: trace_id.into(),
    })
}

fn to_clap_command(spec: &CommandSpec, locale: &str) -> Command {
    // clap 的 `Command::new`/`Arg::new` 通常要求 `'static` 字符串引用。
    // 这里用 `Box::leak` 把 `String` 变成 `'static`，代价是：这部分内存不会释放。
    // 对 CLI 进程来说这是可接受的（进程退出即回收），也能换来实现上的简单与性能稳定。
    let name: &'static str = Box::leak(spec.name.clone().into_boxed_str());
    let about: &'static str = Box::leak(spec.about.clone().into_boxed_str());
    let mut command = Command::new(name)
        .about(about)
        .disable_help_subcommand(true)
        .disable_help_flag(true)
        .help_template(help_template(locale));
    command = command
        .subcommand_help_heading(if locale == "zh" { "命令" } else { "Commands" })
        .arg(help_arg(locale));

    if let Some(alias) = &spec.alias {
        let alias: &'static str = Box::leak(alias.clone().into_boxed_str());
        command = command.visible_alias(alias);
    }

    if !spec.aliases.is_empty() {
        let aliases: Vec<&'static str> = spec
            .aliases
            .iter()
            .cloned()
            .map(|alias| Box::leak(alias.into_boxed_str()) as &'static str)
            .collect();
        command = command.visible_aliases(aliases);
    }

    for arg_spec in &spec.args {
        command = command.arg(to_positional_arg(arg_spec, locale));
    }

    for option_spec in &spec.options {
        command = command.arg(to_option_arg(option_spec, locale));
        if let Some(negated_arg) = to_negated_option_arg(option_spec, locale) {
            // `--no-xxx` 采用“隐藏 flag + 与原 flag 冲突”的方式实现，避免在 help 中污染选项列表。
            command = command.arg(negated_arg);
        }
    }

    for subcommand in &spec.subcommands {
        command = command.subcommand(to_clap_command(subcommand, locale));
    }

    if let Some(help_footer) = render_help_footer(spec, locale) {
        let help_footer: &'static str = Box::leak(help_footer.into_boxed_str());
        command = command.after_help(help_footer);
    }

    command
}

fn to_positional_arg(spec: &ArgSpec, locale: &str) -> Arg {
    let name: &'static str = Box::leak(spec.name.clone().into_boxed_str());
    let help: &'static str = Box::leak(spec.help.clone().into_boxed_str());
    let mut arg = Arg::new(name)
        .help(help)
        .required(spec.required)
        .help_heading(if locale == "zh" {
            Some("参数")
        } else {
            Some("Arguments")
        });

    if spec.multiple {
        arg = arg.num_args(1..);
    }

    arg
}

fn to_option_arg(spec: &crate::OptionSpec, locale: &str) -> Arg {
    let name: &'static str = Box::leak(spec.long.clone().into_boxed_str());
    let help: &'static str = Box::leak(spec.help.clone().into_boxed_str());
    let mut arg = Arg::new(name)
        .long(name)
        .help(help)
        .help_heading(if locale == "zh" {
            Some("选项")
        } else {
            Some("Options")
        });

    if let Some(short) = spec.short {
        arg = arg.short(short);
    }

    match spec.value_kind {
        ValueKind::Bool => {
            arg = arg.action(ArgAction::SetTrue);
        }
        ValueKind::String | ValueKind::Number | ValueKind::OptionalString => {
            arg = arg.num_args(1);
        }
    }

    if let Some(default_value) = &spec.default_value {
        let default_value: &'static str = Box::leak(default_value.clone().into_boxed_str());
        arg = arg.default_value(default_value);
    }

    if !spec.choices.is_empty() {
        let choices: Vec<&'static str> = spec
            .choices
            .iter()
            .cloned()
            .map(|choice| Box::leak(choice.into_boxed_str()) as &'static str)
            .collect();
        arg = arg.value_parser(choices);
    }

    arg
}

fn to_negated_option_arg(spec: &crate::OptionSpec, locale: &str) -> Option<Arg> {
    if spec.value_kind != ValueKind::Bool || !spec.negatable {
        return None;
    }
    let name = format!("no-{}", spec.long);
    let long: &'static str = Box::leak(name.clone().into_boxed_str());
    let main: &'static str = Box::leak(spec.long.clone().into_boxed_str());
    Some(
        Arg::new(long)
            .long(long)
            .action(ArgAction::SetTrue)
            .help_heading(if locale == "zh" {
                Some("选项")
            } else {
                Some("Options")
            })
            .hide(true)
            .conflicts_with(main),
    )
}

fn parse_subcommand_with_spec(
    commands: &[CommandSpec],
    matches: &ArgMatches,
) -> Option<(Vec<String>, ParsedArgv)> {
    let (name, subcommand) = matches.subcommand()?;
    let spec = commands
        .iter()
        .find(|spec| spec_matches_segment(spec, name))?;

    // argv 的设计是“分开收集 args 和 options”：
    // - args：位置参数，按 spec.args 的名字落到 map 里
    // - options：flag/option，按 long 名落到 map 里
    let mut argv = ParsedArgv::default();
    capture_matches_with_spec(subcommand, spec, &mut argv);

    // 处理嵌套子命令：把子层解析出的 argv 合并到当前层。
    // 约定：同名 key 以更深层为准（`extend` 会覆盖），便于子命令复用父层选项名。
    if !spec.subcommands.is_empty() {
        if let Some((mut path, nested)) = parse_subcommand_with_spec(&spec.subcommands, subcommand)
        {
            path.insert(0, name.to_string());
            argv.args.extend(nested.args);
            argv.options.extend(nested.options);
            return Some((path, argv));
        }
    }

    Some((vec![name.to_string()], argv))
}

#[allow(dead_code)]
fn parse_subcommand(matches: &ArgMatches) -> Option<(Vec<String>, ParsedArgv)> {
    let (name, subcommand) = matches.subcommand()?;
    let mut argv = ParsedArgv::default();
    capture_matches(subcommand, &mut argv);

    if let Some((mut path, nested)) = parse_subcommand(subcommand) {
        path.insert(0, name.to_string());
        argv.args.extend(nested.args);
        argv.options.extend(nested.options);
        Some((path, argv))
    } else {
        Some((vec![name.to_string()], argv))
    }
}

fn capture_matches_with_spec(matches: &ArgMatches, spec: &CommandSpec, argv: &mut ParsedArgv) {
    // 选项参数：直接遍历 clap 识别到的 ids。
    for id in matches.ids() {
        let key = id.as_str().to_string();
        if let Some(value) = value_from_matches(matches, &key) {
            argv.options.insert(key, value);
        }
    }

    // 位置参数：必须依赖 spec 才能知道具体参数名。
    for arg_spec in &spec.args {
        if arg_spec.multiple {
            if let Some(values) = matches
                .try_get_many::<String>(&arg_spec.name)
                .ok()
                .flatten()
            {
                argv.args.insert(
                    arg_spec.name.clone(),
                    json!(values.cloned().collect::<Vec<_>>()),
                );
            }
            continue;
        }
        if let Some(value) = matches.try_get_one::<String>(&arg_spec.name).ok().flatten() {
            argv.args.insert(arg_spec.name.clone(), json!(value));
        }
    }

    normalize_negated_flags(argv);
}

#[allow(dead_code)]
fn capture_matches(matches: &ArgMatches, argv: &mut ParsedArgv) {
    // 兼容保留：旧实现只收集 options，不收集 args。
    // 当前解析路径使用 `capture_matches_with_spec`，因为它能同时处理位置参数。
    for id in matches.ids() {
        let key = id.as_str().to_string();
        if let Some(value) = value_from_matches(matches, &key) {
            argv.options.insert(key, value);
        }
    }

    normalize_negated_flags(argv);
}

fn normalize_negated_flags(argv: &mut ParsedArgv) {
    // clap 会把 `--no-foo` 当作一个独立的 bool flag；这里把它规范化成 `foo=false`。
    // 这样上游 handler 只需要关心正向选项名。
    let negated = argv
        .options
        .keys()
        .filter(|key| key.starts_with("no-"))
        .cloned()
        .collect::<Vec<_>>();
    for key in negated {
        if argv
            .options
            .get(&key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            argv.options
                .insert(key.trim_start_matches("no-").to_string(), json!(false));
        }
        argv.options.remove(&key);
    }
}

fn value_from_matches(matches: &ArgMatches, key: &str) -> Option<Value> {
    if let Some(value) = matches.try_get_one::<bool>(key).ok().flatten() {
        return Some(json!(*value));
    }

    if let Some(value) = matches.try_get_one::<String>(key).ok().flatten() {
        // 这里做一个很轻量的“字符串 -> 数字”尝试，
        // 目的是让上层 handler 少处理一层显而易见的 `"123"` -> `123` 转换。
        // 但它不会做更激进的类型推断（例如 bool / object / array），避免把 CLI 输入语义搞得过于魔法化。
        if let Ok(number) = value.parse::<i64>() {
            return Some(json!(number));
        }
        return Some(json!(value));
    }

    if let Some(values) = matches.try_get_many::<String>(key).ok().flatten() {
        return Some(json!(values.cloned().collect::<Vec<_>>()));
    }

    None
}

fn lookup_handler_id(commands: &[CommandSpec], path: &[String]) -> Option<String> {
    let (head, tail) = path.split_first()?;
    let spec = commands
        .iter()
        .find(|spec| spec_matches_segment(spec, head))?;
    lookup_handler_id_in_spec(spec, tail)
}

fn lookup_handler_id_in_spec(spec: &CommandSpec, path: &[String]) -> Option<String> {
    if path.is_empty() {
        return Some(spec.handler_id.clone());
    }

    let (head, tail) = path.split_first()?;
    let child = spec
        .subcommands
        .iter()
        .find(|spec| spec_matches_segment(spec, head))?;
    lookup_handler_id_in_spec(child, tail)
}

fn render_help_footer(spec: &CommandSpec, locale: &str) -> Option<String> {
    let mut lines = Vec::new();

    let mut aliases = Vec::new();
    if let Some(alias) = &spec.alias {
        aliases.push(alias.clone());
    }
    aliases.extend(spec.aliases.iter().cloned());
    if !aliases.is_empty() {
        lines.push(format!(
            "{}: {}",
            if locale == "zh" { "别名" } else { "Aliases" },
            aliases.join(", ")
        ));
    }

    if !spec.examples.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(if locale == "zh" {
            "示例:".to_string()
        } else {
            "Examples:".to_string()
        });
        for example in &spec.examples {
            lines.push(format!("  {}  {}", example.command, example.description));
        }
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn help_template(locale: &str) -> String {
    let usage = if locale == "zh" { "用法" } else { "Usage" };
    format!("{{about-with-newline}}\n{usage}: {{usage}}\n\n{{all-args}}{{after-help}}")
}

fn help_arg(locale: &str) -> Arg {
    Arg::new("help")
        .short('h')
        .long("help")
        .action(ArgAction::Help)
        .help(if locale == "zh" {
            "显示帮助"
        } else {
            "Print help"
        })
        .help_heading(if locale == "zh" {
            Some("选项")
        } else {
            Some("Options")
        })
}

fn version_arg(locale: &str) -> Arg {
    Arg::new("version")
        .short('V')
        .long("version")
        .action(ArgAction::Version)
        .help(if locale == "zh" {
            "显示版本"
        } else {
            "Print version"
        })
        .help_heading(if locale == "zh" {
            Some("选项")
        } else {
            Some("Options")
        })
}

fn spec_matches_segment(spec: &CommandSpec, segment: &str) -> bool {
    spec.name == segment
        || spec.alias.as_deref() == Some(segment)
        || spec.aliases.iter().any(|alias| alias == segment)
}

#[derive(Debug, PartialEq, Eq)]
enum BuiltinCommand {
    Help { path: Vec<String> },
    Version,
}

fn detect_builtin_command(args: &[String]) -> Option<BuiltinCommand> {
    let tokens = &args[1..];
    let first = tokens.first()?;
    // builtin help/version 在这里提前识别，而不是完全交给 clap：
    // - 这样可以在 host runtime 尚未初始化完时也稳定输出帮助/版本
    // - 也便于把错误/help 输出统一包装成当前项目约定的文本或 JSON 形态
    if matches!(first.as_str(), "-V" | "--version" | "version") {
        return Some(BuiltinCommand::Version);
    }
    if matches!(first.as_str(), "-h" | "--help") {
        return Some(BuiltinCommand::Help { path: vec![] });
    }
    if first == "help" {
        return Some(BuiltinCommand::Help {
            path: tokens[1..].to_vec(),
        });
    }
    if let Some(index) = tokens
        .iter()
        .position(|item| item == "--help" || item == "-h")
    {
        return Some(BuiltinCommand::Help {
            path: tokens[..index].to_vec(),
        });
    }
    if let Some(index) = tokens.iter().position(|item| item == "help") {
        if index != tokens.len() - 1 {
            return None;
        }
        return Some(BuiltinCommand::Help {
            path: tokens[..index].to_vec(),
        });
    }
    None
}

fn render_help_command(
    binary_name: &'static str,
    about: &'static str,
    version: &'static str,
    commands: &[CommandSpec],
    path: &[String],
    runtime_error_exit_code: i32,
    locale: &str,
) -> BuiltinCommandOutput {
    let mut cli = build_cli(binary_name, about, version, commands, locale);
    let mut target = &mut cli;
    for segment in path {
        // 这里沿着 path 一层层下钻子命令树。
        // 如果任一层找不到，就把它视为“未知帮助主题”，而不是继续交给 clap 兜底。
        match target.find_subcommand_mut(segment) {
            Some(subcommand) => target = subcommand,
            None => {
                return BuiltinCommandOutput {
                    output: serde_json::to_string_pretty(&serde_json::json!({
                        "kind": "error",
                        "message": if locale == "zh" {
                            format!("未知帮助主题：{segment}")
                        } else {
                            format!("unknown help topic: {segment}")
                        },
                        "exitCode": runtime_error_exit_code,
                    }))
                    .unwrap_or_else(|_| {
                        "{\"kind\":\"error\",\"message\":\"unknown help topic\",\"exitCode\":1}"
                            .into()
                    }) + "\n",
                    exit_code: runtime_error_exit_code,
                };
            }
        }
    }

    let mut buffer = Cursor::new(Vec::new());
    match target.write_long_help(&mut buffer) {
        Ok(_) => {
            let mut output = String::from_utf8_lossy(buffer.get_ref()).into_owned();
            output.push('\n');
            BuiltinCommandOutput {
                output,
                exit_code: 0,
            }
        }
        Err(_) => BuiltinCommandOutput {
            output: if locale == "zh" {
                format!("{binary_name} - {about}\n使用 `{binary_name} --help` 查看帮助。\n")
            } else {
                format!("{binary_name} - {about}\nUse `{binary_name} --help` to show help.\n")
            },
            exit_code: 0,
        },
    }
}

fn apply_legacy_aliases_to_spec(spec: &mut CommandSpec, parent_path: &[&str]) {
    let mut path = parent_path.to_vec();
    path.push(spec.name.as_str());

    if let Some(aliases) = legacy_aliases_for_path(&path) {
        if spec.alias.is_none() {
            spec.alias = aliases.first().map(|alias| (*alias).to_string());
        }
        for alias in aliases.iter().skip(1) {
            if !spec.aliases.iter().any(|existing| existing == alias) {
                spec.aliases.push((*alias).to_string());
            }
        }
    }

    for child in &mut spec.subcommands {
        apply_legacy_aliases_to_spec(child, &path);
    }
}

fn legacy_aliases_for_path(path: &[&str]) -> Option<&'static [&'static str]> {
    match path {
        ["dev"] => Some(&["d"]),
        ["build"] => Some(&["b"]),
        ["lint"] => Some(&["l"]),
        ["create"] => Some(&["c"]),
        ["add"] => Some(&["a"]),
        ["generate"] => Some(&["g"]),
        ["generate", "api"] => Some(&["contract"]),
        ["release"] => Some(&["r"]),
        ["sync"] => Some(&["s"]),
        ["template"] => Some(&["t"]),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
