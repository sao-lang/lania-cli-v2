//! TUI 的应用状态与交互状态机。
//!
//! 与渲染解耦：这里只维护「选中了什么、处于哪个模式、有哪些数据」，
//! 渲染层只读取这些字段，便于单测。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lania_runtime_manager::{
    build_plan, execute_plan, EnvAction, EnvPlan, EnvService, Language, Platform, RuntimeHome,
    RuntimeStore, ToolStatus,
};

/// 当前交互模式。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Browse,
    Input(InputKind),
    Confirm,
    Result,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputKind {
    Install,
    Use,
    Remove,
}

impl InputKind {
    pub fn action(self) -> EnvAction {
        match self {
            Self::Install => EnvAction::Install,
            Self::Use => EnvAction::Use,
            Self::Remove => EnvAction::Remove,
        }
    }

    pub fn label(self, zh: bool) -> &'static str {
        match (self, zh) {
            (Self::Install, true) => "安装",
            (Self::Use, true) => "切换",
            (Self::Remove, true) => "卸载",
            (Self::Install, false) => "install",
            (Self::Use, false) => "use",
            (Self::Remove, false) => "remove",
        }
    }
}

/// 左侧列表中的一行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// 语言分组标题（不可选中），值为语言 id
    Group(String),
    /// 工具条目，值为 `tools` 中的下标
    Tool(usize),
}

pub struct App {
    pub cwd: PathBuf,
    /// 生效的 CLI locale（`en` / `zh`）。
    pub locale: String,
    /// 首轮扫描尚未完成时置 true，用于渲染骨架屏而不是空白。
    pub loading: bool,
    /// 工作目录所在仓库名（无 git 时为 None）。
    pub repo: Option<String>,
    /// 当前 git 分支（detached HEAD 时显示短 hash）。
    pub git_branch: Option<String>,
    /// 设备系统信息，如 `Windows x64 · HOSTNAME`。
    pub system: String,
    pub service: EnvService,
    pub tools: Vec<ToolStatus>,
    /// 左侧列表行（分组标题 + 工具）
    pub rows: Vec<Row>,
    /// `rows` 中的当前下标（永远指向 `Row::Tool`）
    pub cursor: usize,
    /// 左侧列表滚动偏移（单位：显示行）
    pub list_offset: usize,
    /// 左侧列表可视高度（渲染时写入）
    pub list_height: usize,
    pub mode: Mode,
    pub input: String,
    pub project_pin: bool,
    /// lania 自管运行时（安装/切换/卸载的唯一入口）。
    pub store: Option<RuntimeStore>,
    /// 已安装版本缓存，key = 语言 id，值为 `(来源, 版本)`。
    pub versions_cache: HashMap<String, Vec<(String, String)>>,
    pub pending_plan: Option<EnvPlan>,
    pub log: Vec<String>,
    pub status: String,
    pub should_quit: bool,
    /// 右侧详情滚动偏移
    pub scroll: u16,
    /// 退出 Result 后是否需要刷新
    pub pending_refresh: bool,
    /// 同步路径是否补全版本号（`--no-versions` 时为 false，只验证布局）。
    pub load_versions: bool,
}

/// 后台扫描的结果。
pub struct ScanResult {
    pub tools: Vec<ToolStatus>,
}

/// 后台补齐的一批 `(tool_id, version)`，以及需要纠正 installed 状态的工具。
pub struct VersionBatch {
    pub versions: Vec<(String, String)>,
    /// `.NET` 系在首屏用占位判断，补齐阶段要纠正真实安装状态。
    pub installed_fixes: Vec<(String, bool)>,
}

impl App {
    pub fn new(cwd: PathBuf, locale: impl Into<String>) -> Self {
        let locale = locale.into();
        let status = if locale.starts_with("zh") {
            "就绪".to_string()
        } else {
            "Ready".to_string()
        };
        let (repo, git_branch) = workspace_git(&cwd);
        let system = system_label();
        let store = Platform::detect().ok().and_then(|platform| {
            RuntimeHome::resolve()
                .ok()
                .map(|home| RuntimeStore::new(home, platform))
        });
        Self {
            cwd,
            locale,
            loading: true,
            repo,
            git_branch,
            system,
            service: EnvService::new(),
            tools: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            list_offset: 0,
            list_height: 20,
            mode: Mode::Browse,
            input: String::new(),
            project_pin: false,
            store,
            versions_cache: HashMap::new(),
            pending_plan: None,
            log: Vec::new(),
            status,
            should_quit: false,
            scroll: 0,
            pending_refresh: false,
            load_versions: true,
        }
    }

    pub fn zh(&self) -> bool {
        self.locale.starts_with("zh")
    }

    /// 选择当前 locale 的文案。
    pub fn tr(&self, en: &'static str, zh: &'static str) -> &'static str {
        if self.zh() {
            zh
        } else {
            en
        }
    }

    /// 测试专用：关闭版本探测，避免改动真实工具链。
    #[cfg(test)]
    pub(crate) fn new_dry_run(cwd: PathBuf, locale: &str) -> Self {
        let mut app = Self::new(cwd, locale);
        app.load_versions = false;
        app
    }

    /// 测试专用：注入固定的工具列表，不依赖本机 PATH。
    #[cfg(test)]
    pub(crate) fn set_tools_for_test(&mut self, tools: Vec<ToolStatus>) {
        self.tools = tools;
        self.loading = false;
        self.rebuild_rows();
        self.cursor = self.rows.len().saturating_sub(1);
        self.snap_cursor_to_tool();
        self.ensure_cursor_visible();
    }

    /// 在后台线程里启动一次扫描，返回接收端（TUI 用它做非阻塞轮询）。
    pub fn spawn_scan(&self) -> std::sync::mpsc::Receiver<ScanResult> {
        let (tx, rx) = std::sync::mpsc::channel();
        let cwd = self.cwd.clone();
        std::thread::spawn(move || {
            // 独立实例：`EnvService` 是零状态的，服务层缓存是进程级的。
            let service = EnvService::new();
            let tools = service.scan_with(&cwd, false);
            let _ = tx.send(ScanResult { tools });
        });
        rx
    }

    /// 后台补齐所有「已安装但还没版本号」的工具的版本。
    ///
    /// 首屏为了快，`scan_with(false)` 不跑 `--version`；如果只靠移动光标懒加载，
    /// 用户会看到一列 `…` 且不知道要按什么键。这里在扫描完成后接着补全。
    pub fn spawn_version_fill(&self) -> std::sync::mpsc::Receiver<VersionBatch> {
        let (tx, rx) = std::sync::mpsc::channel();
        // 只补「可能已安装」的：显式未安装的跳过；`.NET` 系首屏用占位判断，
        // 需要真实探测来纠正 installed 状态。
        let targets: Vec<(String, String)> = self
            .tools
            .iter()
            .filter(|tool| {
                tool.version.is_none()
                    && (tool.installed || matches!(tool.language.as_str(), "csharp" | "fsharp"))
            })
            .map(|tool| (tool.id.clone(), tool.language.clone()))
            .collect();
        if targets.is_empty() {
            return rx;
        }
        std::thread::spawn(move || {
            // 每个版本探测都是一次子进程启动（npm/pnpm/pip 要 0.5~1 秒），
            // 串行累计会超过 5 秒，因此按 CPU 数并行。
            let threads = std::thread::available_parallelism()
                .map(|value| value.get())
                .unwrap_or(4)
                .clamp(1, 8);
            for chunk in targets.chunks(threads) {
                let resolved: Vec<(String, Option<String>, Option<bool>)> =
                    std::thread::scope(|scope| {
                        let handles: Vec<_> = chunk
                            .iter()
                            .map(|(tool_id, language_id)| {
                                scope.spawn(move || {
                                    let service = EnvService::new();
                                    let Some(language) = Language::parse_id(language_id) else {
                                        return (tool_id.clone(), None, None);
                                    };
                                    // `.NET` 系要连 installed 一起纠正。
                                    let is_dotnet =
                                        matches!(language_id.as_str(), "csharp" | "fsharp");
                                    let real = if is_dotnet {
                                        Some(service.status_with(
                                            language,
                                            std::path::Path::new("."),
                                            true,
                                        ))
                                    } else {
                                        None
                                    };
                                    let version = if tool_id == language_id {
                                        real.as_ref()
                                            .and_then(|status| status.version.clone())
                                            .or_else(|| service.resolve_version(language))
                                    } else {
                                        service.resolve_companion_version(tool_id)
                                    };
                                    let installed = real.map(|status| status.installed);
                                    (tool_id.clone(), version, installed)
                                })
                            })
                            .collect();
                        handles
                            .into_iter()
                            .filter_map(|handle| handle.join().ok())
                            .collect()
                    });

                let versions: Vec<(String, String)> = resolved
                    .iter()
                    .filter_map(|(id, version, _)| version.clone().map(|v| (id.clone(), v)))
                    .collect();
                let installed_fixes: Vec<(String, bool)> = resolved
                    .iter()
                    .filter_map(|(id, _, installed)| installed.map(|flag| (id.clone(), flag)))
                    .collect();
                if !versions.is_empty() || !installed_fixes.is_empty() {
                    let batch = VersionBatch {
                        versions,
                        installed_fixes,
                    };
                    if tx.send(batch).is_err() {
                        return;
                    }
                }
            }
        });
        rx
    }

    /// 应用一条后台补齐的版本（可能有多条）。
    pub fn apply_versions(&mut self, batch: VersionBatch) {
        for (tool_id, installed) in batch.installed_fixes {
            if let Some(tool) = self.tools.iter_mut().find(|tool| tool.id == tool_id) {
                tool.installed = installed;
            }
        }
        for (tool_id, version) in batch.versions {
            if let Some(tool) = self.tools.iter_mut().find(|tool| tool.id == tool_id) {
                if tool.version.is_none() {
                    tool.version = Some(version);
                }
            }
        }
    }

    /// 应用后台扫描结果（在主线程调用，保证 UI 状态一致）。
    pub fn apply_scan(&mut self, result: ScanResult) {
        self.tools = result.tools;
        self.rebuild_rows();
        let languages: std::collections::HashSet<String> = self
            .tools
            .iter()
            .map(|tool| tool.language.clone())
            .collect();
        self.versions_cache.retain(|key, _| languages.contains(key));
        self.loading = false;
        self.status = format!("{} {}", self.tools.len(), self.tr("tools", "个工具"));
        self.ensure_cursor_visible();
        // 这里必须保持懒加载：全量跑 `--version` 要 5 秒以上（npm/pnpm/pip 的
        // 启动开销尤其大），不能在主线程做。其余工具的版本由
        // [`spawn_version_fill`] 在后台线程分批补上。
        self.resolve_current_version();
        self.prime_versions();
    }

    pub fn refresh(&mut self) {
        self.loading = true;
        self.status = self.tr("Scanning toolchains…", "扫描环境中…").into();
        // 先只判断安装状态（不跑 `--version`），让首屏尽快出现。
        self.tools = self.service.scan_with(&self.cwd, false);
        self.rebuild_rows();
        // 丢弃已不存在的语言缓存，其余保留（避免每次刷新都重扫）。
        let languages: std::collections::HashSet<String> = self
            .tools
            .iter()
            .map(|tool| tool.language.clone())
            .collect();
        self.versions_cache.retain(|key, _| languages.contains(key));
        self.loading = false;
        self.status = format!("{} {}", self.tools.len(), self.tr("tools", "个工具"));
        self.ensure_cursor_visible();
        // CLI / snapshot 没有事件循环，只能同步补全；交互路径走
        // [`spawn_version_fill`] 的后台线程，不受这里影响。
        if self.load_versions {
            self.resolve_all_versions();
            self.prime_versions();
        }
    }

    /// 补全当前选中项的版本号（`scan_with(false)` 之后调用）。
    pub fn resolve_current_version(&mut self) {
        let Some(index) = self.selected_tool_index() else {
            return;
        };
        self.resolve_version_at(index);
    }

    /// 同步补全所有「已安装但还没版本号」的工具（CLI / snapshot 路径使用）。
    ///
    /// TUI 交互路径不用它，而是走 [`spawn_version_fill`]：逐个跑 `--version`
    /// 在 Windows 上要好几秒，不能阻塞界面。
    pub fn resolve_all_versions(&mut self) {
        let indices: Vec<usize> = self
            .tools
            .iter()
            .enumerate()
            .filter(|(_, tool)| tool.version.is_none())
            .map(|(index, _)| index)
            .collect();
        for index in indices {
            self.resolve_version_at(index);
        }
    }

    fn resolve_version_at(&mut self, index: usize) {
        let Some(tool) = self.tools.get(index) else {
            return;
        };
        if tool.version.is_some() {
            return;
        }
        // `.NET` 系在首屏用「可执行文件存在」占位，这里用真实探测纠正：
        // 只装 runtime 时 `dotnet --list-sdks` 为空，应当显示未安装。
        if matches!(tool.language.as_str(), "csharp" | "fsharp") {
            let language = Language::parse_id(&tool.language);
            if let Some(language) = language {
                let real = self.service.status_with(language, &self.cwd, true);
                if let Some(tool) = self.tools.get_mut(index) {
                    tool.installed = real.installed;
                    tool.version = real.version;
                }
                return;
            }
        }
        if !tool.installed {
            return;
        }
        let tool_id = tool.id.clone();
        let language_id = tool.language.clone();

        let version = if tool_id == language_id {
            Language::parse_id(&language_id)
                .and_then(|language| self.service.resolve_version(language))
        } else {
            // 附属工具：直接跑它自己的 `--version`。
            self.service.resolve_companion_version(&tool_id)
        };

        if let Some(version) = version {
            if let Some(tool) = self.tools.get_mut(index) {
                tool.version = Some(version);
            }
        }
    }

    /// 重建左侧列表：语言作为分组标题，其附属包管理器紧跟在该语言下方。
    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        for (index, tool) in self.tools.iter().enumerate() {
            if tool.id != tool.language {
                continue;
            }
            rows.push(Row::Group(tool.language.clone()));
            rows.push(Row::Tool(index));
            // 工具列表里 companions 排在所有语言之后，这里按归属语言重新插入。
            for (companion_index, companion) in self.tools.iter().enumerate() {
                if companion.language == tool.language
                    && companion.id != companion.language
                    && companion_index != index
                {
                    rows.push(Row::Tool(companion_index));
                }
            }
        }
        self.rows = rows;
        self.snap_cursor_to_tool();
    }

    /// 把光标修正到最近的 `Row::Tool` 上。
    fn snap_cursor_to_tool(&mut self) {
        if self.rows.is_empty() {
            self.cursor = 0;
            return;
        }
        if self.cursor >= self.rows.len() {
            self.cursor = self.rows.len() - 1;
        }
        if matches!(self.rows.get(self.cursor), Some(Row::Group(_))) {
            if let Some(index) = self.next_selectable(self.cursor, 1) {
                self.cursor = index;
            } else if let Some(index) = self.next_selectable(self.cursor, -1) {
                self.cursor = index;
            }
        }
    }

    /// 从 `from` 开始按 `step` 方向找下一个可选中的行。
    fn next_selectable(&self, from: usize, step: isize) -> Option<usize> {
        let mut index = from as isize + step;
        while index >= 0 && (index as usize) < self.rows.len() {
            if matches!(self.rows[index as usize], Row::Tool(_)) {
                return Some(index as usize);
            }
            index += step;
        }
        None
    }

    pub fn selected_tool_index(&self) -> Option<usize> {
        match self.rows.get(self.cursor) {
            Some(Row::Tool(index)) => Some(*index),
            _ => None,
        }
    }

    pub fn current(&self) -> Option<&ToolStatus> {
        self.selected_tool_index()
            .and_then(|index| self.tools.get(index))
    }

    /// 移动光标。`delta` 支持多步（PageUp/PageDown/Home/End）。
    pub fn move_cursor(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let step = if delta >= 0 { 1 } else { -1 };
        for _ in 0..delta.unsigned_abs() {
            match self.next_selectable(self.cursor, step) {
                Some(next) => self.cursor = next,
                None => break,
            }
        }
        self.scroll = 0;
        self.ensure_cursor_visible();
        // 懒加载：只给新选中的工具补版本号 + 版本列表，不重扫全量。
        self.resolve_current_version();
        self.prime_versions();
    }

    /// 保证光标所在行位于可视窗口内。
    ///
    /// 渲染时分组标题会与其后的语言工具行合并成一行，因此这里必须用
    /// `display_index_of` 把 row 下标换算成显示行号，否则滚动会错位。
    pub fn ensure_cursor_visible(&mut self) {
        let visible = self.list_height.max(1);
        let display_row = self.display_index_of(self.cursor);
        let total = self.display_row_count();
        let max_offset = total.saturating_sub(visible);

        if display_row < self.list_offset {
            self.list_offset = display_row;
        } else if display_row >= self.list_offset + visible {
            self.list_offset = display_row + 1 - visible;
        }
        self.list_offset = self.list_offset.min(max_offset);
    }

    /// 实际会被画出来的显示行数（分组标题与语言工具行合并）。
    pub fn display_row_count(&self) -> usize {
        let mut count = 0usize;
        let mut index = 0usize;
        while index < self.rows.len() {
            match &self.rows[index] {
                Row::Group(_) => {
                    if matches!(self.rows.get(index + 1), Some(Row::Tool(_))) {
                        count += 1;
                        index += 2;
                        continue;
                    }
                }
                Row::Tool(_) => count += 1,
            }
            index += 1;
        }
        count
    }

    /// 某个 row 下标对应的显示行号。
    pub fn display_index_of(&self, row_index: usize) -> usize {
        let mut display = 0usize;
        let mut index = 0usize;
        while index < self.rows.len() && index < row_index {
            match &self.rows[index] {
                Row::Group(_) => {
                    if matches!(self.rows.get(index + 1), Some(Row::Tool(_))) {
                        if index + 1 == row_index {
                            return display;
                        }
                        display += 1;
                        index += 2;
                        continue;
                    }
                }
                Row::Tool(_) => display += 1,
            }
            index += 1;
        }
        display
    }

    /// 右侧详情滚动（到达边界后夹紧）。
    pub fn scroll_detail(&mut self, delta: isize) {
        self.scroll = (self.scroll as isize + delta).clamp(0, u16::MAX as isize) as u16;
    }

    /// 当前工具是否由 lania 自己管理（第一批语言）。
    pub fn is_managed(&self) -> bool {
        self.current().is_some_and(|tool| tool.managed)
    }

    /// 右侧「当前来源」文案：`lania` / `system` / `—`。
    pub fn current_source(&self) -> Option<&str> {
        self.current().and_then(|tool| tool.source.as_deref())
    }

    /// 若当前语言还没缓存版本，则扫描一次（切换工具时调用）。
    pub fn prime_versions(&mut self) {
        let Some(tool) = self.current() else { return };
        let language_id = tool.language.clone();
        if self.versions_cache.contains_key(&language_id) {
            return;
        }
        let Some(language) = Language::parse_id(&language_id) else {
            return;
        };
        let versions: Vec<(String, String)> = self
            .service
            .managed_versions(language)
            .into_iter()
            .map(|item| (item.source, item.version))
            .collect();
        self.versions_cache.insert(language_id, versions);
    }

    /// 强制重新读取当前语言的已安装版本（按 `v` 时调用）。
    pub fn reload_versions(&mut self) {
        let Some(tool) = self.current() else { return };
        let language_id = tool.language.clone();
        self.versions_cache.remove(&language_id);
        self.prime_versions();
        let count = self
            .versions_cache
            .get(&language_id)
            .map(Vec::len)
            .unwrap_or(0);
        self.status = if count == 0 {
            format!(
                "{language_id}: {}",
                self.tr("no installed versions", "无已安装版本")
            )
        } else {
            format!(
                "{language_id}: {count} {}",
                self.tr("installed versions", "个已安装版本")
            )
        };
    }

    /// 当前语言已缓存的版本。
    pub fn cached_versions(&self) -> &[(String, String)] {
        self.current()
            .and_then(|tool| self.versions_cache.get(&tool.language))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn doctor(&mut self) {
        let missing: Vec<String> = self
            .tools
            .iter()
            .filter(|tool| !tool.installed && tool.id == tool.language)
            .map(|tool| tool.name.clone())
            .collect();
        let external: Vec<String> = self
            .tools
            .iter()
            .filter(|tool| {
                tool.installed
                    && tool.id == tool.language
                    && tool.source.as_deref() == Some("system")
            })
            .map(|tool| tool.name.clone())
            .collect();
        let mut lines = vec![
            self.tr("Doctor result", "体检结果").into(),
            String::new(),
            format!(
                "{:<16}{}",
                self.tr("Missing runtime", "缺失运行时"),
                if missing.is_empty() {
                    self.tr("none", "无").to_string()
                } else {
                    missing.join(", ")
                }
            ),
            format!(
                "{:<16}{}",
                self.tr("System install", "系统安装（非自管）"),
                if external.is_empty() {
                    self.tr("none", "无").to_string()
                } else {
                    external.join(", ")
                }
            ),
        ];
        // 自管状态与 PATH 一致性检查（shim 是否已生成、是否在 PATH 上）。
        match &self.store {
            Some(store) => match store.diagnose() {
                Ok(diagnostics) if diagnostics.is_empty() => {
                    lines.push(format!(
                        "{:<16}{}",
                        self.tr("Self-managed", "自管状态"),
                        self.tr("ok", "正常")
                    ));
                }
                Ok(diagnostics) => {
                    for diagnostic in diagnostics {
                        let prefix = match diagnostic.level {
                            lania_runtime_manager::DiagnosticLevel::Error => "✗",
                            lania_runtime_manager::DiagnosticLevel::Warn => "!",
                            lania_runtime_manager::DiagnosticLevel::Info => "·",
                        };
                        lines.push(format!("{prefix} {}", diagnostic.message));
                    }
                }
                Err(error) => lines.push(format!("✗ {error}")),
            },
            None => lines.push(format!(
                "✗ {}",
                self.tr("runtime store unavailable", "运行时存储不可用")
            )),
        }
        self.log = lines;
        self.mode = Mode::Result;
        self.status = self.tr("Esc to go back", "Esc 返回").into();
    }

    pub fn begin_input(&mut self, kind: InputKind) {
        if self.current().is_none() {
            return;
        }
        self.input.clear();
        self.mode = Mode::Input(kind);
    }

    /// 确认输入：生成计划并进入确认态。
    pub fn submit_input(&mut self) {
        let Mode::Input(kind) = self.mode else { return };
        let Some(tool) = self.current() else { return };
        let version = self.input.trim().to_string();
        let tool_id = tool.language.clone();
        let plan = build_plan(
            kind.action(),
            &tool_id,
            if version.is_empty() {
                None
            } else {
                Some(version.as_str())
            },
            &self.cwd,
            self.project_pin,
        );
        self.pending_plan = Some(plan);
        self.mode = Mode::Confirm;
    }

    /// 用户确认后执行计划。
    pub fn execute_pending(&mut self) {
        let Some(plan) = self.pending_plan.take() else {
            self.mode = Mode::Browse;
            return;
        };
        self.mode = Mode::Result;
        self.log.clear();
        self.log
            .push(format!("$ {} {}", plan.action.as_str(), plan.tool));

        if let Some(error) = plan.error.as_deref() {
            self.log.push(format!("✗ {error}"));
            self.status = self.tr("Failed", "失败").into();
            return;
        }

        let Some(store) = self.store.as_ref() else {
            self.log.push(format!(
                "✗ {}",
                self.tr("runtime store unavailable", "运行时存储不可用")
            ));
            self.status = self.tr("Failed", "失败").into();
            return;
        };

        let mut steps = Vec::new();
        let result = execute_plan(store, &plan, |step| steps.push(step.clone()));
        let mut lines: Vec<String> = steps
            .iter()
            .map(|step| format!("› {}", step.command))
            .collect();
        let mut failed = false;
        match result {
            Ok(results) => lines.extend(results.into_iter().map(|line| format!("  ✓ {line}"))),
            Err(error) => {
                lines.push(format!("  ✗ {error}"));
                failed = true;
            }
        }
        self.log.extend(lines);
        self.status = if failed {
            self.tr("Execution failed", "执行失败").into()
        } else {
            format!("{} {}", plan.action.as_str(), self.tr("completed", "完成"))
        };
        self.pending_refresh = true;
    }
}

/// 从 cwd 向上查找 `.git`，返回 `(仓库名, 分支)`。
///
/// 直接读文件而不是调用 `git`，避免 TUI 首屏再启动一个子进程；
/// worktree 的 `.git` 是文件，内容形如 `gitdir: <path>`，这里一并处理。
fn workspace_git(cwd: &Path) -> (Option<String>, Option<String>) {
    let mut dir = if cwd.is_dir() {
        cwd.to_path_buf()
    } else {
        cwd.parent().unwrap_or(cwd).to_path_buf()
    };
    loop {
        let dot_git = dir.join(".git");
        if dot_git.exists() {
            let repo = dir
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string);
            let git_dir = if dot_git.is_dir() {
                dot_git
            } else {
                std::fs::read_to_string(&dot_git)
                    .ok()
                    .and_then(|content| {
                        content
                            .trim()
                            .strip_prefix("gitdir:")
                            .map(str::trim)
                            .map(PathBuf::from)
                    })
                    .map(|path| {
                        if path.is_absolute() {
                            path
                        } else {
                            dir.join(path)
                        }
                    })
                    .unwrap_or(dot_git)
            };
            let branch = std::fs::read_to_string(git_dir.join("HEAD"))
                .ok()
                .and_then(|head| parse_git_head(head.trim()));
            return (repo, branch);
        }
        if !dir.pop() {
            return (None, None);
        }
    }
}

fn parse_git_head(head: &str) -> Option<String> {
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        return Some(branch.to_string());
    }
    if head.len() >= 7 {
        return Some(format!("detached@{}", &head[..7]));
    }
    None
}

fn system_label() -> String {
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|host| !host.trim().is_empty());
    match host {
        Some(host) => format!("{os} {arch} · {host}"),
        None => format!("{os} {arch}"),
    }
}
