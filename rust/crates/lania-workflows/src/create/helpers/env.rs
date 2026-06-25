use std::{fs, net::TcpListener, path::Path};

use anyhow::{anyhow, Context, Result};

pub(crate) fn ensure_directory_empty(dir: &Path) -> Result<()> {
    let mut entries =
        fs::read_dir(dir).with_context(|| format!("failed to read directory {}", dir.display()))?;
    if entries.next().transpose()?.is_some() {
        return Err(anyhow!(
            "current directory is not empty; use `--directory <name>` to create in a child directory"
        ));
    }
    Ok(())
}

pub(crate) fn find_available_port(host: &str, preferred: u16) -> u16 {
    const LEGACY_DEFAULT_DEV_PORT: u16 = 8089;
    const LEGACY_FALLBACK_START: u16 = 18089;
    const LEGACY_FALLBACK_END: u16 = 18999;

    if can_bind(host, preferred) {
        return preferred;
    }

    let mut candidates: Vec<u16> = Vec::new();
    if preferred == LEGACY_DEFAULT_DEV_PORT {
        candidates.extend(LEGACY_FALLBACK_START..=LEGACY_FALLBACK_END);
    } else {
        let start = preferred.saturating_add(1).max(1024);
        let end = preferred.saturating_add(20).max(1024);
        candidates.extend(start..=end);
    }

    for candidate in candidates {
        if candidate == preferred {
            continue;
        }
        if can_bind(host, candidate) {
            return candidate;
        }
    }

    TcpListener::bind((host, 0))
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
        .unwrap_or(preferred)
}

fn can_bind(host: &str, port: u16) -> bool {
    TcpListener::bind((host, port)).is_ok()
}
