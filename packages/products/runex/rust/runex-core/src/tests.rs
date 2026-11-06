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

/// §11.3 冻结导出面的编译期守卫。
///
/// 只引用、不断言行为：目的是让「悄悄删掉/改名某个稳定导出」变成编译错误，
/// 而不是等外部消费者（GUI / IDE 插件 / CI 脚本）报 bug 才发现。
/// 增删这份清单前，先改 `docs/design/self-managed-runtime-manager.zh-CN.md` §11.3。
#[test]
fn frozen_export_surface_is_intact() {
    use crate::{RuntimeProvider, HOME_ENV, META_FILE};

    // 每一项都必须仍然可以从 crate 根导入（`as _` 只校验名字存在）。
    #[allow(unused_imports)]
    use crate::{
        build_plan as _, describe_provider as _, execute_plan as _, pick_best as _,
        provider_for as _, Arch as _, Diagnostic as _, EnvAction as _, EnvPlan as _,
        EnvService as _, InstallEvent as _, InstallOutcome as _, InstalledVersion as _,
        Language as _, Libc as _, Os as _, PlanStep as _, Platform as _, ProjectVersion as _,
        ProviderArtifact as _, ProviderContext as _, ProviderInfo as _, RuntimeHome as _,
        RuntimeStore as _, ToolStatus as _, VersionTag as _,
    };

    // 扩展点是 trait，单独用泛型约束盯住（不能被降级成私有）。
    #[allow(dead_code)]
    fn assert_provider<T: RuntimeProvider + ?Sized>() {}

    // 这两项是用户可见契约（`RUNEX_HOME` 与每版本元数据文件名），钉住字面值。
    assert_eq!(HOME_ENV, "RUNEX_HOME");
    assert_eq!(META_FILE, ".runex-meta.json");
}
