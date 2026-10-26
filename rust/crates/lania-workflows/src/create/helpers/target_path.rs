use std::path::Path;

use anyhow::{anyhow, Result};

pub(crate) fn normalize_add_target(target: &str) -> String {
    target.trim_start_matches('/').to_string()
}

pub(crate) fn validate_relative_target(target: &str) -> Result<()> {
    let path = Path::new(target);
    if path.is_absolute() {
        return Err(anyhow!("target path must be relative"));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(anyhow!("target path must not traverse parent directories"));
    }
    Ok(())
}
