//! `runex` 的稳定 Rust API。
//!
//! 这是宿主（`lan env`）、GUI / IDE 插件与自动化脚本唯一应该依赖的入口。
//! `runex-core` 只是内部引擎：它负责下载 / 校验 / 解压 / shim，
//! 但不应该被外部消费者直接引用。
//!
//! ```no_run
//! use runex::api::{EnvAction, Runex};
//!
//! let api = Runex::resolve()?;
//! api.install("node", "22", &mut |_event| {})?;
//! api.use_version("node", "22")?;
//! let plan = api.plan(EnvAction::Remove, "node", Some("20"), false);
//! let executed = api.apply(&plan, false, |_step| {})?;
//! # Ok::<(), runex::api::RuntimeError>(())
//! ```

use std::path::{Path, PathBuf};

pub use runex_core::{
    build_plan, build_plan_at, describe_provider, execute_plan, execute_plan_with_install_progress,
    platform_limitation, provider_for, supported_language_ids, upstream_for, Diagnostic,
    DiagnosticLevel, EnvAction, EnvPlan, EnvService, GcReport, InstallEvent, InstallOutcome,
    InstalledRuntime, InstalledVersion, Language, PlanStep, Platform, ProviderInfo, RuntimeError,
    RuntimeHome, RuntimeStore, ShimHost, ToolStatus, VersionTag,
};

/// 计划执行后的单步结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutedStep {
    pub label: String,
    pub command: String,
    pub status: String,
    pub output: String,
}

/// 多运行时管理 API。
///
/// 构造后即可调用全部能力：安装、切换、删除、列举、探测、which、doctor、init。
/// 所有会落盘的方法返回 [`RuntimeError`]，不把 `anyhow` 泄漏给调用方。
pub struct Runex {
    home: RuntimeHome,
    platform: Platform,
    cwd: PathBuf,
    store: RuntimeStore,
    service: EnvService,
}

impl Runex {
    /// 使用显式 home / platform 构造；`cwd` 默认为进程当前目录。
    pub fn new(home: RuntimeHome, platform: Platform) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        // 默认生成 `runex` shim；宿主内嵌（`lan env`）用 [`Self::with_shim_host`] 覆盖。
        let store = RuntimeStore::new(home.clone(), platform)
            .with_shim_host(ShimHost::binary("runex", std::env::current_exe().ok()));
        Self {
            store,
            home,
            platform,
            cwd,
            service: EnvService::new(),
        }
    }

    /// 解析 `RUNEX_HOME` / `~/.runex`，并探测当前平台。
    pub fn resolve() -> Result<Self, RuntimeError> {
        let home = RuntimeHome::resolve().map_err(RuntimeError::from)?;
        let platform = Platform::detect()?;
        Ok(Self::new(home, platform))
    }

    /// 设置当前工作目录（用于项目版本文件与项目级探测）。
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = cwd.into();
        self
    }

    /// 覆盖 shim 生成时使用的管理命令。
    ///
    /// 宿主内嵌用 [`ShimHost::lan`]，独立二进制用
    /// `ShimHost::binary("runex", current_exe())`。
    pub fn with_shim_host(mut self, host: ShimHost) -> Self {
        self.store = RuntimeStore::new(self.home.clone(), self.platform).with_shim_host(host);
        self
    }

    pub fn home(&self) -> &RuntimeHome {
        &self.home
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn store(&self) -> &RuntimeStore {
        &self.store
    }

    pub fn service(&self) -> &EnvService {
        &self.service
    }

    /// 把 `~/.runex/shims` 前置到当前进程 PATH（幂等）。
    pub fn activate(&self) {
        self.home.activate();
    }

    /// 解析工具 id；未知工具返回 [`RuntimeError::UnsupportedLanguage`]。
    pub fn language(&self, tool: &str) -> Result<Language, RuntimeError> {
        Language::parse_id(tool).ok_or_else(|| RuntimeError::UnsupportedLanguage {
            language: tool.to_string(),
        })
    }

    /// 全部受支持语言 id（第一批自管语言）。
    pub fn managed_ids(&self) -> &'static [&'static str] {
        supported_language_ids()
    }

    /// 扫描全部工具链（安装状态 + 版本；可能启动子进程）。
    pub fn scan(&self) -> Vec<ToolStatus> {
        self.service.scan(&self.cwd)
    }

    /// 扫描但不探测版本（TUI 首屏使用）。
    pub fn scan_with(&self, with_version: bool) -> Vec<ToolStatus> {
        self.service.scan_with(&self.cwd, with_version)
    }

    /// 单个工具链状态。
    pub fn status(&self, tool: &str) -> Result<ToolStatus, RuntimeError> {
        self.status_with(tool, true)
    }

    /// 单个工具链状态；`with_version=false` 时只判断是否安装。
    pub fn status_with(&self, tool: &str, with_version: bool) -> Result<ToolStatus, RuntimeError> {
        Ok(self
            .service
            .status_with(self.language(tool)?, &self.cwd, with_version))
    }

    /// 某语言由 runex 自己安装的版本。
    pub fn managed_versions(&self, tool: &str) -> Result<Vec<InstalledVersion>, RuntimeError> {
        Ok(self.service.managed_versions(self.language(tool)?))
    }

    /// 全部自管版本（跨语言，新 → 旧）。
    pub fn installed_all(&self) -> Result<Vec<InstalledRuntime>, RuntimeError> {
        self.store.installed_all()
    }

    /// 某语言的自管版本。
    pub fn installed_versions(&self, tool: &str) -> Result<Vec<InstalledRuntime>, RuntimeError> {
        self.store.installed_versions(tool)
    }

    /// 查询上游可用版本（需要网络或缓存）。
    pub fn available_versions(&self, tool: &str) -> Result<Vec<VersionTag>, RuntimeError> {
        self.store.available_versions(tool)
    }

    /// 某语言的 Provider 能力描述（不支持时返回 `None`）。
    pub fn provider_info(&self, tool: &str) -> Option<ProviderInfo> {
        describe_provider(tool, self.platform)
    }

    /// 下载 + 校验 + 解压 + 登记 + 生成 shim。
    pub fn install(
        &self,
        tool: &str,
        version: &str,
        progress: &mut dyn FnMut(InstallEvent),
    ) -> Result<InstallOutcome, RuntimeError> {
        self.store.install(tool, version, progress)
    }

    /// 切换 current 指针并重写 shim。
    pub fn use_version(&self, tool: &str, version: &str) -> Result<PathBuf, RuntimeError> {
        self.store.use_version(tool, version)
    }

    /// 删除自管版本；删 current 时回退到剩余最高版本。
    pub fn remove(&self, tool: &str, version: &str) -> Result<PathBuf, RuntimeError> {
        self.store.remove(tool, version)
    }

    /// 入口命令对应的真实路径（只读 state，不回退 PATH）。
    pub fn which(&self, name: &str) -> Result<PathBuf, RuntimeError> {
        self.store.which(name)
    }

    /// 指针 / shim / PATH 体检。
    pub fn diagnose(&self) -> Result<Vec<Diagnostic>, RuntimeError> {
        self.store.diagnose()
    }

    /// 生成把 shims 加入 PATH 的 shell 片段。
    pub fn init_snippet(&self, shell: &str) -> String {
        self.store.init_snippet(shell)
    }

    /// 清理下载缓存；`max_age = None` 表示全部清空。
    pub fn gc_download_cache(
        &self,
        max_age: Option<std::time::Duration>,
        dry_run: bool,
    ) -> Result<GcReport, RuntimeError> {
        self.store.gc_download_cache(max_age, dry_run)
    }

    /// 在当前 cwd 上启动 TUI（阻塞到用户退出）。
    ///
    /// 使用本 API 构造时配置的 shim host；`runex ui` 与 `lan env ui` 都走这里。
    pub fn run_tui(&self, locale: &str) -> Result<(), RuntimeError> {
        crate::tui::run_with_home(
            self.cwd.clone(),
            locale.to_string(),
            Some(self.home.clone()),
            self.store.shim_host().clone(),
        )
        .map_err(|error| RuntimeError::other(error.to_string()))
    }

    /// 非交互 TUI 快照；供 GUI / CI 验证布局，不进入 raw mode。
    pub fn tui_snapshot(
        &self,
        cols: u16,
        rows: u16,
        down: usize,
        wait_versions: bool,
        locale: &str,
    ) -> Result<String, RuntimeError> {
        crate::tui::snapshot_with_home(
            self.cwd.clone(),
            cols,
            rows,
            down,
            wait_versions,
            locale,
            Some(self.home.clone()),
            self.store.shim_host().clone(),
        )
        .map_err(|error| RuntimeError::other(error.to_string()))
    }

    /// 为当前 cwd 构建一个 dry-run 计划。
    pub fn plan(
        &self,
        action: EnvAction,
        tool: &str,
        version: Option<&str>,
        project: bool,
    ) -> EnvPlan {
        build_plan_at(action, tool, version, &self.cwd, project, Some(&self.home))
    }

    /// 执行计划；`dry_run=true` 时不落盘，返回空结果。
    ///
    /// `on_step` 在每个步骤开始前调用，宿主 / TUI 用它显示进度。
    pub fn apply(
        &self,
        plan: &EnvPlan,
        dry_run: bool,
        mut on_step: impl FnMut(&PlanStep),
    ) -> Result<Vec<ExecutedStep>, RuntimeError> {
        if let Some(error) = plan.error.as_deref() {
            return Err(RuntimeError::other(error));
        }
        if dry_run {
            return Ok(Vec::new());
        }

        let mut steps = Vec::new();
        let results = execute_plan(&self.store, plan, |step| {
            on_step(step);
            steps.push(step.clone());
        })?;
        Ok(steps
            .into_iter()
            .zip(results)
            .map(|(step, output)| ExecutedStep {
                label: step.label,
                command: step.command,
                status: "ok".to_string(),
                output,
            })
            .collect())
    }
}

/// 用 `current_dir` 解析默认 API；失败时返回可读错误。
pub fn default_api() -> Result<Runex, RuntimeError> {
    Runex::resolve()
}
