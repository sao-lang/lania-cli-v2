use serde_json::{json, Map, Value};

use super::{
    get_value, load_package_config, package_config_path, save_package_config, set_value,
    validate_package_name, ConfigKey, ConfigValueKind,
};

#[test]
fn package_name_rejects_traversal_and_weird_names() {
    for bad in [
        "",
        "Env",
        "env.json",
        "../env",
        "..",
        "a/b",
        "a b",
        "-env",
        "env_2",
        "env..",
    ] {
        assert!(validate_package_name(bad).is_err(), "must reject `{bad}`");
    }
    assert!(validate_package_name("env").is_ok());
    assert!(validate_package_name("lan-env").is_ok());
    assert!(validate_package_name("pi2").is_ok());
}

#[test]
fn path_is_scoped_under_home() {
    // 用环境变量守卫，只在测试进程内生效。
    let guard = HomeGuard::set("/tmp/lania-pkg-config-home");
    let path = package_config_path("env").expect("some path").expect("path");
    assert!(path.starts_with("/tmp/lania-pkg-config-home/.lania/config/"));
    assert_eq!(path.file_name().unwrap(), "env.json");
    drop(guard);
}

#[test]
fn roundtrip_through_home() {
    let guard = HomeGuard::set(&tmpdir("roundtrip"));
    save_package_config("env", &Map::new()).expect("save empty");
    assert_eq!(
        load_package_config("env").expect("load"),
        Map::<String, Value>::new()
    );

    let mut config = load_package_config("env").expect("load");
    set_value(&mut config, "shim.host", json!("runex")).expect("set");
    save_package_config("env", &config).expect("save");

    let config = load_package_config("env").expect("reload");
    assert_eq!(
        get_value(&config, "shim.host"),
        Some(&json!("runex")),
        "value must roundtrip through the isolated per-package file"
    );
}

#[test]
fn missing_file_is_empty_map() {
    let guard = HomeGuard::set(&tmpdir("missing"));
    assert_eq!(
        load_package_config("never-installed").expect("load"),
        Map::<String, Value>::new()
    );
}#[test]
fn dotted_get_set_and_conflicts() {
    let mut config = Map::new();
    set_value(&mut config, "a.b.c", json!(1)).expect("set nested");
    assert_eq!(get_value(&config, "a.b.c"), Some(&json!(1)));
    assert_eq!(get_value(&config, "a.b"), Some(&json!({"c": 1})));
    assert_eq!(get_value(&config, "a.b.d"), None);
    assert_eq!(get_value(&config, "a.b.c.d"), None);
    assert_eq!(get_value(&config, "missing"), None);

    // 标量当中段 → 冲突报错，且不破坏原值。
    set_value(&mut config, "x", json!("scalar")).expect("set scalar");
    assert!(set_value(&mut config, "x.y", json!(1)).is_err());
    assert_eq!(get_value(&config, "x"), Some(&json!("scalar")));

    // 空 key 段拒绝。
    assert!(set_value(&mut config, "a..b", json!(1)).is_err());
}

#[test]
fn config_key_parse_validates() {
    let enum_key = ConfigKey::enumeration("shim.host", &["lan", "runex"], "lan");
    assert_eq!(enum_key.parse("runex").expect("ok"), json!("runex"));
    assert_eq!(enum_key.parse(" RUNEX ").expect("normalized"), json!("runex"));
    assert!(enum_key.parse("bash").is_err());

    let bool_key = ConfigKey::boolean("flag", false);
    assert_eq!(bool_key.parse("on").expect("ok"), json!(true));
    assert_eq!(bool_key.parse("0").expect("ok"), json!(false));
    assert!(bool_key.parse("maybe").is_err());

    let string_key = ConfigKey::string("name", "");
    assert_eq!(string_key.parse(" node ").expect("ok"), json!("node"));
    assert!(string_key.parse("   ").is_err());
}

#[test]
fn config_key_serializes_for_manifest() {
    let key = ConfigKey::enumeration("shim.host", &["lan", "runex"], "lan");
    let value = serde_json::to_value(&key).expect("serialize");
    assert_eq!(value["key"], "shim.host");
    assert_eq!(value["kind"], "enum");
    assert_eq!(value["choices"], json!(["lan", "runex"]));
    assert_eq!(value["default"], "lan");
    assert_eq!(ConfigValueKind::Bool, ConfigValueKind::Bool);
}

/// 临时目录（进程内全局计数，避免并行测试互相覆盖）。
fn tmpdir(label: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "lania-package-config-test-{}-{label}-{id}",
        std::process::id()
    ));
    std::fs::create_dir_all(&base).expect("create tmpdir");
    base.display().to_string()
}

/// 进程内 HOME 环境守卫：构造时改写并持有全局互斥（HOME 是进程级状态，
/// 并行测试同时改写会互相污染），drop 时还原。
struct HomeGuard {
    previous: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl HomeGuard {
    fn set(home: &str) -> Self {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let lock = LOCK.lock().expect("home lock");
        let previous = std::env::var_os("HOME");
        std::env::set_var("HOME", home);
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
}
