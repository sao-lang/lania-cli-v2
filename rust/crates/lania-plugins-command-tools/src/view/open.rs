use anyhow::{anyhow, Result};
use lania_exec::ExecCommand;
use std::path::Path;

pub(super) fn open_with_system_command(path: &Path, cwd: &str) -> Result<ExecCommand> {
    let file = path
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))?;

    #[cfg(target_os = "macos")]
    let command = ExecCommand::new("open")
        .with_args(vec![file])
        .in_dir(cwd.to_string());
    #[cfg(target_os = "linux")]
    let command = ExecCommand::new("xdg-open")
        .with_args(vec![file])
        .in_dir(cwd.to_string());
    #[cfg(target_os = "windows")]
    let command = ExecCommand::new("cmd")
        .with_args(vec!["/C".into(), "start".into(), "".into(), file])
        .in_dir(cwd.to_string());

    Ok(command)
}
