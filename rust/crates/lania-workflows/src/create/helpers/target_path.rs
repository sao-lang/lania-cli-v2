use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use super::super::capability::RenderedAddTemplate;

pub(crate) fn resolve_add_output_path(
    cwd: &Path,
    target: &str,
    prompt_name: Option<&str>,
    rendered: &RenderedAddTemplate,
) -> Result<PathBuf> {
    let target_path = cwd.join(target);
    if is_explicit_file_path(Path::new(target)) {
        return Ok(target_path);
    }

    let file_name = if let Some(filename) = &rendered.filename {
        filename.clone()
    } else {
        let base = prompt_name.unwrap_or("index");
        match &rendered.extname {
            Some(extname) => format!("{base}.{extname}"),
            None => base.to_string(),
        }
    };
    Ok(target_path.join(file_name))
}

fn is_explicit_file_path(path: &Path) -> bool {
    path.extension().is_some()
        || path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.starts_with('.'))
}

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
