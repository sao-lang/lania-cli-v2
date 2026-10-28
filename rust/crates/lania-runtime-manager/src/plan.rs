//! 把用户意图（安装 / 切换 / 删除）翻译成**自管运行时**的可执行计划。
//!
//! 这里不再选择任何外部版本管理器：`lan env` 只有一条路径，
//! 由 [`crate::RuntimeStore`] 自己下载、校验、解压、登记。
//!
//! `EnvPlan` 仍然保留「先给计划、再执行」的语义（CLI 默认 dry-run、TUI 先弹确认框），
//! 只是步骤内容从「调用 nvm/volta」变成「lania 自己的动作」。

use serde::Serialize;

use crate::{Language, RuntimeHome, RuntimeStore};

/// 用户可执行的环境操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvAction {
    Install,
    Use,
    Remove,
}

impl EnvAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Use => "use",
            Self::Remove => "remove",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "install" | "i" | "add" => Some(Self::Install),
            "use" | "switch" | "set" | "default" => Some(Self::Use),
            "remove" | "rm" | "delete" | "uninstall" => Some(Self::Remove),
            _ => None,
        }
    }
}

/// 计划中的单个执行步骤。
///
/// `kind` 决定 CLI/TUI 如何执行：
/// - `install` / `use` / `remove`：调用 Store 的对应方法（lania 内部动作）；
/// - `write-file`：写入/清除项目版本文件（不经过 Store）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    /// `install` | `use` | `remove` | `write-file`
    pub kind: String,
    /// 人类可读的步骤说明（同时是 dry-run 的主输出）。
    pub label: String,
    /// 等价的 CLI 文本，便于复制到脚本里复现。
    pub command: String,
    /// `write-file` 专用：目标路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// `write-file` 专用：写入内容（空字符串表示删除文件）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl PlanStep {
    fn new(kind: &str, label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            label: label.into(),
            command: command.into(),
            path: None,
            content: None,
        }
    }

    fn write_file(path: &str, content: &str, label: impl Into<String>) -> Self {
        Self {
            kind: "write-file".into(),
            label: label.into(),
            command: format!("write {path} <- {content}"),
            path: Some(path.to_string()),
            content: Some(content.to_string()),
        }
    }
}

/// 版本号白名单：只允许字母、数字与 `. - _ + @ /`。
///
/// 版本号会进入下载 URL 与版本目录名，入口处统一校验，
/// 避免把 `../` 之类的路径片段带进安装目录。
pub fn validate_version(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 || trimmed.contains("..") {
        return None;
    }
    let valid = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '@' | '/'));
    valid.then_some(trimmed)
}

/// 一次环境操作的完整计划。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvPlan {
    pub action: EnvAction,
    pub language: Language,
    pub tool: String,
    pub version: Option<String>,
    /// 是否由 lania 自己管理（第一批语言为 true）。
    pub managed: bool,
    pub steps: Vec<PlanStep>,
    /// 无法生成计划时的原因。
    pub error: Option<String>,
    /// 提醒用户注意的附加说明。
    pub notes: Vec<String>,
}

impl EnvPlan {
    pub fn is_runnable(&self) -> bool {
        self.error.is_none() && !self.steps.is_empty()
    }

    fn failed(action: EnvAction, tool: &str, version: Option<&str>, error: String) -> Self {
        Self {
            action,
            language: Language::Node,
            tool: tool.to_string(),
            version: version.map(ToOwned::to_owned),
            managed: false,
            steps: Vec::new(),
            error: Some(error),
            notes: Vec::new(),
        }
    }
}

/// 构建环境操作计划。
///
/// `cwd` 只用于项目级 pin 的路径；`home` 仅用于说明文本里的真实路径，
/// 不存在的目录不会在这里创建（真正落盘发生在执行阶段）。
pub fn build_plan(
    action: EnvAction,
    tool: &str,
    version: Option<&str>,
    cwd: &std::path::Path,
    project_scope: bool,
) -> EnvPlan {
    let Some(language) = Language::parse_id(tool) else {
        return EnvPlan::failed(
            action,
            tool,
            version,
            format!(
                "unsupported tool `{tool}`; supported: {}",
                crate::supported_language_ids().join(", ")
            ),
        );
    };

    let language_id = language.id();
    let managed = crate::supported_language_ids().contains(&language_id);
    let mut notes = Vec::new();
    let mut steps = Vec::new();
    let mut error = None;

    if let Some(version) = version {
        if validate_version(version).is_none() {
            error = Some(format!(
                "invalid version `{version}`: only letters, digits and . - _ + @ / are allowed"
            ));
        }
    }

    if error.is_none() && !managed {
        error = Some(format!(
            "{language_id} is not self-managed yet; available now: {}",
            crate::supported_language_ids().join(", ")
        ));
    }

    if error.is_none() {
        match action {
            EnvAction::Install => match version {
                None => {
                    error = Some(format!(
                        "install requires a version, e.g. `lan env install {language_id} 20`"
                    ))
                }
                Some(version) => {
                    let home = RuntimeHome::resolve()
                        .map(|home| home.display_for_shell(&home.runtimes_dir()))
                        .unwrap_or_else(|_| "~/.runex/runtimes".to_string());
                    steps.push(PlanStep::new(
                        "install",
                        format!(
                            "download {language_id} {version} from {} into {home}/{language_id}",
                            crate::provider::upstream_for(language_id)
                        ),
                        format!("lan env install {language_id} {version} --yes"),
                    ));
                }
            },
            EnvAction::Use => match version {
                Some(_) => steps.push(PlanStep::new(
                    "use",
                    format!("point {language_id} at the requested version and regenerate shims"),
                    format!(
                        "lan env use {language_id} {} --yes",
                        version.unwrap_or_default()
                    ),
                )),
                None if project_scope => {}
                None => {
                    error = Some(
                        "use requires a version (or --project to clear the project pin)".into(),
                    )
                }
            },
            EnvAction::Remove => match version {
                Some(version) => steps.push(PlanStep::new(
                    "remove",
                    format!("delete lania-managed {language_id} {version}"),
                    format!("lan env remove {language_id} {version} --yes"),
                )),
                None => error = Some("remove requires a version".into()),
            },
        }
    }

    if error.is_none() && project_scope {
        match language.version_files().first().copied() {
            Some(file) => {
                let path = cwd.join(file).display().to_string();
                match version {
                    Some(version) => steps.push(PlanStep::write_file(
                        &path,
                        version,
                        format!("pin {file} to {version}"),
                    )),
                    None => steps.push(PlanStep::write_file(
                        &path,
                        "",
                        format!("clear {file} project pin"),
                    )),
                }
                notes.push("项目级 pin 只影响当前目录及其子目录".into());
            }
            None => error = Some(format!("{language_id} has no project version file")),
        }
    }

    if error.is_none() && steps.is_empty() {
        error = Some(format!(
            "no {} steps could be generated for {language_id}",
            action.as_str()
        ));
    }

    EnvPlan {
        action,
        language,
        tool: language_id.to_string(),
        version: version.map(ToOwned::to_owned),
        managed,
        steps,
        error,
        notes,
    }
}

/// 执行一个计划（dry-run 由调用方决定是否调用）。
///
/// 返回每个步骤的执行结果文本，供 CLI/TUI 展示。
pub fn execute_plan(
    store: &RuntimeStore,
    plan: &EnvPlan,
    mut on_event: impl FnMut(&PlanStep),
) -> Result<Vec<String>, crate::RuntimeError> {
    if let Some(error) = plan.error.as_deref() {
        return Err(crate::RuntimeError::other(error));
    }
    let mut results = Vec::new();
    for step in &plan.steps {
        on_event(step);
        match step.kind.as_str() {
            "install" => {
                let version = plan.version.clone().unwrap_or_default();
                let outcome = store.install(&plan.tool, &version, &mut |_| {})?;
                results.push(format!(
                    "{} {} installed at {}",
                    outcome.language,
                    outcome.version,
                    outcome.path.display()
                ));
            }
            "use" => {
                let version = plan.version.clone().unwrap_or_default();
                let path = store.use_version(&plan.tool, &version)?;
                results.push(format!("{} → {}", plan.tool, path.display()));
            }
            "remove" => {
                let version = plan.version.clone().unwrap_or_default();
                let path = store.remove(&plan.tool, &version)?;
                results.push(format!("removed {}", path.display()));
            }
            "write-file" => {
                let path = step.path.clone().unwrap_or_default();
                let content = step.content.clone().unwrap_or_default();
                if content.is_empty() {
                    let _ = std::fs::remove_file(&path);
                } else {
                    if let Some(parent) = std::path::Path::new(&path).parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::write(&path, format!("{content}\n"))?;
                }
                results.push(step.label.clone());
            }
            other => {
                return Err(crate::RuntimeError::other(format!(
                    "unknown plan step `{other}`"
                )))
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> std::path::PathBuf {
        std::path::PathBuf::from(".")
    }

    #[test]
    fn install_plan_is_self_managed() {
        let plan = build_plan(EnvAction::Install, "node", Some("20"), &cwd(), false);
        assert!(plan.error.is_none(), "{:?}", plan.error);
        assert!(plan.managed);
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].kind, "install");
        assert!(plan.steps[0].label.contains("nodejs.org/dist"));
    }

    #[test]
    fn unmanaged_language_reports_available_list() {
        let plan = build_plan(EnvAction::Install, "ruby", Some("3.3.0"), &cwd(), false);
        let error = plan.error.expect("ruby is not implemented yet");
        assert!(error.contains("not self-managed"), "{error}");
        assert!(error.contains("node"));
    }

    #[test]
    fn unknown_tool_is_rejected() {
        let plan = build_plan(
            EnvAction::Install,
            "not-a-language",
            Some("1"),
            &cwd(),
            false,
        );
        assert!(plan.error.is_some());
    }

    #[test]
    fn install_without_version_is_rejected() {
        let plan = build_plan(EnvAction::Install, "go", None, &cwd(), false);
        assert!(plan.error.unwrap().contains("requires a version"));
    }

    #[test]
    fn path_traversal_versions_are_rejected() {
        assert!(validate_version("20").is_some());
        assert!(validate_version("3.12.4").is_some());
        assert!(validate_version("../../etc").is_none());
        assert!(validate_version("20;whoami").is_none());
    }

    #[test]
    fn project_use_writes_version_file() {
        let plan = build_plan(EnvAction::Use, "node", Some("20"), &cwd(), true);
        assert!(plan.error.is_none(), "{:?}", plan.error);
        let kinds: Vec<&str> = plan.steps.iter().map(|step| step.kind.as_str()).collect();
        assert_eq!(kinds, vec!["use", "write-file"]);
        assert!(plan.steps[1]
            .path
            .as_deref()
            .is_some_and(|path| path.ends_with(".nvmrc")));
    }

    #[test]
    fn use_and_remove_require_version() {
        assert!(build_plan(EnvAction::Use, "node", None, &cwd(), false)
            .error
            .is_some());
        assert!(build_plan(EnvAction::Remove, "node", None, &cwd(), false)
            .error
            .is_some());
    }
}
