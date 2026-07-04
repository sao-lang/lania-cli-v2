//! lania-preferences 单元测试。
//!
//! 关键点：
//! - 包含文件系统读写/路径解析
//! - 包含序列化/反序列化与 JSON 结构约定
use std::env;
use std::sync::Mutex;

use super::*;

/// Serializes tests that modify `HOME` env var to prevent race conditions.
static HOME_LOCK: Mutex<()> = Mutex::new(());

// ── normalize_locale ──────────────────────────────────────────

#[test]
fn normalize_locale_zh_is_recognized() {
    assert_eq!(normalize_locale("zh"), "zh");
}

#[test]
fn normalize_locale_zh_variants_map_to_zh() {
    assert_eq!(normalize_locale("zh-CN"), "zh");
    assert_eq!(normalize_locale("zh-TW"), "zh");
    assert_eq!(normalize_locale("zh-Hans"), "zh");
}

#[test]
fn normalize_locale_en_is_recognized() {
    assert_eq!(normalize_locale("en"), "en");
}

#[test]
fn normalize_locale_en_variants_map_to_en() {
    assert_eq!(normalize_locale("en-US"), "en");
    assert_eq!(normalize_locale("en-GB"), "en");
}

#[test]
fn normalize_locale_unknown_falls_back_to_en() {
    assert_eq!(normalize_locale("fr"), "en");
    assert_eq!(normalize_locale("de"), "en");
    assert_eq!(normalize_locale("ja"), "en");
}

#[test]
fn normalize_locale_handles_whitespace_and_case() {
    assert_eq!(normalize_locale("  ZH-CN  "), "zh");
    assert_eq!(normalize_locale("En"), "en");
    assert_eq!(normalize_locale("  "), "en");
}

// ── normalize_output_mode ─────────────────────────────────────

#[test]
fn normalize_output_mode_json_is_default() {
    assert_eq!(normalize_output_mode("json"), "json");
}

#[test]
fn normalize_output_mode_stream_accepts_aliases() {
    assert_eq!(normalize_output_mode("stream"), "stream");
    assert_eq!(normalize_output_mode("jsonl"), "stream");
}

#[test]
fn normalize_output_mode_human_is_recognized() {
    assert_eq!(normalize_output_mode("human"), "human");
}

#[test]
fn normalize_output_mode_unknown_falls_back_to_json() {
    assert_eq!(normalize_output_mode("xml"), "json");
    assert_eq!(normalize_output_mode(""), "json");
    assert_eq!(normalize_output_mode("  "), "json");
}

// ── UserPreferences::default ──────────────────────────────────

#[test]
fn default_preferences_uses_english_and_json() {
    let prefs = UserPreferences::default();
    assert_eq!(prefs.locale, "en");
    assert_eq!(prefs.output_mode, "json");
    assert_eq!(prefs.log_timestamps, false);
}

// ── preferences_path ──────────────────────────────────────────

#[test]
fn preferences_path_returns_some_when_home_is_set() {
    let _guard = HOME_LOCK.lock().unwrap();
    let previous = env::var("HOME").ok();
    env::set_var("HOME", "/tmp/lania-prefs-test-home");
    let path = preferences_path();
    restore_env("HOME", previous);
    assert!(path.is_some());
    let path = path.unwrap();
    assert!(path.ends_with(".lania/preferences.json"));
}

#[test]
fn preferences_path_returns_none_when_home_is_empty() {
    let _guard = HOME_LOCK.lock().unwrap();
    let previous = env::var("HOME").ok();
    env::set_var("HOME", "");
    let path = preferences_path();
    restore_env("HOME", previous);
    assert!(path.is_none());
}

/// Helper: create a unique temp root and set HOME to it.
/// Returns (root_path, previous_HOME_value, _lock_guard).
fn with_isolated_home() -> (PathBuf, Option<String>, std::sync::MutexGuard<'static, ()>) {
    let guard = HOME_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "lania-prefs-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let previous = env::var("HOME").ok();
    env::set_var("HOME", root.display().to_string());
    (root, previous, guard)
}

fn restore_env(key: &str, previous: Option<String>) {
    match previous {
        Some(v) => env::set_var(key, v),
        None => env::remove_var(key),
    }
}

// ── load_preferences ──────────────────────────────────────────

#[test]
fn load_preferences_returns_defaults_when_file_missing() {
    let (root, prev, _guard) = with_isolated_home();
    let prefs = load_preferences();
    restore_env("HOME", prev);
    assert_eq!(prefs.locale, "en");
    assert_eq!(prefs.output_mode, "json");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_preferences_returns_defaults_when_file_is_invalid_json() {
    let (root, prev, _guard) = with_isolated_home();
    fs::create_dir_all(root.join(".lania")).expect("dir created");
    fs::write(root.join(".lania/preferences.json"), "not valid json").expect("file written");
    let prefs = load_preferences();
    restore_env("HOME", prev);
    assert_eq!(prefs.locale, "en");
    assert_eq!(prefs.output_mode, "json");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_preferences_reads_and_normalizes_valid_file() {
    let (root, prev, _guard) = with_isolated_home();
    fs::create_dir_all(root.join(".lania")).expect("dir created");
    fs::write(
        root.join(".lania/preferences.json"),
        r#"{"locale":"zh-CN","outputMode":"human","logTimestamps":true}"#,
    )
    .expect("file written");
    let prefs = load_preferences();
    restore_env("HOME", prev);
    assert_eq!(prefs.locale, "zh");
    assert_eq!(prefs.output_mode, "human");
    assert_eq!(prefs.log_timestamps, true);
    let _ = fs::remove_dir_all(root);
}

// ── save_preferences ──────────────────────────────────────────

#[test]
fn save_and_load_roundtrip_preserves_normalized_values() {
    let (root, prev, _guard) = with_isolated_home();

    let input = UserPreferences {
        locale: "zh-TW".into(),
        output_mode: "jsonl".into(),
        log_timestamps: true,
    };
    save_preferences(&input).expect("save succeeds");
    // Verify file was actually written
    let saved_path = root.join(".lania/preferences.json");
    assert!(saved_path.exists(), "preferences file should exist after save");
    let loaded = load_preferences();
    restore_env("HOME", prev);
    // Values should be normalized after round-trip
    assert_eq!(loaded.locale, "zh", "locale should be normalized to zh");
    assert_eq!(loaded.output_mode, "stream", "output_mode should be normalized to stream");
    assert_eq!(loaded.log_timestamps, true);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn save_preferences_creates_directory_and_file() {
    let (root, prev, _guard) = with_isolated_home();
    let prefs = UserPreferences::default();
    save_preferences(&prefs).expect("save succeeds");
    let path = root.join(".lania/preferences.json");
    assert!(path.exists(), "preferences file should exist");
    let content = fs::read_to_string(&path).expect("file readable");
    assert!(content.contains(r#""locale": "en""#));
    assert!(content.contains(r#""outputMode": "json""#));
    restore_env("HOME", prev);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn save_preferences_without_home_is_noop() {
    let _guard = HOME_LOCK.lock().unwrap();
    let previous = env::var("HOME").ok();
    env::remove_var("HOME");
    let result = save_preferences(&UserPreferences::default());
    restore_env("HOME", previous);
    assert!(result.is_ok(), "should return Ok even without HOME");
}
