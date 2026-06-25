use anyhow::{bail, Result};
use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
};

pub(super) fn choose_first_available(
    path: &Path,
    cwd: &Path,
    candidates: &[(&str, Vec<String>)],
) -> Result<(String, Vec<String>)> {
    for (program, args) in candidates {
        if let Some(resolved) = resolve_command(program, path, cwd) {
            return Ok((resolved, args.clone()));
        }
    }
    let tried = candidates
        .iter()
        .map(|(program, _)| *program)
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "no suitable runtime found for {} (tried: {})",
        path.display(),
        tried
    )
}

pub(super) fn resolve_command(program: &str, script_path: &Path, cwd: &Path) -> Option<String> {
    let candidate = PathBuf::from(program);
    if candidate.components().count() > 1 {
        return candidate.is_file().then(|| program.to_string());
    }

    for base in search_roots(script_path, cwd) {
        let local = base.join("node_modules").join(".bin").join(program);
        if local.is_file() {
            return Some(local.display().to_string());
        }
    }

    let path_value = env::var_os("PATH")?;
    env::split_paths(&path_value)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
        .map(|candidate| candidate.display().to_string())
}

fn search_roots(script_path: &Path, cwd: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    let mut push_ancestors = |start: &Path| {
        for ancestor in start.ancestors() {
            let owned = ancestor.to_path_buf();
            if seen.insert(owned.clone()) {
                roots.push(owned);
            }
        }
    };

    let script_dir = script_path.parent().unwrap_or(cwd);
    push_ancestors(script_dir);
    push_ancestors(cwd);
    roots
}
