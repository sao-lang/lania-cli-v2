//! `runex` Rust API 的进程内测试：不联网、不碰真实 `~/.runex`。
//!
//! 所有用例都用显式 `RuntimeHome::at(temp)`，所以即使测试并行也不会互相污染。

use std::path::PathBuf;

use runex::api::{EnvAction, Platform, Runex, RuntimeHome};

fn temp_root(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("runex-api-{name}-{unique}"));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn api(name: &str) -> (Runex, PathBuf) {
    let root = temp_root(name);
    let home = RuntimeHome::at(root.join("home"));
    let platform = Platform::detect().expect("platform");
    (Runex::new(home, platform).with_cwd(root.clone()), root)
}

#[test]
fn api_exposes_the_management_surface() {
    let (api, root) = api("surface");
    assert_eq!(api.cwd(), root.as_path());
    assert_eq!(api.home().root(), root.join("home").as_path());
    assert!(api.managed_ids().contains(&"node"));
    assert!(api.provider_info("node").is_some());
    assert!(api.provider_info("java").is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn plan_for_an_unknown_tool_reports_an_error() {
    let (api, root) = api("unknown");
    let plan = api.plan(EnvAction::Install, "unknown", Some("1"), false);
    assert!(plan.error.is_some());
    assert!(plan.error.unwrap().contains("unsupported tool"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn install_plan_requires_a_version() {
    let (api, root) = api("no-version");
    let plan = api.plan(EnvAction::Install, "node", None, false);
    assert!(plan
        .error
        .as_deref()
        .is_some_and(|error| error.contains("requires a version")));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn plan_uses_the_api_home_instead_of_the_global_one() {
    let (api, root) = api("plan-home");
    let plan = api.plan(EnvAction::Install, "node", Some("22"), false);
    assert!(plan.error.is_none(), "{:?}", plan.error);
    let expected = api
        .home()
        .runtimes_dir()
        .display()
        .to_string()
        .replace('\\', "/");
    let label = &plan.steps[0].label;
    assert!(label.contains(&expected), "expected {expected} in {label}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dry_run_apply_does_not_touch_the_store() {
    let (api, root) = api("dry-run");
    let plan = api.plan(EnvAction::Install, "node", Some("22"), false);
    assert!(plan.error.is_none(), "{:?}", plan.error);
    let executed = api.apply(&plan, true, |_| {}).expect("dry run succeeds");
    assert!(executed.is_empty());
    assert!(!api.home().runtimes_dir().exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn installed_all_is_empty_on_a_clean_home() {
    let (api, root) = api("empty");
    assert!(api.installed_all().expect("clean store").is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn which_fails_cleanly_when_nothing_is_installed() {
    let (api, root) = api("which");
    let error = api.which("node").expect_err("nothing installed");
    let message = error.to_string();
    assert!(message.contains("runex install"), "{message}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn init_snippet_mentions_the_shims_directory() {
    let (api, root) = api("init");
    let snippet = api.init_snippet("bash");
    assert!(snippet.contains("PATH"), "{snippet}");
    assert!(
        snippet.contains("shims") || snippet.contains("runex"),
        "{snippet}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn api_can_render_a_tui_snapshot() {
    let (api, root) = api("tui");
    let frame = api
        .tui_snapshot(80, 24, 0, false, "en")
        .expect("snapshot renders");
    assert_eq!(frame.lines().count(), 24, "{frame}");
    assert!(!frame.trim().is_empty(), "snapshot should not be blank");
    let _ = std::fs::remove_dir_all(root);
}
