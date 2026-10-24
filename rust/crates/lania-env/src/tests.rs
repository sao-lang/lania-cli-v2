//! 语言表与计划层的集成测试（跨模块行为）。
//!
//! 单模块的行为测试在各个文件的 `mod tests` 里；这里只覆盖「语言元数据」
//! 与「第一批自管语言」之间的契约。

use crate::{supported_language_ids, Language};

#[test]
fn supports_at_least_40_languages() {
    assert!(
        Language::all().len() >= 40,
        "expected the full language table, got {}",
        Language::all().len()
    );
}

#[test]
fn every_language_has_probes_and_display_metadata() {
    for language in Language::all() {
        assert!(!language.id().is_empty(), "{language:?} has no id");
        assert!(
            !language.display_name().is_empty(),
            "{language:?} has no display name"
        );
        assert!(
            !language.probe_binaries().is_empty(),
            "{language:?} has no probe binaries"
        );
        assert!(
            Language::parse_id(language.id()) == Some(language),
            "{} does not round-trip through parse_id",
            language.id()
        );
    }
}

#[test]
fn batch_one_languages_all_exist_in_the_table() {
    for id in supported_language_ids() {
        let language = Language::parse_id(id)
            .unwrap_or_else(|| panic!("{id} has a provider but no Language variant"));
        assert_eq!(language.id(), *id);
    }
}

#[test]
fn parses_language_aliases() {
    let cases = [
        ("node", Language::Node),
        ("nodejs", Language::Node),
        ("javascript", Language::Node),
        ("python3", Language::Python),
        ("golang", Language::Go),
        ("c++", Language::Cpp),
        ("仓颉", Language::Cangjie),
        ("鸿蒙", Language::ArkTs),
    ];
    for (input, expected) in cases {
        assert_eq!(Language::parse_id(input), Some(expected), "{input}");
    }
    assert_eq!(Language::parse_id("not-a-language"), None);
}

#[test]
fn companions_only_include_package_managers() {
    let node = Language::Node.companions();
    assert!(node.contains(&"npm"));
    assert!(node.contains(&"pnpm"));
    // 编译器 / LSP 不应出现在附属工具里。
    assert!(!node.contains(&"tsc"));
}

#[test]
fn language_summaries_are_bilingual() {
    for language in Language::all() {
        let en = language.summary(false);
        let zh = language.summary(true);
        assert!(!en.is_empty(), "{language:?} missing en summary");
        assert!(!zh.is_empty(), "{language:?} missing zh summary");
    }
}

#[test]
fn version_files_are_declared_for_common_languages() {
    assert!(Language::Node.version_files().contains(&".nvmrc"));
    assert!(Language::Python
        .version_files()
        .contains(&".python-version"));
    assert!(Language::Rust
        .version_files()
        .contains(&"rust-toolchain.toml"));
}
