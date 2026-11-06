//! 本地化覆盖 conformance 测试（自 `lania-command` 迁入）。
//!
//! 组装与 `lania-cli/src/main.rs` 相同的内建命令树，断言 zh 本地化覆盖
//! 每一条 about / option help / arg help / example 文案——任何命令包新增
//! 命令、选项、参数或示例而未提供中文文案时，本测试即失败。

use lania_command::{localize_command_specs, CommandSpec};
use lania_host::{
    capability::CapabilityContainer,
    plugin::{Plugin, PluginSetupContext},
    registry::{CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl},
    HookBusImpl,
};
use lania_plugins_command_add::AddCommandPlugin;
use lania_plugins_command_build::BuildCommandPlugin;
use lania_plugins_command_create::CreateCommandPlugin;
use lania_plugins_command_dev::DevCommandPlugin;
use lania_plugins_command_env::EnvCommandPlugin;
use lania_plugins_command_generate::GenerateCommandPlugin;
use lania_plugins_command_git::GitCommandPlugin;
use lania_plugins_command_lint::LintCommandPlugin;
use lania_plugins_command_locale::ConfigCommandPlugin;
use lania_plugins_command_pi::PiCommandPlugin;
use lania_plugins_command_release::ReleaseCommandPlugin;
use lania_plugins_command_template::TemplateCommandPlugin;
use lania_plugins_command_tools::ToolsCommandPlugin;

/// Rebuild the same built-in command tree that `lania-cli` registers before
/// rendering help, so any new command/option/arg/example is covered here
/// automatically.
fn builtin_command_specs() -> Vec<CommandSpec> {
    let mut commands = CommandRegistryImpl::new();
    let mut hooks = HookBusImpl::new();
    let mut capabilities = CapabilityContainer::new();
    let mut handlers = CommandHandlerRegistryImpl::new();

    macro_rules! setup {
        ($plugin:expr) => {{
            $plugin
                .setup(&mut PluginSetupContext {
                    commands: &mut commands,
                    hooks: &mut hooks,
                    capabilities: &mut capabilities,
                    handlers: &mut handlers,
                })
                .expect("built-in plugin setup succeeds");
        }};
    }

    // Same registration order as `lania-cli/src/main.rs`.
    setup!(DevCommandPlugin);
    setup!(BuildCommandPlugin);
    setup!(LintCommandPlugin);
    setup!(CreateCommandPlugin);
    setup!(AddCommandPlugin);
    setup!(GenerateCommandPlugin);
    setup!(ReleaseCommandPlugin);
    setup!(GitCommandPlugin);
    setup!(TemplateCommandPlugin);
    setup!(ToolsCommandPlugin);
    setup!(EnvCommandPlugin);
    setup!(PiCommandPlugin);
    setup!(ConfigCommandPlugin);

    // 这里不存在原 lania-command dev-deps 环导致的双编译问题：
    // conformance 是测试汇合点，所有插件与本 crate 共享同一个
    // lania-command 编译单元，类型天然一致，无需 serde 桥接。
    commands.commands().iter().cloned().collect()
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch,
            '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
        )
    })
}

fn collect_untranslated(spec: &CommandSpec, parent_path: &str, failures: &mut Vec<String>) {
    let path = if parent_path.is_empty() {
        format!("lan {} ({})", spec.name, spec.handler_id)
    } else {
        format!("{parent_path} {} ({})", spec.name, spec.handler_id)
    };

    if !contains_cjk(&spec.about) {
        failures.push(format!("{path}: about = {:?}", spec.about));
    }
    for option in &spec.options {
        let label = if option.long.is_empty() {
            option
                .short
                .map(|short| format!("-{short}"))
                .unwrap_or_else(|| "<unknown>".into())
        } else {
            format!("--{}", option.long)
        };
        if !contains_cjk(&option.help) {
            failures.push(format!("{path}: option {label} help = {:?}", option.help));
        }
    }
    for arg in &spec.args {
        if !contains_cjk(&arg.help) {
            failures.push(format!("{path}: arg {} help = {:?}", arg.name, arg.help));
        }
    }
    for example in &spec.examples {
        if !contains_cjk(&example.description) {
            failures.push(format!(
                "{path}: example `{}` description = {:?}",
                example.command, example.description
            ));
        }
    }
    for subcommand in &spec.subcommands {
        collect_untranslated(subcommand, &path, failures);
    }
}

#[test]
fn zh_localization_covers_every_builtin_command_string() {
    let mut specs = builtin_command_specs();
    for expected in [
        "dev", "build", "lint", "create", "add", "generate", "release", "git", "template",
        "tools", "env", "pi", "config",
    ] {
        assert!(
            specs.iter().any(|spec| spec.name == expected),
            "built-in command tree is missing `{expected}`"
        );
    }

    localize_command_specs("zh", &mut specs);

    let mut failures = Vec::new();
    for spec in &specs {
        collect_untranslated(spec, "", &mut failures);
    }

    assert!(
        failures.is_empty(),
        "missing zh localization for {} help string(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
