//! 包管理器识别与安装、执行命令规划。
//!
//! 主要导出：binary、lockfile、detect_from_files、detect_from_cwd、spec、supported_managers、init_command、
//! 以及 package.json scripts 的读取与校验辅助方法。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
//! - `lania-pm` 负责“命令规划与最小校验”，不直接执行外部命令（执行交给 `lania-exec`）
use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{anyhow, Context, Result};
use lania_exec::ExecCommand;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    pub fn binary(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    pub fn lockfile(self) -> &'static str {
        match self {
            Self::Npm => "package-lock.json",
            Self::Pnpm => "pnpm-lock.yaml",
            Self::Yarn => "yarn.lock",
            Self::Bun => "bun.lockb",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl PackageCommand {
    pub fn to_exec_command(&self) -> ExecCommand {
        ExecCommand::new(self.program.clone()).with_args(self.args.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManagerSpec {
    pub manager: PackageManager,
    pub binary: String,
    pub init_subcommand: String,
    pub add_subcommand: String,
    pub install_subcommand: String,
    pub remove_subcommand: String,
    pub update_subcommand: String,
    pub run_subcommand: String,
    pub save_flag: Option<String>,
    pub save_dev_flag: String,
    pub silent_flag: Option<String>,
    pub strict_peer_flag: Option<String>,
    pub init_flag: Option<String>,
    pub run_separator: Option<String>,
    pub lockfile: String,
}

#[derive(Debug, Clone, Default)]
pub struct PackageManagerService;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PackageJsonSnapshot {
    pub path: String,
    pub exists: bool,
    pub scripts: BTreeMap<String, String>,
    pub raw: Value,
}

impl PackageManagerService {
    pub fn detect_from_files<I, S>(&self, files: I) -> PackageManager
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let names: Vec<String> = files
            .into_iter()
            .map(|file| file.as_ref().to_string())
            .collect();

        if names.iter().any(|file| file == "pnpm-lock.yaml") {
            return PackageManager::Pnpm;
        }
        if names.iter().any(|file| file == "yarn.lock") {
            return PackageManager::Yarn;
        }
        if names
            .iter()
            .any(|file| file == "bun.lockb" || file == "bun.lock")
        {
            return PackageManager::Bun;
        }

        PackageManager::Npm
    }

    pub fn detect_from_cwd(&self, cwd: impl AsRef<Path>) -> PackageManager {
        let cwd = cwd.as_ref();
        // Keep behavior aligned with existing lockfile precedence.
        let candidates = [
            cwd.join("pnpm-lock.yaml"),
            cwd.join("yarn.lock"),
            cwd.join("bun.lockb"),
            cwd.join("bun.lock"),
            cwd.join("package-lock.json"),
        ];
        let file_names = candidates
            .iter()
            .filter(|path| path.exists())
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        self.detect_from_files(file_names)
    }

    pub fn spec(&self, manager: PackageManager) -> PackageManagerSpec {
        match manager {
            PackageManager::Npm => PackageManagerSpec {
                manager,
                binary: "npm".into(),
                init_subcommand: "init".into(),
                add_subcommand: "install".into(),
                install_subcommand: "install".into(),
                remove_subcommand: "uninstall".into(),
                update_subcommand: "update".into(),
                run_subcommand: "run".into(),
                save_flag: Some("--save".into()),
                save_dev_flag: "--save-dev".into(),
                silent_flag: Some("--silent".into()),
                strict_peer_flag: Some("--legacy-peer-deps".into()),
                init_flag: Some("-y".into()),
                run_separator: Some("--".into()),
                lockfile: manager.lockfile().into(),
            },
            PackageManager::Pnpm => PackageManagerSpec {
                manager,
                binary: "pnpm".into(),
                init_subcommand: "init".into(),
                add_subcommand: "install".into(),
                install_subcommand: "install".into(),
                remove_subcommand: "remove".into(),
                update_subcommand: "update".into(),
                run_subcommand: "run".into(),
                save_flag: Some("--save".into()),
                save_dev_flag: "--save-dev".into(),
                silent_flag: Some("--reporter=silent".into()),
                strict_peer_flag: Some("--strict-peer-dependencies=false".into()),
                init_flag: None,
                run_separator: Some("--".into()),
                lockfile: manager.lockfile().into(),
            },
            PackageManager::Yarn => PackageManagerSpec {
                manager,
                binary: "yarn".into(),
                init_subcommand: "init".into(),
                add_subcommand: "add".into(),
                install_subcommand: "install".into(),
                remove_subcommand: "remove".into(),
                update_subcommand: "upgrade".into(),
                run_subcommand: "run".into(),
                save_flag: None,
                save_dev_flag: "--dev".into(),
                silent_flag: Some("--silent".into()),
                strict_peer_flag: None,
                init_flag: Some("-y".into()),
                run_separator: None,
                lockfile: manager.lockfile().into(),
            },
            PackageManager::Bun => PackageManagerSpec {
                manager,
                binary: "bun".into(),
                init_subcommand: "init".into(),
                add_subcommand: "add".into(),
                install_subcommand: "install".into(),
                remove_subcommand: "remove".into(),
                update_subcommand: "update".into(),
                run_subcommand: "run".into(),
                save_flag: None,
                save_dev_flag: "--dev".into(),
                silent_flag: Some("--silent".into()),
                strict_peer_flag: None,
                init_flag: Some("-y".into()),
                run_separator: None,
                lockfile: manager.lockfile().into(),
            },
        }
    }

    pub fn supported_managers(&self) -> Vec<PackageManager> {
        vec![
            PackageManager::Npm,
            PackageManager::Pnpm,
            PackageManager::Yarn,
            PackageManager::Bun,
        ]
    }

    pub fn init_command(&self, manager: PackageManager) -> PackageCommand {
        let spec = self.spec(manager);
        let mut args = vec![spec.init_subcommand];
        if let Some(flag) = spec.init_flag {
            if !flag.is_empty() {
                args.push(flag);
            }
        }
        PackageCommand {
            program: spec.binary,
            args,
        }
    }

    pub fn install_all_command(&self, manager: PackageManager) -> PackageCommand {
        let spec = self.spec(manager);
        let mut args = vec![spec.install_subcommand];
        if let Some(flag) = spec.strict_peer_flag {
            args.push(flag);
        }
        PackageCommand {
            program: spec.binary,
            args,
        }
    }

    pub fn install_command(
        &self,
        manager: PackageManager,
        packages: &[String],
        dev: bool,
    ) -> PackageCommand {
        let spec = self.spec(manager);
        let mut args = vec![spec.add_subcommand];
        if let Some(flag) = spec.strict_peer_flag {
            args.push(flag);
        }
        if dev {
            args.push(spec.save_dev_flag);
        } else if let Some(flag) = spec.save_flag {
            if manager == PackageManager::Npm {
                args.push(flag);
            }
        }
        args.extend(packages.iter().cloned());
        PackageCommand {
            program: spec.binary,
            args,
        }
    }

    pub fn remove_command(&self, manager: PackageManager, packages: &[String]) -> PackageCommand {
        let spec = self.spec(manager);
        let mut args = vec![spec.remove_subcommand];
        args.extend(packages.iter().cloned());
        PackageCommand {
            program: spec.binary,
            args,
        }
    }

    pub fn update_command(&self, manager: PackageManager, packages: &[String]) -> PackageCommand {
        let spec = self.spec(manager);
        let mut args = vec![spec.update_subcommand];
        args.extend(packages.iter().cloned());
        PackageCommand {
            program: spec.binary,
            args,
        }
    }

    pub fn run_script_command(
        &self,
        manager: PackageManager,
        script: &str,
        extra_args: &[String],
    ) -> PackageCommand {
        let spec = self.spec(manager);
        let mut args = vec![spec.run_subcommand, script.to_string()];
        if let Some(separator) = spec.run_separator {
            if !extra_args.is_empty() {
                args.push(separator);
            }
        }
        args.extend(extra_args.iter().cloned());
        PackageCommand {
            program: spec.binary,
            args,
        }
    }

    pub fn publish_command(&self, manager: PackageManager, tag: Option<&str>) -> PackageCommand {
        let spec = self.spec(manager);
        let mut args = vec!["publish".to_string()];
        if let Some(tag) = tag {
            args.push("--tag".into());
            args.push(tag.to_string());
        }
        PackageCommand {
            program: spec.binary,
            args,
        }
    }

    // -----------------------------
    // package.json scripts helpers
    // -----------------------------

    pub fn load_package_json_snapshot(&self, cwd: impl AsRef<Path>) -> Result<PackageJsonSnapshot> {
        let cwd = cwd.as_ref();
        let path = cwd.join("package.json");
        if !path.exists() {
            return Ok(PackageJsonSnapshot {
                path: path.display().to_string(),
                exists: false,
                scripts: BTreeMap::new(),
                raw: Value::Null,
            });
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let raw: Value = serde_json::from_str(&content)
            .with_context(|| format!("invalid json in {}", path.display()))?;
        let scripts = raw
            .get("scripts")
            .and_then(|value| value.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(key, value)| {
                        value
                            .as_str()
                            .map(|script| (key.clone(), script.to_string()))
                    })
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        Ok(PackageJsonSnapshot {
            path: path.display().to_string(),
            exists: true,
            scripts,
            raw,
        })
    }

    pub fn script_exists(&self, cwd: impl AsRef<Path>, script: &str) -> Result<bool> {
        let snapshot = self.load_package_json_snapshot(cwd)?;
        Ok(snapshot.exists && snapshot.scripts.contains_key(script))
    }

    pub fn require_script(&self, cwd: impl AsRef<Path>, script: &str) -> Result<()> {
        let snapshot = self.load_package_json_snapshot(cwd.as_ref())?;
        if !snapshot.exists {
            return Err(anyhow!(
                "package.json not found in {}",
                snapshot
                    .path
                    .rsplit_once('/')
                    .map(|(d, _)| d)
                    .unwrap_or(&snapshot.path)
            ));
        }
        if snapshot.scripts.contains_key(script) {
            return Ok(());
        }
        let available = snapshot
            .scripts
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        Err(anyhow!(
            "script `{}` not found in package.json (available: {})",
            script,
            if available.is_empty() {
                "none"
            } else {
                &available
            }
        ))
    }

    pub fn run_script_command_checked(
        &self,
        cwd: impl AsRef<Path>,
        manager: PackageManager,
        script: &str,
        extra_args: &[String],
    ) -> Result<PackageCommand> {
        // This is an opt-in helper. Callers can keep legacy behavior by using `run_script_command`.
        self.require_script(cwd, script)?;
        Ok(self.run_script_command(manager, script, extra_args))
    }

    pub fn add_dependency_commands(
        &self,
        manager: PackageManager,
        dependencies: &[String],
        dev_dependencies: &[String],
    ) -> Vec<PackageCommand> {
        let mut commands = Vec::new();
        if !dependencies.is_empty() {
            commands.push(self.install_command(manager, dependencies, false));
        }
        if !dev_dependencies.is_empty() {
            commands.push(self.install_command(manager, dev_dependencies, true));
        }
        commands
    }

    pub fn lockfile_strategy(&self, manager: PackageManager) -> String {
        format!("{} uses {}", manager.binary(), manager.lockfile())
    }
}

#[cfg(test)]
mod tests;
