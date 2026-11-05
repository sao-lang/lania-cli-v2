//! `runex` CLI / TUI 的人类语言（locale）。
//!
//! 只做 **en / zh** 两种；未识别的值回落到英文。读取顺序：
//! `--locale` → `RUNEX_LOCALE` → `$RUNEX_HOME/config.json` →
//! `LC_ALL` → `LC_MESSAGES` → `LANG` → `LANGUAGE`。
//!
//! 「设置语言」用 `runex locale <en|zh|auto>`：写入 `$RUNEX_HOME/config.json`，
//! 之后 CLI 与 TUI 都从同一个值取文案，不再各读各的环境变量。
//!
//! 这不是运行时语言（node/python/...）的列表，后者见 `runex::api`。

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::api::{EnvPlan, RuntimeHome};

/// 保存用户选择语言的配置文件（位于 `$RUNEX_HOME` 下）。
pub const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    En,
    Zh,
}

/// locale 是从哪里来的（`runex locale` 展示用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleSource {
    Cli,
    Env,
    Config,
    Default,
}

impl LocaleSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Env => "env",
            Self::Config => "config",
            Self::Default => "default",
        }
    }

    pub fn label(self, zh: bool) -> &'static str {
        match (self, zh) {
            (Self::Cli, true) => "命令行",
            (Self::Env, true) => "环境变量",
            (Self::Config, true) => "配置文件",
            (Self::Default, true) => "默认",
            (Self::Cli, false) => "command line",
            (Self::Env, false) => "environment",
            (Self::Config, false) => "config",
            (Self::Default, false) => "default",
        }
    }
}

impl Locale {
    /// 宽松解析：未识别的值回落到英文。
    pub fn parse(value: &str) -> Self {
        Self::from_code(value).unwrap_or(Self::En)
    }

    /// 严格解析：只接受 `en*` / `zh*`，其余返回 `None`（供 `runex locale` 校验）。
    pub fn from_code(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        if value.is_empty() {
            return None;
        }
        if value.starts_with("zh") {
            Some(Self::Zh)
        } else if value.starts_with("en") {
            Some(Self::En)
        } else {
            None
        }
    }

    /// 按与 lania / 终端约定一致的顺序探测（不看配置文件）。
    pub fn detect() -> Self {
        for key in ["RUNEX_LOCALE", "LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Some(value) = env_value(key) {
                return Self::parse(&value);
            }
        }
        Self::En
    }

    /// 从 `--locale` 或环境变量得到最终 locale（不读配置文件）。
    ///
    /// 保留这个入口是为了兼容只依赖 `runex::Locale` 的调用方；
    /// CLI / TUI 请用 [`Locale::resolve_with_home`]，它会把 `runex locale` 的选择也算进去。
    pub fn resolve(explicit: Option<&str>) -> Self {
        if let Some(value) = explicit.filter(|value| !value.trim().is_empty()) {
            return Self::parse(value);
        }
        Self::detect()
    }

    /// CLI / TUI 的统一解析入口：显式参数 > 环境变量 > 配置文件 > 系统 locale。
    pub fn resolve_with_home(explicit: Option<&str>, home: &RuntimeHome) -> Self {
        Self::resolve_detailed(explicit, home).0
    }

    /// 与 [`Locale::resolve_with_home`] 相同，但一并返回来源，便于 `runex locale` 展示。
    pub fn resolve_detailed(explicit: Option<&str>, home: &RuntimeHome) -> (Self, LocaleSource) {
        if let Some(value) = explicit.filter(|value| !value.trim().is_empty()) {
            return (Self::parse(value), LocaleSource::Cli);
        }
        if let Some(value) = env_value("RUNEX_LOCALE") {
            return (Self::parse(&value), LocaleSource::Env);
        }
        if let Some(locale) = Self::saved(home) {
            return (locale, LocaleSource::Config);
        }
        for key in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Some(value) = env_value(key) {
                return (Self::parse(&value), LocaleSource::Env);
            }
        }
        (Self::En, LocaleSource::Default)
    }

    /// 读取 `$RUNEX_HOME/config.json` 里保存的语言；文件不存在或损坏时返回 `None`。
    pub fn saved(home: &RuntimeHome) -> Option<Self> {
        let raw = std::fs::read_to_string(settings_path(home)).ok()?;
        let settings: Settings = serde_json::from_str(&raw).ok()?;
        settings.locale.as_deref().and_then(Self::from_code)
    }

    /// 保存语言选择；`None` 表示清除（`auto`，回到环境变量 / 系统 locale）。
    ///
    /// 返回写入的配置文件路径。
    pub fn save(home: &RuntimeHome, locale: Option<Self>) -> Result<PathBuf> {
        let path = settings_path(home);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut settings = read_settings(&path)?;
        settings.locale = locale.map(|locale| locale.code().to_string());
        let body =
            serde_json::to_string_pretty(&settings).context("failed to encode the runex config")?;
        std::fs::write(&path, format!("{body}\n"))
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(path)
    }

    /// 配置文件路径（`runex locale` 的输出里会显示它）。
    pub fn config_path(home: &RuntimeHome) -> PathBuf {
        settings_path(home)
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
        }
    }

    pub fn is_zh(self) -> bool {
        self == Self::Zh
    }

    /// 选择当前 locale 的文案。
    pub fn tr(self, en: &'static str, zh: &'static str) -> &'static str {
        if self.is_zh() {
            zh
        } else {
            en
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locale: Option<String>,
}

fn settings_path(home: &RuntimeHome) -> PathBuf {
    home.root().join(CONFIG_FILE)
}

/// 读已有配置；损坏时报错而不是静默覆盖（避免丢掉用户其它设置）。
fn read_settings(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(Settings::default());
    }
    serde_json::from_str(&raw).with_context(|| format!("invalid config at {}", path.display()))
}

fn env_value(key: &str) -> Option<String> {
    std::env::var_os(key)
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
}

/// 供需要把 locale 交给子进程/命令行的场景使用。
pub fn locale_from_env() -> OsString {
    OsString::from(Locale::detect().code())
}

// ---------------------------------------------------------------------------
// 计划文案：核心层（`runex-core`）只产出英文 label，
// TUI 与 CLI 在这里统一翻成当前语言，避免两套界面各说各话。
// ---------------------------------------------------------------------------

/// 把计划里给人看的文案（步骤说明、备注、可预期的错误）翻成 `locale`。
///
/// `command` 字段保留原样：它是给脚本/JSON 用的等价命令，TUI 不再展示它。
pub fn localize_plan(plan: &mut EnvPlan, home: Option<&RuntimeHome>, locale: Locale) {
    let zh = locale.is_zh();
    let tool = plan.tool.clone();
    let version = plan.version.clone();
    for step in &mut plan.steps {
        step.label = localized_step_label(
            &step.kind,
            step.path.as_deref(),
            step.content.as_deref(),
            &tool,
            version.as_deref(),
            home,
            locale,
        );
    }
    if plan.steps.iter().any(|step| step.kind == "write-file") {
        plan.notes = vec![if zh {
            "项目级锁定只影响当前目录及其子目录".to_string()
        } else {
            "The project pin only affects this directory and its subdirectories".to_string()
        }];
    }
    if let Some(error) = plan.error.take() {
        plan.error = Some(localize_error(&tool, &error, locale));
    }
}

fn localized_step_label(
    kind: &str,
    path: Option<&str>,
    content: Option<&str>,
    tool: &str,
    version: Option<&str>,
    home: Option<&RuntimeHome>,
    locale: Locale,
) -> String {
    let zh = locale.is_zh();
    let version = version.unwrap_or_default();
    match kind {
        "install" => {
            let upstream = crate::api::upstream_for(tool);
            let dir = home
                .map(|home| home.display_for_shell(&home.runtimes_dir().join(tool)))
                .unwrap_or_else(|| format!("~/.runex/runtimes/{tool}"));
            if zh {
                format!("从 {upstream} 下载 {tool} {version} 到 {dir}")
            } else {
                format!("download {tool} {version} from {upstream} into {dir}")
            }
        }
        "use" => {
            if zh {
                format!("把 {tool} 切换到 {version} 并重建入口")
            } else {
                format!("switch {tool} to {version} and regenerate shims")
            }
        }
        "remove" => {
            if zh {
                format!("删除 runex 自管的 {tool} {version}")
            } else {
                format!("delete runex-managed {tool} {version}")
            }
        }
        "write-file" => {
            let file = path
                .and_then(|path| Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .unwrap_or(path.unwrap_or("the version file"));
            let clearing = content.is_none_or(str::is_empty);
            match (zh, clearing) {
                (true, true) => format!("清除 {file} 的项目锁定"),
                (true, false) => format!("把 {file} 锁定到 {version}"),
                (false, true) => format!("clear the {file} project pin"),
                (false, false) => format!("pin {file} to {version}"),
            }
        }
        _ => kind.to_string(),
    }
}

/// 把核心层常见的英文错误翻成中文；不认识的错误保留原文（技术细节不该丢）。
fn localize_error(tool: &str, error: &str, locale: Locale) -> String {
    if !locale.is_zh() {
        return error.to_string();
    }
    if error.contains("install requires a version") {
        return format!("安装 {tool} 需要版本号，例如 `{tool} 20`");
    }
    if error.starts_with("invalid version") {
        return format!("版本号不合法：{error}");
    }
    if let Some(rest) = error.split_once(" is not self-managed yet; available now: ") {
        return format!("{} 尚未支持自管；当前可用：{}", rest.0, rest.1);
    }
    if let Some(rest) = error.split_once("unsupported tool ") {
        if let Some((name, supported)) = rest.1.split_once("; supported: ") {
            return format!("不支持的工具 {name}；支持：{supported}");
        }
    }
    if error.starts_with("use requires a version") {
        return "切换需要版本号（或加 --project 清除项目锁定）".to_string();
    }
    if error == "remove requires a version" {
        return "删除需要版本号".to_string();
    }
    if error.ends_with("has no project version file") {
        return format!("{tool} 没有项目版本文件");
    }
    error.to_string()
}

/// 把 `RuntimeStore::diagnose` 的英文诊断翻成中文；不认识的保留原文。
///
/// 诊断里有真实路径与状态，翻译时只替换外壳文案，路径原样保留。
pub fn localize_diagnostic(message: &str, locale: Locale) -> String {
    if !locale.is_zh() {
        return message.to_string();
    }
    if message.contains(" does not exist yet; it is created on the first") {
        let dir = message
            .split_once(" does not exist yet")
            .map(|(dir, _)| dir)
            .unwrap_or(message);
        return format!("{dir} 尚不存在；第一次安装运行时时会创建");
    }
    if message.contains(" is not prepended to this process PATH") {
        let dir = message
            .split_once(" is not prepended to this process PATH")
            .map(|(dir, _)| dir)
            .unwrap_or(message);
        return format!(
            "{dir} 未加入当前进程的 PATH；命令可能回退到系统运行时。这通常是 lania 的 bug，请反馈"
        );
    }
    if message.contains(" is not in your shell profile") {
        let dir = message
            .split_once(" is not in your shell profile")
            .map(|(dir, _)| dir)
            .unwrap_or(message);
        return format!(
            "{dir} 未写入 shell 配置；把 init 片段追加进去，外部 shell 才会使用自管运行时"
        );
    }
    if message.contains(" points at ") && message.ends_with(" is missing") {
        if let Some((language, rest)) = message.split_once(" points at ") {
            if let Some((version, tail)) = rest.split_once(" but ") {
                let dir = tail.strip_suffix(" is missing").unwrap_or(tail);
                return format!("{language} 指向 {version}，但 {dir} 不存在");
            }
        }
    }
    if let Some(rest) = message.strip_prefix("missing shim for `") {
        if let Some((entry, tail)) = rest.split_once("` (expected in ") {
            let dir = tail.strip_suffix(')').unwrap_or(tail);
            return format!("缺少 `{entry}` 的 shim（应在 {dir}）");
        }
    }
    message.to_string()
}

/// 计划成功执行后，TUI / CLI 共用的结果行。
///
/// `resolved_install_version` 是安装事件里服务端/Provider 最终落地的版本
/// （`install node 23` 可能解析成 `23.11.0`），没有时退回计划里的版本。
pub fn plan_result_lines(
    plan: &EnvPlan,
    locale: Locale,
    resolved_install_version: Option<&str>,
) -> Vec<String> {
    let zh = locale.is_zh();
    let version = plan.version.as_deref().unwrap_or_default();
    plan.steps
        .iter()
        .map(|step| match step.kind.as_str() {
            "install" => {
                let installed = resolved_install_version.unwrap_or(version);
                if zh {
                    format!("已安装 {} {installed}", plan.tool)
                } else {
                    format!("installed {} {installed}", plan.tool)
                }
            }
            "use" => {
                if zh {
                    format!("已切换 {} → {version}", plan.tool)
                } else {
                    format!("switched {} -> {version}", plan.tool)
                }
            }
            "remove" => {
                if zh {
                    format!("已删除 {} {version}", plan.tool)
                } else {
                    format!("removed {} {version}", plan.tool)
                }
            }
            "write-file" => {
                let path = step.path.as_deref().unwrap_or("the version file");
                let clearing = step.content.as_deref().is_none_or(str::is_empty);
                match (zh, clearing) {
                    (true, true) => format!("已清除 {path}"),
                    (true, false) => format!("已写入 {path}"),
                    (false, true) => format!("cleared {path}"),
                    (false, false) => format!("wrote {path}"),
                }
            }
            _ => step.label.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{EnvAction, Language, PlanStep};

    #[test]
    fn parses_common_locale_shapes() {
        assert_eq!(Locale::parse("zh-CN"), Locale::Zh);
        assert_eq!(Locale::parse("zh_CN.UTF-8"), Locale::Zh);
        assert_eq!(Locale::parse("en-US"), Locale::En);
        assert_eq!(Locale::parse("C"), Locale::En);
        assert_eq!(Locale::from_code("fr"), None);
        assert_eq!(Locale::from_code("auto"), None);
    }

    #[test]
    fn falls_back_to_english() {
        assert_eq!(Locale::En.tr("hello", "你好"), "hello");
        assert_eq!(Locale::Zh.tr("hello", "你好"), "你好");
    }

    fn home(name: &str) -> RuntimeHome {
        RuntimeHome::at(
            std::env::temp_dir().join(format!("runex-i18n-{name}-{}", std::process::id())),
        )
    }

    #[test]
    fn saved_locale_round_trips_and_clears() {
        let home = home("save");
        let _ = std::fs::remove_dir_all(home.root());
        assert_eq!(Locale::saved(&home), None);

        let path = Locale::save(&home, Some(Locale::Zh)).expect("save");
        assert_eq!(Locale::saved(&home), Some(Locale::Zh));
        assert!(path.ends_with(CONFIG_FILE));

        Locale::save(&home, None).expect("clear");
        assert_eq!(Locale::saved(&home), None);
        let _ = std::fs::remove_dir_all(home.root());
    }

    #[test]
    fn plan_labels_are_localized_and_never_leak_a_shell_command() {
        let mut plan = EnvPlan {
            action: EnvAction::Install,
            language: Language::Node,
            tool: "node".into(),
            version: Some("23".into()),
            managed: true,
            steps: vec![PlanStep {
                kind: "install".into(),
                label: "download node 23 from nodejs.org/dist into ~/.runex/runtimes/node".into(),
                command: "lan env install node 23 --yes".into(),
                path: None,
                content: None,
            }],
            error: None,
            notes: Vec::new(),
        };
        let home = home("labels");
        localize_plan(&mut plan, Some(&home), Locale::Zh);
        let label = &plan.steps[0].label;
        assert!(
            label.starts_with("从 nodejs.org/dist 下载 node 23 到 "),
            "{label}"
        );
        assert!(!label.contains("lan env"), "{label}");
        assert!(!label.contains("runex install"), "{label}");
        // `command` 保留给脚本，但 TUI 不再展示。
        assert_eq!(plan.steps[0].command, "lan env install node 23 --yes");
    }

    #[test]
    fn plan_errors_are_localized() {
        let mut plan = EnvPlan {
            action: EnvAction::Install,
            language: Language::Node,
            tool: "node".into(),
            version: None,
            managed: true,
            steps: Vec::new(),
            error: Some("install requires a version, e.g. `lan env install node 20`".into()),
            notes: Vec::new(),
        };
        localize_plan(&mut plan, None, Locale::Zh);
        assert_eq!(
            plan.error.as_deref(),
            Some("安装 node 需要版本号，例如 `node 20`")
        );
    }

    #[test]
    fn diagnostics_are_localized_without_losing_paths() {
        let cases = [
            (
                "/home/u/.runex/shims does not exist yet; it is created on the first `runex install`",
                "/home/u/.runex/shims 尚不存在；第一次安装运行时时会创建",
            ),
            (
                "/home/u/.runex/shims is not prepended to this process PATH; commands may use system runtimes instead. This indicates a lania bug — please report it",
                "/home/u/.runex/shims 未加入当前进程的 PATH；命令可能回退到系统运行时。这通常是 lania 的 bug，请反馈",
            ),
            (
                "/home/u/.runex/shims is not in your shell profile; run `runex init` and append the snippet so external shells also use managed runtimes",
                "/home/u/.runex/shims 未写入 shell 配置；把 init 片段追加进去，外部 shell 才会使用自管运行时",
            ),
            (
                "node points at 22.1.0 but /home/u/.runex/runtimes/node/22.1.0 is missing",
                "node 指向 22.1.0，但 /home/u/.runex/runtimes/node/22.1.0 不存在",
            ),
            (
                "missing shim for `node` (expected in /home/u/.runex/shims)",
                "缺少 `node` 的 shim（应在 /home/u/.runex/shims）",
            ),
        ];
        for (english, chinese) in cases {
            assert_eq!(
                localize_diagnostic(english, Locale::Zh),
                chinese,
                "{english}"
            );
            assert_eq!(localize_diagnostic(english, Locale::En), english);
        }
    }
}
