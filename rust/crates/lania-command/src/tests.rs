use serde_json::json;

use crate::{
    apply_legacy_aliases, build_cli, command_context_from_matches, render_builtin_command, ArgSpec,
    CommandSpec, Example, OptionSpec, ValueKind,
};

#[test]
fn converts_command_spec_to_clap_and_parses_options() {
    // 这是命令层最核心的回归测试之一：
    // 它串起了 `CommandSpec -> clap matches -> CommandContext` 整条路径，
    // 确保声明式命令最终真的能变成 handler 可消费的结构化参数。
    let mut spec = CommandSpec::new("dev", "Run dev server", "command.dev");
    spec.options = vec![
        OptionSpec {
            long: "port".into(),
            short: Some('p'),
            help: "Port".into(),
            value_kind: ValueKind::Number,
            default_value: Some("3000".into()),
            choices: vec![],
            negatable: false,
        },
        OptionSpec {
            long: "open".into(),
            short: Some('o'),
            help: "Open browser".into(),
            value_kind: ValueKind::Bool,
            default_value: None,
            choices: vec![],
            negatable: false,
        },
    ];
    spec.examples = vec![Example {
        command: "lan dev --port 4000".into(),
        description: "custom port".into(),
    }];

    let command = build_cli("lan", "Lania CLI", "0.1.0", &[spec.clone()], "en");
    command.clone().debug_assert();
    let matches = command
        .try_get_matches_from(["lan", "dev", "--port", "4000", "--open"])
        .expect("matches should parse");
    let context = command_context_from_matches(&[spec], &matches, "/repo", "trace-1")
        .expect("subcommand selection exists");

    assert_eq!(context.handler_id, "command.dev");
    assert_eq!(context.argv.options["port"], json!(4000));
    assert_eq!(context.argv.options["open"], json!(true));
}

#[test]
fn supports_nested_subcommands() {
    // 验证 handler_id 查找是沿“命令路径”递归下钻，而不是只看顶层命令名。
    let mut parent = CommandSpec::new("sync", "Sync repo", "command.sync");
    parent.subcommands.push(CommandSpec::new(
        "add",
        "Attach add workflow under sync",
        "command.sync.add",
    ));

    let commands = vec![parent];
    let matches = build_cli("lan", "Lania CLI", "0.1.0", &commands, "en")
        .try_get_matches_from(["lan", "sync", "add"])
        .expect("nested subcommand parses");
    let context = command_context_from_matches(&commands, &matches, "/repo", "trace-2")
        .expect("nested selection exists");

    assert_eq!(context.handler_id, "command.sync.add");
}

#[test]
fn supports_negated_bool_flags() {
    // 这个测试锁定 `--no-xxx -> xxx=false` 的规范化语义。
    // 上层 handler 永远只需要读正向 key（这里是 `push`），不必同时处理 `no-push`。
    let mut spec = CommandSpec::new("sync", "Sync repo", "command.sync");
    spec.options.push(OptionSpec {
        long: "push".into(),
        short: Some('p'),
        help: "push changes".into(),
        value_kind: ValueKind::Bool,
        default_value: None,
        choices: vec![],
        negatable: true,
    });

    let matches = build_cli("lan", "Lania CLI", "0.1.0", &[spec.clone()], "en")
        .try_get_matches_from(["lan", "sync", "--no-push"])
        .expect("matches should parse");
    let context = command_context_from_matches(&[spec], &matches, "/repo", "trace-1")
        .expect("subcommand selection exists");

    assert_eq!(context.argv.options["push"], json!(false));
}

#[test]
fn supports_multiple_positional_args() {
    let spec = CommandSpec::new("tools", "Tools", "command.tools.run").with_args(vec![
        ArgSpec {
            name: "file".into(),
            required: true,
            multiple: false,
            help: "file".into(),
        },
        ArgSpec {
            name: "args".into(),
            required: false,
            multiple: true,
            help: "args".into(),
        },
    ]);

    let matches = build_cli("lan", "Lania CLI", "0.1.0", &[spec.clone()], "en")
        .try_get_matches_from(["lan", "tools", "demo.py", "one", "two"])
        .expect("matches should parse");
    let context = command_context_from_matches(&[spec], &matches, "/repo", "trace-many")
        .expect("command selection exists");

    assert_eq!(context.argv.args["file"], json!("demo.py"));
    assert_eq!(context.argv.args["args"], json!(["one", "two"]));
}

#[test]
fn appends_examples_and_aliases_to_clap_command() {
    // builtin help 是用户最常直接接触的输出面，
    // 所以这里回归测试“声明里的 alias/example 是否真的进入 help 文本”。
    let spec =
        CommandSpec::new("build", "Build repo", "command.build").with_examples(vec![Example {
            command: "lan build".into(),
            description: "build once".into(),
        }]);
    let mut commands = vec![spec];
    apply_legacy_aliases(&mut commands);

    let help = render_builtin_command(
        "lan",
        "Lania CLI",
        "0.1.0",
        &commands,
        &["lan".into(), "help".into(), "build".into()],
        1,
        "en",
    )
    .expect("help output");

    assert_eq!(help.exit_code, 0);
    assert!(help.output.contains("Aliases: b"));
    assert!(help.output.contains("Examples:"));
    assert!(help.output.contains("lan build"));
}

#[test]
fn supports_alias_lookup_in_command_context() {
    // 这里不仅测试 alias 能被 clap 识别，还测试它最终能解析回原命令的 handler_id。
    let spec = CommandSpec::new("build", "Build repo", "command.build");
    let mut commands = vec![spec];
    apply_legacy_aliases(&mut commands);

    let matches = build_cli("lan", "Lania CLI", "0.1.0", &commands, "en")
        .try_get_matches_from(["lan", "b"])
        .expect("alias should parse");
    let context = command_context_from_matches(&commands, &matches, "/repo", "trace-1")
        .expect("alias should resolve");

    assert_eq!(context.handler_id, "command.build");
}
