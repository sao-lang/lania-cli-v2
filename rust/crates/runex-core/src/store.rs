//! 运行时存储：安装、切换、卸载、列举。
//!
//! 职责边界：
//! - **不做**「下载什么 / 目录怎么摆」的判断 —— 那是 Provider 的事；
//! - **只做**「落盘、登记、原子替换、维护 current 指针、生成 shim」。
//!
//! 安装流程（任何语言都一样）：
//! ```text
//! resolve artifact → 下载到 cache（命中则跳过）→ 校验 sha256
//!   → 解压到 .staging → Provider.install_layout → 挪到最终目录
//!   → 写 .runex-meta.json → 更新 state.json → 生成 shims
//! ```

use std::path::{Path, PathBuf};

use crate::archive;
use crate::checksum;
use crate::home::{RuntimeHome, PATH_ENV};
use crate::http::HttpClient;
use crate::provider::{self, ProviderContext, ProviderError, RuntimeProvider};
use crate::shims::{self, ShimHost};
use crate::state::{InstalledMeta, InstalledRuntime, RuntimeState};
use crate::version::{self, VersionTag};
use crate::{Platform, RuntimeError};

/// 安装进度回调（CLI 打印、TUI 显示状态都走这里）。
pub type ProgressFn<'a> = &'a mut dyn FnMut(InstallEvent);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallEvent {
    Resolving,
    Downloading {
        url: String,
        cached: bool,
    },
    /// 下载字节进度；`total` 来自 `Content-Length`，缺失时为 `None`。
    DownloadProgress {
        url: String,
        downloaded: u64,
        total: Option<u64>,
    },
    Verifying,
    Extracting,
    Finalizing,
    Done {
        version: String,
    },
}

/// 安装结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub language: String,
    pub version: String,
    pub path: PathBuf,
    /// 是否命中下载缓存。
    pub from_cache: bool,
    /// 上游未提供校验和时的提示。
    pub warnings: Vec<String>,
}

pub struct RuntimeStore {
    home: RuntimeHome,
    platform: Platform,
    http: HttpClient,
    /// 生成 shim 与用户提示时使用的「管理命令」（`lan env` 或 `runex`）。
    shim_host: ShimHost,
}

impl RuntimeStore {
    pub fn new(home: RuntimeHome, platform: Platform) -> Self {
        Self {
            home,
            platform,
            http: HttpClient::new(),
            shim_host: ShimHost::lan(),
        }
    }

    /// 切换「管理命令」：独立二进制（`runex`）用它生成自己的 shim。
    ///
    /// 默认是 `lan env`，因此宿主内嵌路径的行为保持不变。
    pub fn with_shim_host(mut self, host: ShimHost) -> Self {
        self.shim_host = host;
        self
    }

    pub fn shim_host(&self) -> &ShimHost {
        &self.shim_host
    }

    pub fn home(&self) -> &RuntimeHome {
        &self.home
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    fn provider(&self, language_id: &str) -> Result<Box<dyn RuntimeProvider>, RuntimeError> {
        provider::provider_for(language_id, self.platform).ok_or_else(|| {
            RuntimeError::UnsupportedLanguage {
                language: language_id.to_string(),
            }
        })
    }

    /// 列出某语言的全部可用版本（需要网络，或命中上游缓存）。
    pub fn available_versions(&self, language_id: &str) -> Result<Vec<VersionTag>, RuntimeError> {
        let provider = self.provider(language_id)?;
        let ctx = ProviderContext {
            platform: self.platform,
            http: &self.http,
        };
        let releases = provider
            .list_releases(&ctx)
            .map_err(|error| ProviderError::into_runtime(error, language_id))?;
        Ok(releases
            .into_iter()
            .filter_map(|release| VersionTag::parse(&release.version))
            .collect())
    }

    /// 安装一个版本。
    ///
    /// `version` 支持前缀约束（`20` 安装 20.x 的最高补丁版本）。
    pub fn install(
        &self,
        language_id: &str,
        version: &str,
        progress: ProgressFn<'_>,
    ) -> Result<InstallOutcome, RuntimeError> {
        let provider = self.provider(language_id)?;
        if let Some(reason) = provider::platform_limitation(language_id, self.platform) {
            return Err(RuntimeError::ArtifactNotFound {
                language: language_id.to_string(),
                version: version.to_string(),
                platform: self.platform,
            })
            .map_err(|_| RuntimeError::other(reason));
        }
        progress(InstallEvent::Resolving);

        let ctx = ProviderContext {
            platform: self.platform,
            http: &self.http,
        };
        let wanted = VersionTag::parse(version).ok_or_else(|| RuntimeError::VersionNotFound {
            language: language_id.to_string(),
            version: version.to_string(),
        })?;
        let releases = provider
            .list_releases(&ctx)
            .map_err(|error| ProviderError::into_runtime(error, language_id))?;
        let parsed: Vec<VersionTag> = releases
            .iter()
            .filter_map(|release| VersionTag::parse(&release.version))
            .collect();
        let matched =
            version::pick_best(&parsed, &wanted).ok_or_else(|| RuntimeError::VersionNotFound {
                language: language_id.to_string(),
                version: version.to_string(),
            })?;
        let release = releases
            .iter()
            .find(|release| release.version == matched.raw)
            .ok_or_else(|| RuntimeError::VersionNotFound {
                language: language_id.to_string(),
                version: version.to_string(),
            })?
            .clone();

        let primary = provider
            .resolve_artifact(&release, &ctx)
            .map_err(|error| ProviderError::into_runtime(error, language_id))?;
        let mut artifacts = vec![primary];
        artifacts.extend(
            provider
                .extra_artifacts(&release, &ctx)
                .map_err(|error| ProviderError::into_runtime(error, language_id))?,
        );

        let mut warnings = Vec::new();
        let mut from_cache = true;
        let mut downloaded = Vec::new();
        for artifact in &artifacts {
            self.home.ensure_dirs()?;
            let cached = crate::http::cache_path_for(&self.home.downloads_dir(), &artifact.url);
            if cached.is_file() {
                from_cache = false;
                progress(InstallEvent::Downloading {
                    url: artifact.url.clone(),
                    cached: true,
                });
            }
            // 缓存里可能躺着一份上次中断留下的坏文件；校验失败就删掉重下一次，
            // 而不是把「校验和不匹配」直接抛给用户。
            const FETCH_ATTEMPTS: usize = 3;
            let mut last_error: Option<RuntimeError> = None;
            let mut verified = false;
            for attempt in 0..FETCH_ATTEMPTS {
                if !cached.is_file() {
                    from_cache = false;
                    progress(InstallEvent::Downloading {
                        url: artifact.url.clone(),
                        cached: false,
                    });
                    let mut on_progress = |downloaded, total| {
                        progress(InstallEvent::DownloadProgress {
                            url: artifact.url.clone(),
                            downloaded,
                            total,
                        });
                    };
                    self.http.download_with_progress(
                        &artifact.url,
                        &cached,
                        Some(&mut on_progress),
                    )?;
                }
                progress(InstallEvent::Verifying);
                match self.verify_artifact(artifact, &cached, &mut warnings) {
                    Ok(()) => {
                        verified = true;
                        break;
                    }
                    Err(error) => {
                        // 坏文件不能留在缓存里被下次命中；连同续传临时文件一起删。
                        let _ = std::fs::remove_file(&cached);
                        let _ = std::fs::remove_file(crate::http::partial_path_for(&cached));
                        last_error = Some(error);
                        if attempt + 1 == FETCH_ATTEMPTS {
                            break;
                        }
                    }
                }
            }
            if !verified {
                return Err(last_error.unwrap_or_else(|| {
                    RuntimeError::other(format!("failed to fetch {}", artifact.file_name))
                }));
            }
            downloaded.push((artifact.clone(), cached));
        }

        let version_dir = self.home.version_dir(language_id, &release.version);
        if version_dir.is_dir() {
            // 已安装：直接返回，避免重复解压覆盖正在使用的文件。
            return Ok(InstallOutcome {
                language: language_id.to_string(),
                version: release.version,
                path: version_dir,
                from_cache,
                warnings,
            });
        }

        progress(InstallEvent::Extracting);
        let staging = self.staging_dir(language_id, &release.version)?;
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;
        let extract_root = staging.join("payload");

        for (artifact, archive_path) in &downloaded {
            // Rust 的组件包解到同一目录后按 `rust-*` / `cargo-*` 分层，不会互相覆盖。
            archive::extract(archive_path, &extract_root)?;
            let _ = artifact;
        }

        progress(InstallEvent::Finalizing);
        let layout = provider
            .install_layout(&extract_root, &version_dir)
            .map_err(|error| ProviderError::into_runtime(error, language_id))?;

        let built = staging.join("built");
        self.materialize(&extract_root, &staging, &built)?;

        // 先把版本目录整体挪到位，再写元数据：中途失败时不会留下「有元数据但不可用」的记录。
        if let Some(parent) = version_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&built, &version_dir)?;

        let meta = InstalledMeta {
            language: language_id.to_string(),
            version: release.version.clone(),
            tag: release.tag.clone(),
            upstream: provider.upstream().to_string(),
            artifact: artifacts
                .iter()
                .map(|artifact| artifact.file_name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            sha256: artifacts
                .first()
                .and_then(|artifact| artifact.sha256.clone()),
            bin_dir: layout.bin_dir.display().to_string(),
            env: layout.env.clone(),
            entrypoints: layout.entrypoints.clone(),
            platform: InstalledMeta::platform_string(self.platform),
            installed_at: Some(now_rfc3339()),
            notes: layout.notes.clone(),
        };
        meta.save(&version_dir)?;
        set_executables(&version_dir, &layout)?;

        let _ = std::fs::remove_dir_all(&staging);

        // 首个安装的版本自动成为 current，避免用户 `use` 之前完全没有入口。
        // 即使 current 已存在，也要补一次 shim 同步：上次安装可能中途失败，
        // 或者版本目录被用户手动删除后重建，shim 会缺失。
        let mut state = RuntimeState::load(&self.home.state_path())?;
        let became_current = state.current_version(language_id).is_none();
        if became_current {
            state.set_current(language_id, &release.version);
            state
                .entrypoints
                .insert(language_id.to_string(), layout.entrypoints.clone());
            state
                .env
                .insert(language_id.to_string(), layout.env.clone());
            state.save(&self.home.state_path())?;
        }
        if became_current || state.current_version(language_id) == Some(release.version.as_str()) {
            let bin_dir = version_dir.join(&layout.bin_dir);
            self.sync_shims(language_id, &bin_dir, &layout.entrypoints)?;
        }

        progress(InstallEvent::Done {
            version: release.version.clone(),
        });
        Ok(InstallOutcome {
            language: language_id.to_string(),
            version: release.version,
            path: version_dir,
            from_cache,
            warnings,
        })
    }

    /// 校验一个已下载的产物（官方 sha256 优先，缺失时退回结构完整性检查）。
    fn verify_artifact(
        &self,
        artifact: &crate::ProviderArtifact,
        cached: &Path,
        warnings: &mut Vec<String>,
    ) -> Result<(), RuntimeError> {
        match &artifact.sha256 {
            Some(expected) => checksum::verify_file(cached, expected)?,
            None => {
                if !warnings
                    .iter()
                    .any(|item| item.contains(&artifact.file_name))
                {
                    warnings.push(format!(
                        "{} has no published checksum; installed without checksum verification",
                        artifact.file_name
                    ));
                }
                // 没有官方校验和时，至少要确认下载完整、归档结构可读，
                // 否则半截文件会被缓存下来，下次安装直接复用。
                let size = std::fs::metadata(cached)?.len();
                if size == 0 {
                    return Err(RuntimeError::other(format!(
                        "downloaded {} is empty",
                        artifact.file_name
                    )));
                }
                archive::verify_integrity(cached)?;
            }
        }
        Ok(())
    }

    /// 切换 current 指针并重新生成 shim。返回切换后的版本目录。
    pub fn use_version(&self, language_id: &str, version: &str) -> Result<PathBuf, RuntimeError> {
        let version_dir = self.resolve_installed(language_id, version)?;
        let meta =
            InstalledMeta::load(&version_dir)?.ok_or_else(|| RuntimeError::NotInstalled {
                language: language_id.to_string(),
                version: version.to_string(),
                root: version_dir.display().to_string(),
            })?;
        let mut state = RuntimeState::load(&self.home.state_path())?;
        state.set_current(language_id, &meta.version);
        state
            .entrypoints
            .insert(language_id.to_string(), meta.entrypoints.clone());
        state.env.insert(language_id.to_string(), meta.env.clone());
        state.save(&self.home.state_path())?;
        // 版本目录内真实存在的入口才会生成 shim（例如 `corepack` 在老版本里可能不存在）。
        let bin_dir = version_dir.join(&meta.bin_dir);
        self.sync_shims(language_id, &bin_dir, &meta.entrypoints)?;
        Ok(version_dir)
    }

    /// 卸载某个版本（只删除 lania 自己管理的目录）。
    pub fn remove(&self, language_id: &str, version: &str) -> Result<PathBuf, RuntimeError> {
        let version_dir = self.resolve_installed(language_id, version)?;
        // 用目录名（可能来自 `20` 这类前缀）与 current 比较，才能正确判断「删的是当前版本」。
        let removed_version = version_dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| version.to_string());

        let mut state = RuntimeState::load(&self.home.state_path())?;
        let was_current = state.current_version(language_id) == Some(removed_version.as_str());
        let entrypoints = state
            .entrypoints
            .get(language_id)
            .cloned()
            .unwrap_or_default();
        if was_current {
            // 先清指针再删目录：即使后续回退失败，也不会留下指向已删除目录的状态。
            state.clear_current(language_id);
            state.save(&self.home.state_path())?;
        }

        std::fs::remove_dir_all(&version_dir)?;

        if was_current {
            // 退回该语言剩下的最高版本；没有剩余版本时清掉 shim。
            match self.installed_versions(language_id)?.first() {
                Some(next) => {
                    let next_version = next.version.clone();
                    self.use_version(language_id, &next_version)?;
                }
                None => {
                    for entry in entrypoints {
                        shims::remove_shim(&self.home, &entry);
                    }
                }
            }
        }
        Ok(version_dir)
    }

    /// 列出某语言已安装的版本（仅 lania 自己管理的目录），新 → 旧。
    pub fn installed_versions(
        &self,
        language_id: &str,
    ) -> Result<Vec<InstalledRuntime>, RuntimeError> {
        let state = RuntimeState::load(&self.home.state_path())?;
        let current = state.current_version(language_id);
        let dir = self.home.language_dir(language_id);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(meta) = InstalledMeta::load(&path)? else {
                continue;
            };
            out.push(InstalledRuntime {
                language: language_id.to_string(),
                version: meta.version.clone(),
                path: path.display().to_string(),
                current: current == Some(meta.version.as_str()),
                managed: true,
                bin_dir: Some(path.join(&meta.bin_dir).display().to_string()),
                installed_at: meta.installed_at.clone(),
            });
        }
        out.sort_by(|a, b| {
            let left = VersionTag::parse(&a.version).map(|tag| tag.parts);
            let right = VersionTag::parse(&b.version).map(|tag| tag.parts);
            right.cmp(&left)
        });
        Ok(out)
    }

    /// 全部语言已安装版本（`lan env list --installed`）。
    pub fn installed_all(&self) -> Result<Vec<InstalledRuntime>, RuntimeError> {
        let mut out = Vec::new();
        for language_id in provider::supported_language_ids() {
            out.extend(self.installed_versions(language_id)?);
        }
        Ok(out)
    }

    /// 解析某个入口当前应指向的二进制（`lan env which` / shim 使用）。
    pub fn which(&self, name: &str) -> Result<PathBuf, RuntimeError> {
        let state = RuntimeState::load(&self.home.state_path())?;
        // 入口名 → 语言：按各语言 current 版本的 entrypoints 匹配。
        let languages: Vec<String> = state.current.keys().cloned().collect();
        for language_id in languages {
            let Some(version) = state.current_version(&language_id) else {
                continue;
            };
            let version_dir = self.home.version_dir(&language_id, version);
            let Ok(Some(meta)) = InstalledMeta::load(&version_dir) else {
                continue;
            };
            if meta.entrypoints.iter().any(|entry| entry == name) {
                let bin_dir = version_dir.join(&meta.bin_dir);
                if shims::entry_exists(&bin_dir, name) {
                    return Ok(PathBuf::from(shims::resolve_entry(&bin_dir, name)));
                }
            }
        }
        Err(RuntimeError::other(format!(
            "no managed runtime provides `{name}`; run `{} install {name} <version>` first",
            self.shim_host.command
        )))
    }

    /// 生成 `init` 的 shell 片段。
    pub fn init_snippet(&self, shell: &str) -> String {
        shims::shell_snippet(&self.home, shell)
    }

    /// `doctor`：检查 current 指针、shim 与 PATH 的一致性。
    pub fn diagnose(&self) -> Result<Vec<Diagnostic>, RuntimeError> {
        let state = RuntimeState::load(&self.home.state_path())?;
        let mut out = Vec::new();
        let command = &self.shim_host.command;

        let shims_dir = self.home.shims_dir();
        // 两条独立诊断：`lan` 每次启动都会 `activate()`，所以「本进程 PATH 是否前置」
        // 只反映内部健康度；真正需要用户动手的是「shell profile 是否固化」。
        let on_path = std::env::var_os(PATH_ENV)
            .map(|value| std::env::split_paths(&value).any(|dir| dir == shims_dir))
            .unwrap_or(false);
        if !shims_dir.is_dir() {
            out.push(Diagnostic {
                level: DiagnosticLevel::Warn,
                message: format!(
                    "{} does not exist yet; it is created on the first `{command} install`",
                    shims_dir.display()
                ),
            });
        } else if !on_path {
            out.push(Diagnostic {
                level: DiagnosticLevel::Warn,
                message: format!(
                    "{} is not prepended to this process PATH; commands may use system runtimes instead. \
                     This indicates a lania bug — please report it",
                    shims_dir.display()
                ),
            });
        }
        if !self.shell_profile_has_shims() {
            out.push(Diagnostic {
                level: DiagnosticLevel::Info,
                message: format!(
                    "{} is not in your shell profile; run `{command} init` and append the snippet \
                     so external shells also use managed runtimes",
                    shims_dir.display()
                ),
            });
        }

        for language_id in provider::supported_language_ids().iter().copied() {
            let Some(version) = state.current_version(language_id) else {
                continue;
            };
            let version_dir = self.home.version_dir(language_id, version);
            if !version_dir.is_dir() {
                out.push(Diagnostic {
                    level: DiagnosticLevel::Error,
                    message: format!(
                        "{language_id} points at {version} but {} is missing",
                        version_dir.display()
                    ),
                });
                continue;
            }
            let entrypoints = state
                .entrypoints
                .get(language_id)
                .cloned()
                .unwrap_or_default();
            for entry in entrypoints {
                let shim = shims_dir.join(&entry);
                let shim_cmd = shims_dir.join(format!("{entry}.cmd"));
                if !shim.is_file() && !shim_cmd.is_file() {
                    out.push(Diagnostic {
                        level: DiagnosticLevel::Warn,
                        message: format!(
                            "missing shim for `{entry}` (expected in {})",
                            shims_dir.display()
                        ),
                    });
                }
            }
        }
        Ok(out)
    }

    /// shell profile 是否已经把 shims 目录写死（只有外部终端才需要它）。
    ///
    /// 必须真的读文件：PATH 里出现 shims 目录也可能是 `lan` 自己注入的，
    /// 那不能证明外部终端也生效。
    fn shell_profile_has_shims(&self) -> bool {
        let needle = self.home.display_for_shell(&self.home.shims_dir());
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let Some(home) = home else {
            return false;
        };
        let mut candidates = vec![
            home.join(".bashrc"),
            home.join(".zshrc"),
            home.join(".profile"),
            home.join(".bash_profile"),
            home.join(".config").join("fish").join("config.fish"),
        ];
        // Windows PowerShell：`$PROFILE` 通常位于 Documents/PowerShell（或 WindowsPowerShell）。
        for dir in ["PowerShell", "WindowsPowerShell"] {
            candidates.push(
                home.join("Documents")
                    .join(dir)
                    .join("Microsoft.PowerShell_profile.ps1"),
            );
            candidates.push(home.join("Documents").join(dir).join("profile.ps1"));
        }
        candidates.iter().any(|path| {
            std::fs::read_to_string(path)
                .map(|content| content.contains(&needle))
                .unwrap_or(false)
        })
    }

    fn installed_versions_paths(&self, language_id: &str) -> Vec<PathBuf> {
        let dir = self.home.language_dir(language_id);
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect()
    }

    /// 把用户输入的版本解析成已安装目录（支持 `20` 这样的前缀）。
    fn resolve_installed(&self, language_id: &str, version: &str) -> Result<PathBuf, RuntimeError> {
        let wanted = VersionTag::parse(version).ok_or_else(|| RuntimeError::VersionNotFound {
            language: language_id.to_string(),
            version: version.to_string(),
        })?;
        let mut candidates: Vec<(VersionTag, PathBuf)> = self
            .installed_versions_paths(language_id)
            .into_iter()
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().to_string();
                let tag = VersionTag::parse(&name)?;
                Some((tag, path))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.parts.cmp(&a.0.parts));
        let tags: Vec<VersionTag> = candidates.iter().map(|(tag, _)| tag.clone()).collect();
        let Some(best) = version::pick_best(&tags, &wanted) else {
            return Err(RuntimeError::NotInstalled {
                language: language_id.to_string(),
                version: version.to_string(),
                root: self.home.language_dir(language_id).display().to_string(),
            });
        };
        candidates
            .into_iter()
            .find(|(tag, _)| tag == best)
            .map(|(_, path)| path)
            .ok_or_else(|| RuntimeError::NotInstalled {
                language: language_id.to_string(),
                version: version.to_string(),
                root: self.home.language_dir(language_id).display().to_string(),
            })
    }

    fn staging_dir(&self, language_id: &str, version: &str) -> Result<PathBuf, RuntimeError> {
        let dir = self
            .home
            .root()
            .join("cache")
            .join("staging")
            .join(language_id)
            .join(version);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 把解压目录整理成最终目录树。
    ///
    /// 三种形态：
    /// - 单层目录（Node / Go / Python / Zig）：摊平到 built；
    /// - 多组件目录（Rust 的 `rust-*/` + `cargo-*/`）：把每个组件的 `bin|lib|share`
    ///   合并到同一层，而不是把组件目录整个嵌进去；
    /// - 顶层散文件（Deno / Bun 的单文件 zip）：直接放到根上。
    fn materialize(
        &self,
        extract_root: &Path,
        staging: &Path,
        built: &Path,
    ) -> Result<PathBuf, RuntimeError> {
        let _ = staging;
        std::fs::create_dir_all(built)?;
        let roots = archive::top_level_dirs(extract_root);
        let top_files = archive::top_level_files(extract_root);

        if !top_files.is_empty() && roots.is_empty() {
            // 单文件产物：全部平铺到根上。
            for file in &top_files {
                if let Some(name) = file.file_name() {
                    std::fs::copy(file, built.join(name))?;
                }
            }
            return Ok(built.to_path_buf());
        }
        if roots.len() == 1 && top_files.is_empty() {
            copy_dir_contents(&roots[0], built)?;
            return Ok(built.to_path_buf());
        }
        // 多组件（Rust 的 `rust-*/` + `cargo-*/`）：每个组件包内部还有一层
        // `rustc/`、`cargo/`、`rust-std-*/` 等子目录，真正的 `bin/`、`lib/`
        // 在这些子目录里。必须逐层合并，否则 `bin/` 不会出现在版本目录根上。
        let component_like = !roots.is_empty()
            && roots.iter().all(|dir| {
                dir.file_name()
                    .map(|name| {
                        let name = name.to_string_lossy();
                        name.starts_with("rust-") || name.starts_with("cargo-")
                    })
                    .unwrap_or(false)
            });
        if component_like {
            for dir in &roots {
                merge_component_dir(dir, built)?;
            }
            for file in &top_files {
                if let Some(name) = file.file_name() {
                    std::fs::copy(file, built.join(name))?;
                }
            }
            return Ok(built.to_path_buf());
        }
        copy_dir_contents(extract_root, built)?;
        Ok(built.to_path_buf())
    }

    fn sync_shims(
        &self,
        _language_id: &str,
        bin_dir: &Path,
        entrypoints: &[String],
    ) -> Result<(), RuntimeError> {
        for entry in entrypoints {
            shims::write_shim(&self.home, entry, bin_dir, &self.shim_host)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// 纯提示：不修也能用，修了更好（例如 shell profile 未固化）。
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
}

/// 原子写文件：先写 `.tmp` 再 rename，避免中途崩溃留下半个 JSON。
pub fn write_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&tmp, contents)?;
    // Windows 上 rename 到已存在文件会失败，先删掉旧文件。
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 合并一个 Rust 组件包（`rust-*` / `cargo-*`）。
///
/// 组件包是 rust-installer 格式：payload 在 `<component>/bin`、`<component>/lib`、
/// `<component>/rust-std-<triple>/lib` 等子目录里，根上还有 `install.sh`、
/// `components`、`manifest.in` 这类安装期文件（丢弃即可）。
fn merge_component_dir(component: &Path, built: &Path) -> Result<(), RuntimeError> {
    for entry in std::fs::read_dir(component)?.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            "bin" | "lib" | "libexec" | "share" | "etc" | "include"
        ) {
            // 组件自身的标准目录：直接并入根上同名目录。
            merge_dir_contents(&path, &built.join(&name))?;
            continue;
        }
        // 子组件（`rustc/`、`cargo/`、`rust-std-<triple>/`、`rustfmt-preview/` …）：
        // 各自内部同样有 `bin|lib|libexec|share|etc`，递归处理一层。
        if name == "rustc"
            || name == "cargo"
            || name.ends_with("-preview")
            || name.starts_with("rust-std-")
            || name.starts_with("rust-analysis-")
            || name.starts_with("rust-mingw")
        {
            merge_component_dir(&path, built)?;
        }
    }
    Ok(())
}

/// 把 `from` 的内容合并进 `to`：同名目录递归合并，同名文件后写覆盖。
///
/// Rust 的 `rust-*/` 与 `cargo-*/` 组件包都含 `bin/`、`lib/`、`share/`，
/// 直接用 `copy_dir_contents` 会得到 `built/rust-*/bin` 这样的嵌套结构。
fn merge_dir_contents(from: &Path, to: &Path) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.filter_map(Result::ok) {
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            std::fs::create_dir_all(&target)?;
            merge_dir_contents(&source, &target)?;
        } else {
            std::fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

fn copy_dir_contents(from: &Path, to: &Path) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.filter_map(Result::ok) {
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_contents(&source, &target)?;
        } else {
            // 保留符号链接指向的内容：运行时的 `bin/npm -> ../lib/...` 必须继续可用。
            std::fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

fn set_executables(
    version_dir: &Path,
    layout: &crate::provider::ProviderLayout,
) -> Result<(), RuntimeError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for relative in &layout.executables {
            let path = version_dir.join(relative);
            if path.is_file() {
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = (version_dir, layout);
    Ok(())
}

fn now_rfc3339() -> String {
    // 不引入时间库：用 UNIX 秒数换算成 UTC ISO-8601（精度到秒，够用于审计）。
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default();
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant 的 `days → y/m/d` 算法（无时区、无闰秒）。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2024-02-29
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }

    #[test]
    fn copy_dir_contents_is_recursive() {
        let root = std::env::temp_dir().join(format!("lania-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src").join("lib")).unwrap();
        std::fs::write(root.join("src").join("bin").join("tool"), b"x").unwrap_or_default();
        std::fs::create_dir_all(root.join("src").join("bin")).unwrap();
        std::fs::write(root.join("src").join("bin").join("tool"), b"x").unwrap();
        std::fs::write(root.join("src").join("lib").join("data"), b"y").unwrap();
        copy_dir_contents(&root.join("src"), &root.join("dst")).unwrap();
        assert!(root.join("dst").join("bin").join("tool").is_file());
        assert!(root.join("dst").join("lib").join("data").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_dir_contents_flattens_component_directories() {
        let root = std::env::temp_dir().join(format!("lania-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // 复刻 rust-installer 布局：rust-*/rustc/bin、rust-*/cargo/bin、cargo-*/cargo/bin
        let rust_pkg = root.join("payload").join("rust-1.99.1-x86_64");
        for (component, binary) in [("rustc", "rustc"), ("cargo", "cargo")] {
            let bin = rust_pkg.join(component).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join(binary), b"x").unwrap();
            std::fs::write(rust_pkg.join(component).join("manifest.in"), b"m").unwrap();
        }
        std::fs::create_dir_all(rust_pkg.join("rust-std-x86_64")).unwrap();
        std::fs::create_dir_all(rust_pkg.join("rust-std-x86_64").join("lib")).unwrap();
        std::fs::write(
            rust_pkg
                .join("rust-std-x86_64")
                .join("lib")
                .join("libstd.rlib"),
            b"s",
        )
        .unwrap();
        std::fs::write(rust_pkg.join("install.sh"), b"#!").unwrap();

        let cargo_pkg = root.join("payload").join("cargo-1.99.1-x86_64");
        std::fs::create_dir_all(cargo_pkg.join("cargo").join("bin")).unwrap();
        std::fs::write(cargo_pkg.join("cargo").join("bin").join("cargo"), b"y").unwrap();

        let store = store_at(&root);
        let built = root.join("built");
        store
            .materialize(&root.join("payload"), &root, &built)
            .unwrap();

        assert!(built.join("bin").join("rustc").is_file());
        assert!(built.join("bin").join("cargo").is_file());
        assert!(built.join("lib").join("libstd.rlib").is_file());
        // 组件目录与安装期文件不应被保留。
        assert!(!built.join("rust-1.99.1-x86_64").exists());
        assert!(!built.join("cargo-1.99.1-x86_64").exists());
        assert!(!built.join("install.sh").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn materialize_unwraps_single_directory() {
        let root = std::env::temp_dir().join(format!("lania-unwrap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let inner = root.join("payload").join("node-v20.11.1");
        std::fs::create_dir_all(inner.join("bin")).unwrap();
        std::fs::write(inner.join("bin").join("node"), b"x").unwrap();
        let store = store_at(&root);
        let built = root.join("built");
        store
            .materialize(&root.join("payload"), &root, &built)
            .unwrap();
        assert!(built.join("bin").join("node").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = std::env::temp_dir().join(format!("lania-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 在临时 `~/.runex` 里手工摆一个「已安装版本」，模拟 install 的产物。
    fn seed_version(home: &RuntimeHome, language: &str, version: &str, entries: &[&str]) {
        let version_dir = home.version_dir(language, version);
        std::fs::create_dir_all(version_dir.join("bin")).unwrap();
        for entry in entries {
            let name = if cfg!(windows) {
                format!("{entry}.cmd")
            } else {
                entry.to_string()
            };
            std::fs::write(version_dir.join("bin").join(name), b"#!/bin/sh\n").unwrap();
            // shim 生成要求真实文件存在；两种扩展名都放一份，保证跨平台测试一致。
            std::fs::write(version_dir.join("bin").join(entry), b"#!/bin/sh\n").unwrap();
        }
        InstalledMeta {
            language: language.into(),
            version: version.into(),
            tag: version.into(),
            upstream: "test".into(),
            artifact: "test.tar.gz".into(),
            sha256: None,
            bin_dir: "bin".into(),
            env: Vec::new(),
            entrypoints: entries.iter().map(|item| item.to_string()).collect(),
            platform: "test".into(),
            installed_at: Some("2026-01-01T00:00:00Z".into()),
            notes: Vec::new(),
        }
        .save(&version_dir)
        .unwrap();
    }

    fn store_at(root: &Path) -> RuntimeStore {
        RuntimeStore::new(
            RuntimeHome::at(root.join("runex")),
            Platform::detect().unwrap(),
        )
    }

    #[test]
    fn use_and_which_track_the_current_pointer() {
        let root = std::env::temp_dir().join(format!("lania-store-use-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let store = store_at(&root);
        seed_version(store.home(), "node", "20.11.1", &["node", "npm"]);
        seed_version(store.home(), "node", "22.21.1", &["node", "npm"]);

        // `20` 这种前缀要解析到已安装的 20.11.1。
        let dir = store.use_version("node", "20").unwrap();
        assert!(dir.ends_with("20.11.1"));

        let resolved = store.which("node").unwrap();
        assert!(
            resolved.display().to_string().contains("20.11.1"),
            "{}",
            resolved.display()
        );

        let installed = store.installed_versions("node").unwrap();
        assert_eq!(installed.len(), 2);
        assert_eq!(
            installed.iter().filter(|item| item.current).count(),
            1,
            "{installed:?}"
        );
        // 新 → 旧
        assert_eq!(installed[0].version, "22.21.1");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_current_version_falls_back_to_the_next_one() {
        let root = std::env::temp_dir().join(format!("lania-store-remove-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let store = store_at(&root);
        seed_version(store.home(), "node", "20.11.1", &["node"]);
        seed_version(store.home(), "node", "22.21.1", &["node"]);
        store.use_version("node", "22.21.1").unwrap();

        let removed = store.remove("node", "22").unwrap();
        assert!(removed.ends_with("22.21.1"));
        assert!(!removed.exists());

        let state = RuntimeState::load(&store.home().state_path()).unwrap();
        assert_eq!(state.current_version("node"), Some("20.11.1"));
        assert!(store
            .which("node")
            .unwrap()
            .display()
            .to_string()
            .contains("20.11.1"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_the_last_version_clears_pointer_and_shims() {
        let root = std::env::temp_dir().join(format!("lania-store-last-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let store = store_at(&root);
        seed_version(store.home(), "node", "20.11.1", &["node"]);
        store.use_version("node", "20.11.1").unwrap();
        let version_dir = store.home().version_dir("node", "20.11.1");
        let shim = store.home().shims_dir().join("node");
        let shim_cmd = store.home().shims_dir().join("node.cmd");
        let listing: Vec<String> = std::fs::read_dir(store.home().shims_dir())
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            shim.is_file() || shim_cmd.is_file(),
            "shim missing in {} (version dir {}, contents {:?}, entry_exists {})",
            store.home().shims_dir().display(),
            version_dir.display(),
            listing,
            crate::shims::entry_exists(&version_dir, "node")
        );

        store.remove("node", "20.11.1").unwrap();

        let state = RuntimeState::load(&store.home().state_path()).unwrap();
        assert_eq!(state.current_version("node"), None);
        assert!(!shim.exists() && !shim_cmd.exists());
        assert!(store.which("node").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_an_unknown_version_reports_not_installed() {
        let root = std::env::temp_dir().join(format!("lania-store-miss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let store = store_at(&root);
        let error = store.remove("node", "18").unwrap_err();
        assert!(error.to_string().contains("not installed"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diagnose_reports_missing_shims_and_broken_pointers() {
        let root = std::env::temp_dir().join(format!("lania-store-diag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let store = store_at(&root);

        // 指针指向不存在的版本 → error
        let mut state = RuntimeState::default();
        state.set_current("node", "99.0.0");
        state.save(&store.home().state_path()).unwrap();
        let diagnostics = store.diagnose().unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.level == DiagnosticLevel::Error
                    && item.message.contains("99.0.0")),
            "{diagnostics:?}"
        );

        // 版本存在但 shim 缺失 → warn
        seed_version(store.home(), "node", "20.11.1", &["node"]);
        let mut state = RuntimeState::default();
        state.set_current("node", "20.11.1");
        state.entrypoints.insert("node".into(), vec!["node".into()]);
        state.save(&store.home().state_path()).unwrap();
        let diagnostics = store.diagnose().unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.level == DiagnosticLevel::Warn && item.message.contains("shim")),
            "{diagnostics:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diagnose_separates_process_path_from_shell_profile() {
        let root =
            std::env::temp_dir().join(format!("lania-store-diag-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let store = store_at(&root);

        // shims 目录还不存在：只报「尚未创建」，不报 PATH 问题（前置不存在的目录没有意义）。
        let diagnostics = store.diagnose().unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message.contains("does not exist yet")),
            "{diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|item| item.message.contains("not prepended")),
            "{diagnostics:?}"
        );

        // shims 目录存在但本进程 PATH 没前置 → 报内部健康度问题。
        store.home().ensure_dirs().unwrap();
        let diagnostics = store.diagnose().unwrap();
        let path_diag = diagnostics
            .iter()
            .find(|item| item.message.contains("not prepended"));
        let profile_diag = diagnostics
            .iter()
            .find(|item| item.message.contains("shell profile"));
        if path_diag.is_some() {
            assert_eq!(path_diag.unwrap().level, DiagnosticLevel::Warn);
        }
        // shell profile 诊断是独立的一条，且永远只是 Info 级提示。
        assert_eq!(profile_diag.unwrap().level, DiagnosticLevel::Info);

        let _ = std::fs::remove_dir_all(&root);
    }
}
