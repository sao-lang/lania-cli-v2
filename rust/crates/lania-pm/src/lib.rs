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

const NODE_MANAGERS: &[PackageManager] = &[
    PackageManager::Npm,
    PackageManager::Pnpm,
    PackageManager::Yarn,
    PackageManager::Bun,
];
const PYTHON_MANAGERS: &[PackageManager] = &[
    PackageManager::Pip,
    PackageManager::Uv,
    PackageManager::Poetry,
];
const RUST_MANAGERS: &[PackageManager] = &[PackageManager::Cargo];
const GO_MANAGERS: &[PackageManager] = &[PackageManager::Go];
const DART_MANAGERS: &[PackageManager] = &[PackageManager::Pub];
const RUBY_MANAGERS: &[PackageManager] = &[PackageManager::Bundler];
const PHP_MANAGERS: &[PackageManager] = &[PackageManager::Composer];
const NO_MANAGERS: &[PackageManager] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
    Pip,
    Uv,
    Poetry,
    Cargo,
    Go,
    Pub,
    Bundler,
    Composer,
}

impl PackageManager {
    pub fn all() -> &'static [PackageManager] {
        &[
            Self::Npm,
            Self::Pnpm,
            Self::Yarn,
            Self::Bun,
            Self::Pip,
            Self::Uv,
            Self::Poetry,
            Self::Cargo,
            Self::Go,
            Self::Pub,
            Self::Bundler,
            Self::Composer,
        ]
    }

    /// 对外稳定标识（`host.pm.*` 与 `ctx.tools.pm` 使用）。
    pub fn id(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
            Self::Pip => "pip",
            Self::Uv => "uv",
            Self::Poetry => "poetry",
            Self::Cargo => "cargo",
            Self::Go => "go",
            Self::Pub => "pub",
            Self::Bundler => "bundler",
            Self::Composer => "composer",
        }
    }

    /// 实际执行的程序名。
    pub fn binary(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
            Self::Pip => "python",
            Self::Uv => "uv",
            Self::Poetry => "poetry",
            Self::Cargo => "cargo",
            Self::Go => "go",
            Self::Pub => "dart",
            Self::Bundler => "bundle",
            Self::Composer => "composer",
        }
    }

    pub fn language(self) -> &'static str {
        match self {
            Self::Npm | Self::Pnpm | Self::Yarn | Self::Bun => "node",
            Self::Pip | Self::Uv | Self::Poetry => "python",
            Self::Cargo => "rust",
            Self::Go => "go",
            Self::Pub => "dart",
            Self::Bundler => "ruby",
            Self::Composer => "php",
        }
    }

    pub fn lockfile(self) -> &'static str {
        match self {
            Self::Npm => "package-lock.json",
            Self::Pnpm => "pnpm-lock.yaml",
            Self::Yarn => "yarn.lock",
            Self::Bun => "bun.lockb",
            Self::Pip => "requirements.txt",
            Self::Uv => "uv.lock",
            Self::Poetry => "poetry.lock",
            Self::Cargo => "Cargo.lock",
            Self::Go => "go.sum",
            Self::Pub => "pubspec.lock",
            Self::Bundler => "Gemfile.lock",
            Self::Composer => "composer.lock",
        }
    }

    pub fn markers(self) -> &'static [&'static str] {
        match self {
            Self::Npm => &["package.json", "package-lock.json"],
            Self::Pnpm => &["pnpm-lock.yaml"],
            Self::Yarn => &["yarn.lock"],
            Self::Bun => &["bun.lockb", "bun.lock"],
            Self::Pip => &["requirements.txt", "pyproject.toml"],
            Self::Uv => &["uv.lock", "pyproject.toml"],
            Self::Poetry => &["poetry.lock", "pyproject.toml"],
            Self::Cargo => &["Cargo.toml", "Cargo.lock"],
            Self::Go => &["go.mod", "go.sum"],
            Self::Pub => &["pubspec.yaml", "pubspec.lock"],
            Self::Bundler => &["Gemfile", "Gemfile.lock"],
            Self::Composer => &["composer.json", "composer.lock"],
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "npm" | "npmjs" => Some(Self::Npm),
            "pnpm" => Some(Self::Pnpm),
            "yarn" => Some(Self::Yarn),
            "bun" => Some(Self::Bun),
            "pip" | "pip3" => Some(Self::Pip),
            "uv" => Some(Self::Uv),
            "poetry" => Some(Self::Poetry),
            "cargo" => Some(Self::Cargo),
            "go" | "golang" => Some(Self::Go),
            "pub" | "dart" => Some(Self::Pub),
            "bundler" | "bundle" => Some(Self::Bundler),
            "composer" => Some(Self::Composer),
            _ => None,
        }
    }

    pub fn managers_for_language(language: &str) -> &'static [PackageManager] {
        match language.trim().to_ascii_lowercase().as_str() {
            "node" => NODE_MANAGERS,
            "python" => PYTHON_MANAGERS,
            "rust" => RUST_MANAGERS,
            "go" => GO_MANAGERS,
            "dart" => DART_MANAGERS,
            "ruby" => RUBY_MANAGERS,
            "php" => PHP_MANAGERS,
            _ => NO_MANAGERS,
        }
    }

    pub fn default_for_language(language: &str) -> Option<Self> {
        Self::managers_for_language(language).first().copied()
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
    pub language: String,
    pub binary: String,
    pub init_subcommand: Option<String>,
    pub add_subcommand: Option<String>,
    pub install_subcommand: Option<String>,
    pub remove_subcommand: Option<String>,
    pub update_subcommand: Option<String>,
    pub update_all_subcommand: Option<String>,
    pub update_package_flag: Option<String>,
    pub run_subcommand: Option<String>,
    pub publish_subcommand: Option<String>,
    pub save_flag: Option<String>,
    pub save_dev_flag: Option<String>,
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
        let has = |name: &str| names.iter().any(|file| file == name);

        if has("pnpm-lock.yaml") {
            return PackageManager::Pnpm;
        }
        if has("yarn.lock") {
            return PackageManager::Yarn;
        }
        if has("bun.lockb") || has("bun.lock") {
            return PackageManager::Bun;
        }
        if has("uv.lock") {
            return PackageManager::Uv;
        }
        if has("poetry.lock") {
            return PackageManager::Poetry;
        }
        if has("Cargo.lock") {
            return PackageManager::Cargo;
        }
        if has("go.sum") {
            return PackageManager::Go;
        }
        if has("pubspec.lock") {
            return PackageManager::Pub;
        }
        if has("Gemfile.lock") {
            return PackageManager::Bundler;
        }
        if has("composer.lock") {
            return PackageManager::Composer;
        }
        if has("package-lock.json") {
            return PackageManager::Npm;
        }

        if has("Cargo.toml") {
            return PackageManager::Cargo;
        }
        if has("go.mod") {
            return PackageManager::Go;
        }
        if has("pubspec.yaml") {
            return PackageManager::Pub;
        }
        if has("Gemfile") {
            return PackageManager::Bundler;
        }
        if has("composer.json") {
            return PackageManager::Composer;
        }
        if has("package.json") {
            return PackageManager::Npm;
        }
        if has("pyproject.toml") || has("requirements.txt") {
            return PackageManager::Pip;
        }

        PackageManager::Npm
    }

    pub fn detect_from_cwd(&self, cwd: impl AsRef<Path>) -> PackageManager {
        let cwd = cwd.as_ref();
        let exists = |name: &str| cwd.join(name).exists();

        if exists("pnpm-lock.yaml") {
            return PackageManager::Pnpm;
        }
        if exists("yarn.lock") {
            return PackageManager::Yarn;
        }
        if exists("bun.lockb") || exists("bun.lock") {
            return PackageManager::Bun;
        }
        if exists("uv.lock") {
            return PackageManager::Uv;
        }
        if exists("poetry.lock") {
            return PackageManager::Poetry;
        }
        if exists("Cargo.lock") {
            return PackageManager::Cargo;
        }
        if exists("go.sum") {
            return PackageManager::Go;
        }
        if exists("pubspec.lock") {
            return PackageManager::Pub;
        }
        if exists("Gemfile.lock") {
            return PackageManager::Bundler;
        }
        if exists("composer.lock") {
            return PackageManager::Composer;
        }
        if exists("package-lock.json") {
            return PackageManager::Npm;
        }

        if exists("Cargo.toml") {
            return PackageManager::Cargo;
        }
        if exists("go.mod") {
            return PackageManager::Go;
        }
        if exists("pubspec.yaml") {
            return PackageManager::Pub;
        }
        if exists("Gemfile") {
            return PackageManager::Bundler;
        }
        if exists("composer.json") {
            return PackageManager::Composer;
        }
        if exists("package.json") {
            return PackageManager::Npm;
        }
        if exists("pyproject.toml") {
            return python_manager_from_pyproject(cwd);
        }
        if exists("requirements.txt") {
            return PackageManager::Pip;
        }

        PackageManager::Npm
    }

    /// 按明确的语言选择包管理器；未知语言返回 `None`。
    pub fn detect_for_language(
        &self,
        cwd: impl AsRef<Path>,
        language: &str,
    ) -> Option<PackageManager> {
        let cwd = cwd.as_ref();
        let exists = |name: &str| cwd.join(name).exists();

        match language.trim().to_ascii_lowercase().as_str() {
            "node" => {
                if exists("pnpm-lock.yaml") {
                    Some(PackageManager::Pnpm)
                } else if exists("yarn.lock") {
                    Some(PackageManager::Yarn)
                } else if exists("bun.lockb") || exists("bun.lock") {
                    Some(PackageManager::Bun)
                } else {
                    Some(PackageManager::Npm)
                }
            }
            "python" => {
                if exists("uv.lock") {
                    Some(PackageManager::Uv)
                } else if exists("poetry.lock") {
                    Some(PackageManager::Poetry)
                } else if exists("pyproject.toml") {
                    Some(python_manager_from_pyproject(cwd))
                } else if exists("requirements.txt") {
                    Some(PackageManager::Pip)
                } else {
                    Some(PackageManager::Pip)
                }
            }
            "rust" => Some(PackageManager::Cargo),
            "go" => Some(PackageManager::Go),
            "dart" => Some(PackageManager::Pub),
            "ruby" => Some(PackageManager::Bundler),
            "php" => Some(PackageManager::Composer),
            _ => None,
        }
    }

    pub fn spec(&self, manager: PackageManager) -> PackageManagerSpec {
        let language = manager.language().to_string();
        let lockfile = manager.lockfile().to_string();
        let base = || PackageManagerSpec {
            manager,
            language: language.clone(),
            binary: manager.binary().to_string(),
            init_subcommand: None,
            add_subcommand: None,
            install_subcommand: None,
            remove_subcommand: None,
            update_subcommand: None,
            update_all_subcommand: None,
            update_package_flag: None,
            run_subcommand: None,
            publish_subcommand: None,
            save_flag: None,
            save_dev_flag: None,
            silent_flag: None,
            strict_peer_flag: None,
            init_flag: None,
            run_separator: None,
            lockfile: lockfile.clone(),
        };

        match manager {
            PackageManager::Npm => PackageManagerSpec {
                manager,
                language,
                binary: "npm".into(),
                init_subcommand: Some("init".into()),
                add_subcommand: Some("install".into()),
                install_subcommand: Some("install".into()),
                remove_subcommand: Some("uninstall".into()),
                update_subcommand: Some("update".into()),
                update_all_subcommand: Some("update".into()),
                update_package_flag: None,
                run_subcommand: Some("run".into()),
                publish_subcommand: Some("publish".into()),
                save_flag: Some("--save".into()),
                save_dev_flag: Some("--save-dev".into()),
                silent_flag: Some("--silent".into()),
                strict_peer_flag: Some("--legacy-peer-deps".into()),
                init_flag: Some("-y".into()),
                run_separator: Some("--".into()),
                lockfile,
            },
            PackageManager::Pnpm => PackageManagerSpec {
                manager,
                language,
                binary: "pnpm".into(),
                init_subcommand: Some("init".into()),
                add_subcommand: Some("install".into()),
                install_subcommand: Some("install".into()),
                remove_subcommand: Some("remove".into()),
                update_subcommand: Some("update".into()),
                update_all_subcommand: Some("update".into()),
                update_package_flag: None,
                run_subcommand: Some("run".into()),
                publish_subcommand: Some("publish".into()),
                save_flag: Some("--save".into()),
                save_dev_flag: Some("--save-dev".into()),
                silent_flag: Some("--reporter=silent".into()),
                strict_peer_flag: Some("--strict-peer-dependencies=false".into()),
                init_flag: None,
                run_separator: Some("--".into()),
                lockfile,
            },
            PackageManager::Yarn => PackageManagerSpec {
                manager,
                language,
                binary: "yarn".into(),
                init_subcommand: Some("init".into()),
                add_subcommand: Some("add".into()),
                install_subcommand: Some("install".into()),
                remove_subcommand: Some("remove".into()),
                update_subcommand: Some("upgrade".into()),
                update_all_subcommand: Some("upgrade".into()),
                update_package_flag: None,
                run_subcommand: Some("run".into()),
                publish_subcommand: Some("publish".into()),
                save_flag: None,
                save_dev_flag: Some("--dev".into()),
                silent_flag: Some("--silent".into()),
                strict_peer_flag: None,
                init_flag: Some("-y".into()),
                run_separator: None,
                lockfile,
            },
            PackageManager::Bun => PackageManagerSpec {
                manager,
                language,
                binary: "bun".into(),
                init_subcommand: Some("init".into()),
                add_subcommand: Some("add".into()),
                install_subcommand: Some("install".into()),
                remove_subcommand: Some("remove".into()),
                update_subcommand: Some("update".into()),
                update_all_subcommand: Some("update".into()),
                update_package_flag: None,
                run_subcommand: Some("run".into()),
                publish_subcommand: Some("publish".into()),
                save_flag: None,
                save_dev_flag: Some("--dev".into()),
                silent_flag: Some("--silent".into()),
                strict_peer_flag: None,
                init_flag: Some("-y".into()),
                run_separator: None,
                lockfile,
            },
            PackageManager::Pip => PackageManagerSpec {
                binary: "python".into(),
                add_subcommand: Some("-m pip install".into()),
                install_subcommand: Some(
                    "-m pip install -r requirements.txt -r requirements-dev.txt".into(),
                ),
                remove_subcommand: Some("-m pip uninstall".into()),
                update_subcommand: Some("-m pip install --upgrade".into()),
                update_all_subcommand: Some("-m pip install --upgrade -r requirements.txt".into()),
                ..base()
            },
            PackageManager::Uv => PackageManagerSpec {
                init_subcommand: Some("init".into()),
                add_subcommand: Some("add".into()),
                install_subcommand: Some(
                    "pip install -r requirements.txt -r requirements-dev.txt".into(),
                ),
                remove_subcommand: Some("remove".into()),
                update_subcommand: Some("lock --upgrade".into()),
                update_all_subcommand: Some("lock --upgrade".into()),
                update_package_flag: Some("--upgrade-package".into()),
                run_subcommand: Some("run".into()),
                publish_subcommand: Some("publish".into()),
                save_dev_flag: Some("--dev".into()),
                ..base()
            },
            PackageManager::Poetry => PackageManagerSpec {
                init_subcommand: Some("init".into()),
                add_subcommand: Some("add".into()),
                install_subcommand: Some("install".into()),
                remove_subcommand: Some("remove".into()),
                update_subcommand: Some("update".into()),
                update_all_subcommand: Some("update".into()),
                run_subcommand: Some("run".into()),
                publish_subcommand: Some("publish".into()),
                save_dev_flag: Some("--group dev".into()),
                ..base()
            },
            PackageManager::Cargo => PackageManagerSpec {
                init_subcommand: Some("init".into()),
                add_subcommand: Some("add".into()),
                install_subcommand: Some("fetch".into()),
                remove_subcommand: Some("remove".into()),
                update_subcommand: Some("update".into()),
                update_all_subcommand: Some("update".into()),
                run_subcommand: Some("run".into()),
                publish_subcommand: Some("publish".into()),
                save_dev_flag: Some("--dev".into()),
                ..base()
            },
            PackageManager::Go => PackageManagerSpec {
                add_subcommand: Some("get".into()),
                install_subcommand: Some("mod tidy".into()),
                update_subcommand: Some("get -u".into()),
                update_all_subcommand: Some("get -u ./...".into()),
                run_subcommand: Some("run".into()),
                ..base()
            },
            PackageManager::Pub => PackageManagerSpec {
                add_subcommand: Some("pub add".into()),
                install_subcommand: Some("pub get".into()),
                remove_subcommand: Some("pub remove".into()),
                update_subcommand: Some("pub upgrade".into()),
                update_all_subcommand: Some("pub upgrade".into()),
                run_subcommand: Some("run".into()),
                publish_subcommand: Some("pub publish".into()),
                save_dev_flag: Some("--dev".into()),
                ..base()
            },
            PackageManager::Bundler => PackageManagerSpec {
                init_subcommand: Some("init".into()),
                add_subcommand: Some("add".into()),
                install_subcommand: Some("install".into()),
                remove_subcommand: Some("remove".into()),
                update_subcommand: Some("update".into()),
                update_all_subcommand: Some("update".into()),
                run_subcommand: Some("exec".into()),
                save_dev_flag: Some("--group development".into()),
                ..base()
            },
            PackageManager::Composer => PackageManagerSpec {
                init_subcommand: Some("init".into()),
                add_subcommand: Some("require".into()),
                install_subcommand: Some("install".into()),
                remove_subcommand: Some("remove".into()),
                update_subcommand: Some("update".into()),
                update_all_subcommand: Some("update".into()),
                run_subcommand: Some("run-script".into()),
                save_dev_flag: Some("--dev".into()),
                run_separator: Some("--".into()),
                ..base()
            },
        }
    }

    pub fn supported_managers(&self) -> Vec<PackageManager> {
        PackageManager::all().to_vec()
    }

    pub fn default_for_language(&self, language: &str) -> Option<PackageManager> {
        PackageManager::default_for_language(language)
    }

    pub fn init_command(&self, manager: PackageManager) -> Result<PackageCommand> {
        let spec = self.spec(manager);
        let subcommand = spec
            .init_subcommand
            .as_deref()
            .ok_or_else(|| unsupported(manager, "init"))?;
        let mut args = Vec::new();
        push_subcommand(&mut args, subcommand);
        if let Some(flag) = spec.init_flag.as_ref().filter(|flag| !flag.is_empty()) {
            args.push(flag.clone());
        }
        Ok(PackageCommand {
            program: spec.binary,
            args,
        })
    }

    pub fn install_all_command(&self, manager: PackageManager) -> Result<PackageCommand> {
        let spec = self.spec(manager);
        let subcommand = spec
            .install_subcommand
            .as_deref()
            .ok_or_else(|| unsupported(manager, "installAll"))?;
        let mut args = Vec::new();
        push_subcommand(&mut args, subcommand);
        if let Some(flag) = spec.strict_peer_flag {
            args.push(flag);
        }
        Ok(PackageCommand {
            program: spec.binary,
            args,
        })
    }

    pub fn install_command(
        &self,
        manager: PackageManager,
        packages: &[String],
        dev: bool,
    ) -> Result<PackageCommand> {
        let spec = self.spec(manager);
        let subcommand = spec
            .add_subcommand
            .as_deref()
            .ok_or_else(|| unsupported(manager, "add"))?;
        let mut args = Vec::new();
        push_subcommand(&mut args, subcommand);
        if let Some(flag) = spec.strict_peer_flag {
            args.push(flag);
        }
        if dev {
            let flag = spec
                .save_dev_flag
                .as_ref()
                .ok_or_else(|| unsupported(manager, "add-dev"))?;
            args.push(flag.clone());
        } else if manager == PackageManager::Npm {
            if let Some(flag) = spec.save_flag {
                args.push(flag);
            }
        }
        args.extend(packages.iter().cloned());
        Ok(PackageCommand {
            program: spec.binary,
            args,
        })
    }

    pub fn remove_command(
        &self,
        manager: PackageManager,
        packages: &[String],
    ) -> Result<PackageCommand> {
        let spec = self.spec(manager);
        let subcommand = spec
            .remove_subcommand
            .as_deref()
            .ok_or_else(|| unsupported(manager, "remove"))?;
        let mut args = Vec::new();
        push_subcommand(&mut args, subcommand);
        args.extend(packages.iter().cloned());
        Ok(PackageCommand {
            program: spec.binary,
            args,
        })
    }

    pub fn update_command(
        &self,
        manager: PackageManager,
        packages: &[String],
    ) -> Result<PackageCommand> {
        let spec = self.spec(manager);
        let mut args = Vec::new();
        if packages.is_empty() {
            let subcommand = spec
                .update_all_subcommand
                .as_deref()
                .or(spec.update_subcommand.as_deref())
                .ok_or_else(|| unsupported(manager, "update"))?;
            push_subcommand(&mut args, subcommand);
        } else {
            let subcommand = spec
                .update_subcommand
                .as_deref()
                .ok_or_else(|| unsupported(manager, "update"))?;
            push_subcommand(&mut args, subcommand);
            if let Some(flag) = spec.update_package_flag {
                for package in packages {
                    args.push(flag.clone());
                    args.push(package.clone());
                }
            } else {
                args.extend(packages.iter().cloned());
            }
        }
        Ok(PackageCommand {
            program: spec.binary,
            args,
        })
    }

    pub fn run_script_command(
        &self,
        manager: PackageManager,
        script: &str,
        extra_args: &[String],
    ) -> Result<PackageCommand> {
        let spec = self.spec(manager);
        let subcommand = spec
            .run_subcommand
            .as_deref()
            .ok_or_else(|| unsupported(manager, "run"))?;
        let mut args = Vec::new();
        push_subcommand(&mut args, subcommand);
        args.push(script.to_string());
        if let Some(separator) = spec.run_separator {
            if !extra_args.is_empty() {
                args.push(separator);
            }
        }
        args.extend(extra_args.iter().cloned());
        Ok(PackageCommand {
            program: spec.binary,
            args,
        })
    }

    pub fn publish_command(
        &self,
        manager: PackageManager,
        tag: Option<&str>,
    ) -> Result<PackageCommand> {
        let spec = self.spec(manager);
        let subcommand = spec
            .publish_subcommand
            .as_deref()
            .ok_or_else(|| unsupported(manager, "publish"))?;
        if tag.is_some() && manager.language() != "node" {
            return Err(anyhow!(
                "publish tag is only supported for node package managers"
            ));
        }
        let mut args = Vec::new();
        push_subcommand(&mut args, subcommand);
        if let Some(tag) = tag {
            args.push("--tag".into());
            args.push(tag.to_string());
        }
        Ok(PackageCommand {
            program: spec.binary,
            args,
        })
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
        self.run_script_command(manager, script, extra_args)
    }

    pub fn add_dependency_commands(
        &self,
        manager: PackageManager,
        dependencies: &[String],
        dev_dependencies: &[String],
    ) -> Result<Vec<PackageCommand>> {
        let mut commands = Vec::new();
        if !dependencies.is_empty() {
            commands.push(self.install_command(manager, dependencies, false)?);
        }
        if !dev_dependencies.is_empty() {
            commands.push(self.install_command(manager, dev_dependencies, true)?);
        }
        Ok(commands)
    }

    pub fn lockfile_strategy(&self, manager: PackageManager) -> String {
        format!("{} uses {}", manager.id(), manager.lockfile())
    }
}

fn python_manager_from_pyproject(cwd: &Path) -> PackageManager {
    let content = fs::read_to_string(cwd.join("pyproject.toml")).unwrap_or_default();
    if content.contains("[tool.poetry]") {
        PackageManager::Poetry
    } else if content.contains("[tool.uv]") {
        PackageManager::Uv
    } else {
        PackageManager::Pip
    }
}

fn push_subcommand(args: &mut Vec<String>, subcommand: &str) {
    args.extend(subcommand.split_whitespace().map(str::to_string));
}

fn unsupported(manager: PackageManager, action: &str) -> anyhow::Error {
    anyhow!("{} does not support {}", manager.id(), action)
}

#[cfg(test)]
mod tests;
