//! `lania-env` 的纯逻辑测试（不依赖本机是否安装任何管理器）。

use crate::{build_plan, extract_semver, EnvAction, Language, ManagerKind};
use std::path::Path;

#[test]
fn parses_language_aliases() {
    assert_eq!(Language::parse_id("node"), Some(Language::Node));
    assert_eq!(Language::parse_id("NodeJS"), Some(Language::Node));
    assert_eq!(Language::parse_id("py"), Some(Language::Python));
    assert_eq!(Language::parse_id("golang"), Some(Language::Go));
    assert_eq!(Language::parse_id("c++"), Some(Language::Cpp));
    assert_eq!(Language::parse_id("jdk"), Some(Language::Java));
    assert_eq!(Language::parse_id("brainfuck"), None);
}

#[test]
fn supports_at_least_40_languages() {
    let all = Language::all();
    assert!(
        all.len() >= 40,
        "expected >= 40 languages, got {}",
        all.len()
    );
    // id 必须唯一且非空
    let mut ids = std::collections::HashSet::new();
    for language in &all {
        assert!(!language.id().is_empty());
        assert!(ids.insert(language.id()), "duplicate id {}", language.id());
    }
    // 别名必须能解析回原始语言（防止 `d` / `v` 这类短名歧义）。
    for language in &all {
        assert_eq!(
            Language::parse_id(language.id()),
            Some(*language),
            "{} id does not round-trip",
            language.id()
        );
    }
}

#[test]
fn every_language_has_probes_and_managers() {
    for language in Language::all() {
        assert!(
            !language.probe_binaries().is_empty(),
            "{} has no probe binaries",
            language.id()
        );
        assert!(
            !language.managers().is_empty(),
            "{} has no managers",
            language.id()
        );
        assert!(
            !language.display_name().is_empty(),
            "{} has no display name",
            language.id()
        );
    }
}

#[test]
fn bun_and_deno_are_supported() {
    assert_eq!(Language::parse_id("bun"), Some(Language::Bun));
    assert_eq!(Language::parse_id("deno"), Some(Language::Deno));
    for language in [Language::Bun, Language::Deno] {
        assert!(Language::all().contains(&language));
        assert!(!language.probe_binaries().is_empty());
    }
}

#[test]
fn extracts_semver_from_common_outputs() {
    assert_eq!(extract_semver("v20.11.1"), Some("20.11.1".into()));
    assert_eq!(
        extract_semver("rustc 1.86.0 (05f9846f8 2025-03-31)"),
        Some("1.86.0".into())
    );
    assert_eq!(extract_semver("cargo 1.86.0"), Some("1.86.0".into()));
    assert_eq!(extract_semver("no version here"), None);
}

#[test]
fn unknown_tool_yields_error_plan() {
    let plan = build_plan(
        EnvAction::Install,
        "brainfuck",
        Some("1.0"),
        None,
        Path::new("."),
        false,
    );
    assert!(!plan.is_runnable());
    assert!(plan.error.unwrap().contains("unsupported tool"));
}

#[test]
fn install_without_version_is_rejected() {
    let plan = build_plan(
        EnvAction::Install,
        "node",
        None,
        Some(ManagerKind::Fnm),
        Path::new("."),
        false,
    );
    assert!(!plan.is_runnable());
    assert!(plan.error.unwrap().contains("requires a version"));
}

#[test]
fn install_plan_uses_requested_manager() {
    let plan = build_plan(
        EnvAction::Install,
        "node",
        Some("20"),
        Some(ManagerKind::Fnm),
        Path::new("."),
        false,
    );
    if plan.is_runnable() {
        // 用户指定 fnm 时优先用它；若未安装，`pick_manager` 会回退到其它可用管理器。
        let manager = plan.manager.expect("runnable plan has a manager");
        if crate::managers::probe(ManagerKind::Fnm).available {
            assert_eq!(manager, ManagerKind::Fnm);
        } else {
            assert!(Language::Node.managers().contains(&manager));
        }
        assert!(!plan.steps.is_empty());
    } else {
        assert!(plan.error.unwrap().contains("no supported version manager"));
    }
}

#[test]
fn project_use_writes_version_file() {
    let plan = build_plan(
        EnvAction::Use,
        "python",
        Some("3.12"),
        Some(ManagerKind::Pyenv),
        Path::new("/repo"),
        true,
    );
    assert!(plan
        .steps
        .iter()
        .any(|step| step.program == "write-file" && step.args[1] == "3.12"));
}

#[test]
fn plan_steps_never_interpolate_into_shell_args() {
    let plan = build_plan(
        EnvAction::Install,
        "node",
        Some("20; rm -rf /"),
        Some(ManagerKind::Volta),
        Path::new("."),
        false,
    );
    // 非法版本必须在生成计划前被拒绝，而不是被拼接进命令。
    assert!(!plan.is_runnable());
    assert!(plan.error.unwrap().contains("invalid version"));
}

#[test]
fn shell_function_steps_only_contain_safe_tokens() {
    // nvm 走 shell 分支，必须确保脚本里只有白名单字符。
    let plan = build_plan(
        EnvAction::Install,
        "node",
        Some("20.11.1"),
        Some(ManagerKind::Nvm),
        Path::new("."),
        false,
    );
    for step in plan.steps.iter().filter(|step| step.use_shell) {
        let script = step.shell_script().expect("shell step has script");
        assert!(
            script
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " .-_+@/".contains(c)),
            "{script}"
        );
    }
}

#[test]
fn rejects_shell_metacharacters() {
    for bad in [
        "20 && whoami",
        "20;whoami",
        "$(whoami)",
        "`id`",
        "20|cat",
        "a\"b",
    ] {
        let plan = build_plan(
            EnvAction::Use,
            "node",
            Some(bad),
            Some(ManagerKind::Nvm),
            Path::new("."),
            false,
        );
        assert!(!plan.is_runnable(), "should reject {bad}");
    }
}

#[test]
fn error_outputs_are_not_parsed_as_versions() {
    // rustup 组件缺失时输出 `error: 'rustfmt' is not installed for the toolchain
    // 'stable-x86_64-unknown-linux-gnu'`。旧实现会从 `x86_64` 里抽出 `86`。
    for text in [
        "error: 'rustfmt' is not installed for the toolchain 'stable-x86_64-unknown-linux-gnu'\nhelp: run `rustup component add rustfmt`",
        "/bin/sh: 1: nvm: not found",
        "make: unknown option -- version",
    ] {
        assert!(
            crate::detect::is_error_output(text),
            "should be treated as error: {text}"
        );
    }
    // 正常的版本输出不能被误判。
    for text in [
        "rustfmt 1.8.0-stable (abcdef 2024-01-01)",
        "cargo 1.86.0 (adf9b6ad1 2025-02-07)",
        "Python 3.14.2",
    ] {
        assert!(
            !crate::detect::is_error_output(text),
            "should not be treated as error: {text}"
        );
    }
}
