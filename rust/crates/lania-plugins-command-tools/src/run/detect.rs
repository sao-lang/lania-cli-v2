use anyhow::{bail, Result};
use std::path::Path;

use super::RunFileKind;

pub(super) fn detect_run_file_kind(path: &Path) -> Result<RunFileKind> {
    let extension = path
        .extension()
        .and_then(|item| item.to_str())
        .map(|item| item.to_ascii_lowercase());
    match extension.as_deref() {
        Some("js" | "mjs" | "cjs") => Ok(RunFileKind::JavaScript),
        Some("ts" | "tsx" | "jsx") => Ok(RunFileKind::TypeScript),
        Some("py") => Ok(RunFileKind::Python),
        Some("sh") => Ok(RunFileKind::Shell("sh")),
        Some("bash") => Ok(RunFileKind::Shell("bash")),
        Some("zsh") => Ok(RunFileKind::Shell("zsh")),
        Some("rb") => Ok(RunFileKind::Ruby),
        Some("php") => Ok(RunFileKind::Php),
        Some("go") => Ok(RunFileKind::Go),
        Some("java") => Ok(RunFileKind::Java),
        Some("rs") => Ok(RunFileKind::Rust),
        Some("c") => Ok(RunFileKind::C),
        Some("lua") => Ok(RunFileKind::Lua),
        Some("dart") => Ok(RunFileKind::Dart),
        Some("nim") => Ok(RunFileKind::Nim),
        Some("zig") => Ok(RunFileKind::Zig),
        Some("kt" | "kts") => Ok(RunFileKind::Kotlin),
        Some("swift") => Ok(RunFileKind::Swift),
        _ => bail!(
            "unsupported runnable file type for {} (supported: js/mjs/cjs/ts/tsx/jsx/py/sh/bash/zsh/rb/php/go/java/rs/c/lua/dart/nim/zig/kt/kts/swift)",
            path.display()
        ),
    }
}
