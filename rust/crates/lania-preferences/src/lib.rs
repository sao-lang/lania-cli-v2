//! User-level persisted preferences (e.g. global locale and output behavior).
//!
//! Storage:
//! - `~/.lania/preferences.json` (best-effort; falls back to defaults if unreadable)
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

const DEFAULT_LOCALE: &str = "en";
const DEFAULT_OUTPUT_MODE: &str = "json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPreferences {
    pub locale: String,
    #[serde(default = "default_output_mode")]
    pub output_mode: String,
    #[serde(default)]
    pub log_timestamps: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            locale: DEFAULT_LOCALE.into(),
            output_mode: DEFAULT_OUTPUT_MODE.into(),
            log_timestamps: false,
        }
    }
}

fn default_output_mode() -> String {
    DEFAULT_OUTPUT_MODE.into()
}

pub fn normalize_locale(value: &str) -> String {
    let raw = value.trim().to_ascii_lowercase();
    if raw == "zh" || raw.starts_with("zh-") {
        "zh".into()
    } else if raw == "en" || raw.starts_with("en-") {
        "en".into()
    } else {
        DEFAULT_LOCALE.into()
    }
}

pub fn normalize_output_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "stream" | "jsonl" => "stream".into(),
        "human" => "human".into(),
        _ => "json".into(),
    }
}

pub fn preferences_path() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    if home.trim().is_empty() {
        return None;
    }
    Some(
        Path::new(home.trim())
            .join(".lania")
            .join("preferences.json"),
    )
}

pub fn load_preferences() -> UserPreferences {
    let Some(path) = preferences_path() else {
        return UserPreferences::default();
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        return UserPreferences::default();
    };
    let Ok(mut prefs) = serde_json::from_str::<UserPreferences>(&raw) else {
        return UserPreferences::default();
    };
    prefs.locale = normalize_locale(&prefs.locale);
    prefs.output_mode = normalize_output_mode(&prefs.output_mode);
    prefs
}

pub fn save_preferences(prefs: &UserPreferences) -> Result<()> {
    let Some(path) = preferences_path() else {
        // No HOME available; treat as no-op success so CLI can continue.
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut normalized = prefs.clone();
    normalized.locale = normalize_locale(&normalized.locale);
    normalized.output_mode = normalize_output_mode(&normalized.output_mode);
    let json = serde_json::to_string_pretty(&normalized)?;
    fs::write(path, format!("{json}\n"))?;
    Ok(())
}
