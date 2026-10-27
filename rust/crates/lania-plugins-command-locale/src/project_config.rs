//! 项目配置（`lan.config.ts`）的读写辅助，供 `lan config init/doctor/set/sync` 使用。
//!
//! 设计约束来自 `docs/design/multi-runtime-bridge.zh-CN.md` §6.5：
//!
//! 1. **`.ts` 是官方格式**，但加载器仍认 7 个候选（§6.5.4）。这里只**写入** `.ts`。
//! 2. 配置允许写函数 / import / hooks，因此**不能盲目程序化改写**（§6.5.8）：
//!    只有「纯数据配置」才允许 `set` / `sync` 改写，其它情况只打印建议片段。
//! 3. 判定「纯数据」= 源文件不含 `import` / `require(` / `=>` / `function`。
//!    （§6.5.8 的第二半“可 JSON 无损往返”由调用方用快照的 `raw` 保证。）
//!
//! 为什么不引入 TS/JS 解析器：写回时我们**不保留原格式**，而是按原模块风格
//! （ESM / CJS）把求值后的 `raw` 重新序列化。对纯数据配置这是等价的，
//! 而真正需要保留函数体的场景本来就被规则挡在写入之外。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};

/// 写入时使用的模块风格（决定 `export default` 还是 `module.exports`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleStyle {
    /// `export default {...}`（`.ts` / `.mts` / `.mjs`，以及 ESM 的 `.js`）
    Esm,
    /// `module.exports = {...}`（CJS 的 `.js` / `.cjs`）
    CommonJs,
    /// `{...}`（`.json`：纯数据，不能写注释与 export）
    Json,
}

/// 默认写入位置（官方格式）。
pub const DEFAULT_CONFIG_FILE: &str = "lan.config.ts";

/// 配置文件的候选扩展名（与 bridge 的 `LAN_CONFIG_SEARCH_PLACES` 同一顺序）。
///
/// 只用于**探测已存在的配置文件**（`init` 判断要不要覆盖、`doctor` 报告现状），
/// 权威候选列表仍在 `node-bridge/src/core/runtime.ts`（§6.5.3：Rust 不维护第二份）。
/// 这里的顺序只是「用户可能手写了哪个」，不需与 TS 完全一致；真正加载仍走 bridge。
pub const KNOWN_CONFIG_FILES: &[&str] = &[
    "lan.config.ts",
    "lan.config.mts",
    "lan.config.cts",
    "lan.config.mjs",
    "lan.config.js",
    "lan.config.cjs",
    "lan.config.json",
];

/// 让配置「不再可安全改写」的标记。
const IMPURE_MARKERS: &[&str] = &["import ", "require(", "=>", "function"];

/// 是否是纯数据配置（可安全程序化改写）。
pub fn is_pure_data(source: &str) -> bool {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return true;
    }
    !IMPURE_MARKERS.iter().any(|marker| trimmed.contains(marker))
}

/// 按扩展名 + 源码推断模块风格。
pub fn module_style(path: &Path, source: &str) -> ModuleStyle {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => ModuleStyle::Json,
        Some("ts") | Some("mts") | Some("mjs") => ModuleStyle::Esm,
        Some("cts") | Some("cjs") => ModuleStyle::CommonJs,
        // `.js` 要看源码：`module.exports` 说明是 CJS（package.json 的 type 可能缺失）。
        Some("js") => {
            if source.contains("module.exports") {
                ModuleStyle::CommonJs
            } else {
                ModuleStyle::Esm
            }
        }
        // 没扩展名（`init` 新建前）默认 ESM。
        _ => ModuleStyle::Esm,
    }
}

/// 把配置对象渲染成源码文本。
///
/// `header` 是写入的注释头；`style = Json` 时不写注释（JSON 不支持）。
pub fn render_config(style: ModuleStyle, config: &Value, header: &[&str]) -> String {
    match style {
        ModuleStyle::Json => format!("{}\n", pretty_json(config)),
        ModuleStyle::Esm => {
            let mut out = String::new();
            push_header(&mut out, header);
            out.push_str("export default ");
            out.push_str(&pretty_json(config));
            out.push_str(";\n");
            out
        }
        ModuleStyle::CommonJs => {
            let mut out = String::new();
            push_header(&mut out, header);
            out.push_str("module.exports = ");
            out.push_str(&pretty_json(config));
            out.push_str(";\n");
            out
        }
    }
}

fn push_header(out: &mut String, header: &[&str]) {
    for line in header {
        if line.is_empty() {
            out.push_str("//\n");
        } else {
            out.push_str("// ");
            out.push_str(line);
            out.push('\n');
        }
    }
    if !header.is_empty() {
        out.push('\n');
    }
}

fn pretty_json(value: &Value) -> String {
    // 统一 2 空格缩进 + 尾部无空格；不用 `serde_json::to_string_pretty` 的默认 2 空格也一样，
    // 但这里显式写出，方便以后换格式化策略。
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
}

/// 在配置对象上按点路径写入（`runtime` / `runtimes.go.build` / `lintTools`）。
///
/// 只允许已登记的顶层字段（`lania_config::known_config_fields`）：
/// 拼错的 key 在这里就被挡住，而不是写进文件后由 validate 报 UnknownField。
pub fn set_dotted(config: &mut Value, path: &str, value: Value) -> Result<()> {
    let segments: Vec<&str> = path.split('.').filter(|part| !part.is_empty()).collect();
    if segments.is_empty() {
        return Err(anyhow!("empty config path"));
    }
    let top = segments[0];
    if !lania_config::known_config_fields().contains(&top) {
        return Err(anyhow!(
            "unknown lan.config field `{top}`; known fields: {}",
            lania_config::known_config_fields().join(", ")
        ));
    }
    let object = config
        .as_object_mut()
        .ok_or_else(|| anyhow!("lan.config root must be an object"))?;
    set_in_object(object, &segments, value)
}

fn set_in_object(object: &mut Map<String, Value>, segments: &[&str], value: Value) -> Result<()> {
    let (head, tail) = segments
        .split_first()
        .ok_or_else(|| anyhow!("empty config path"))?;
    if tail.is_empty() {
        object.insert((*head).to_string(), value);
        return Ok(());
    }
    let entry = object
        .entry((*head).to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    let nested = entry
        .as_object_mut()
        .ok_or_else(|| anyhow!("`{head}` is not an object"))?;
    set_in_object(nested, tail, value)
}

/// 在 `cwd` 下探测已存在的配置文件（按 `KNOWN_CONFIG_FILES` 顺序）。
pub fn find_existing_config(cwd: &Path) -> Option<PathBuf> {
    KNOWN_CONFIG_FILES
        .iter()
        .map(|name| cwd.join(name))
        .find(|path| path.is_file())
}

/// `lan config init` 写入的目标路径。
pub fn init_target(cwd: &Path) -> PathBuf {
    cwd.join(DEFAULT_CONFIG_FILE)
}

/// 项目里能否 `import { defineConfig } from 'lania'`（决定 `init` 要不要写类型化模板）。
///
/// 新项目通常没装 CLI 包；直接写 import 会让 `lan.config.ts` 加载失败（§6.5.9 的
/// 「加载失败必须可见」会把它暴露成 warning，但用户第一步就踩坑没必要）。
pub fn define_config_available(cwd: &Path) -> bool {
    if cwd.join("node_modules/lania/package.json").is_file() {
        return true;
    }
    let package_json = cwd.join("package.json");
    let Ok(contents) = std::fs::read_to_string(package_json) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|section| {
            parsed
                .get(*section)
                .is_some_and(|deps| deps.get("lania").is_some())
        })
}

/// 默认 lint 工具（非 JS 项目写成显式 `lintTools`，JS 项目交给默认值）。
pub fn default_lint_tools(kind: lania_host::RuntimeKind) -> Vec<String> {
    vec![lania_host::default_lint_tool(kind).to_string()]
}

/// `lan.config` 的写盘入口（统一 err 文案）。
pub fn write_config(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| anyhow!("failed to create {}: {error}", parent.display()))?;
    }
    std::fs::write(path, contents)
        .map_err(|error| anyhow!("failed to write {}: {error}", path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 命令计划（纯函数：输入快照 + cwd，输出「写什么 / 建议什么」）
//
// 把计划与 I/O 分开，是为了让 `init` / `set` / `sync` / `doctor` 的核心行为
// 可以不起进程、不写盘就单测（handler 只负责调用 + 落盘 + 拼输出 JSON）。
// ---------------------------------------------------------------------------

use lania_config::LanConfigSnapshot;
use lania_host::RuntimeKind;

/// 配置写入时统一的注释头（`init` / `set` / `sync` 共用，产物风格一致）。
const CONFIG_HEADER: &[&str] = &[
    "lan.config.ts —— Lania CLI 项目配置",
    "文档：docs/design/multi-runtime-bridge.zh-CN.md §6.5",
    "支持导出函数（按环境分支）、hooks、plugins；`lan config doctor` 可做只读诊断。",
];

/// 项目配置视图：cwd + 已加载的配置快照（可能不存在）。
#[derive(Debug, Clone, Copy)]
pub struct ProjectView<'a> {
    pub cwd: &'a Path,
    pub snapshot: Option<&'a LanConfigSnapshot>,
}

impl<'a> ProjectView<'a> {
    pub fn new(cwd: &'a Path, snapshot: Option<&'a LanConfigSnapshot>) -> Self {
        Self { cwd, snapshot }
    }

    /// 已存在的配置文件路径：优先用 bridge 实际命中的那个，其次自己探测。
    pub fn existing_path(&self) -> Option<PathBuf> {
        self.snapshot
            .and_then(|snapshot| snapshot.config_path.as_deref())
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| find_existing_config(self.cwd))
    }

    /// 配置源码（不存在时为空串）。
    pub fn source(&self) -> String {
        self.existing_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default()
    }

    /// 已求值的配置对象（不存在时 `{}`）。
    pub fn raw(&self) -> Value {
        self.snapshot
            .map(|snapshot| snapshot.raw.clone())
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Map::new()))
    }

    /// 纯数据配置才允许程序化改写（§6.5.8）。
    pub fn is_pure_data(&self) -> bool {
        is_pure_data(&self.source())
    }

    /// 配置里显式声明的运行时。
    pub fn declared_runtime(&self) -> Option<String> {
        self.snapshot.and_then(|snapshot| snapshot.runtime.clone())
    }

    /// 按特征文件判定的运行时（忽略配置声明）。
    pub fn detected_runtime(&self) -> RuntimeKind {
        lania_host::detect_project_runtime(self.cwd)
    }

    /// 最终生效的运行时（配置声明 > 特征文件 > node）。
    pub fn effective_runtime(&self) -> (RuntimeKind, &'static str) {
        let detected = self.detected_runtime();
        match self
            .declared_runtime()
            .as_deref()
            .and_then(RuntimeKind::parse)
        {
            Some(declared) => (declared, "config"),
            None if detected != RuntimeKind::Node => (detected, "feature-files"),
            None => (RuntimeKind::Node, "fallback"),
        }
    }

    pub fn lint_tools(&self) -> Vec<String> {
        self.snapshot
            .map(|snapshot| snapshot.lint_tools.clone())
            .unwrap_or_default()
    }
}

/// `lan config init` 的计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitPlan {
    pub path: PathBuf,
    pub existing: Vec<PathBuf>,
    pub written: bool,
    /// `typed`（用 `defineConfig`） / `plain`（普通对象）
    pub mode: &'static str,
    pub contents: String,
    pub runtime: RuntimeKind,
}

/// 规划 `init`：`--force` 才覆盖已有配置（不删其它候选文件，只在报告里提示）。
pub fn plan_init(view: &ProjectView<'_>, force: bool) -> InitPlan {
    let (runtime, _) = view.effective_runtime();
    let existing: Vec<PathBuf> = KNOWN_CONFIG_FILES
        .iter()
        .map(|name| view.cwd.join(name))
        .filter(|path| path.is_file())
        .collect();
    let path = init_target(view.cwd);
    let mut config = Map::new();
    if runtime != RuntimeKind::Node {
        // node 是默认值，没必要写出来；非 JS 项目必须显式声明。
        config.insert("runtime".into(), Value::String(runtime.id().to_string()));
    }
    config.insert(
        "lintTools".into(),
        Value::Array(
            default_lint_tools(runtime)
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    let config = Value::Object(config);
    let typed = define_config_available(view.cwd);
    let contents = if typed {
        render_typed_config(&config)
    } else {
        render_config(ModuleStyle::Esm, &config, CONFIG_HEADER)
    };
    let blocked = !force && !existing.is_empty();
    InitPlan {
        path,
        existing,
        written: !blocked,
        mode: if typed { "typed" } else { "plain" },
        contents,
        runtime,
    }
}

/// 带 `defineConfig` 的模板（项目能解析到 `lania` 包时才用）。
fn render_typed_config(config: &Value) -> String {
    let mut out = String::new();
    push_header(&mut out, CONFIG_HEADER);
    out.push_str("import { defineConfig } from 'lania';\n\nexport default defineConfig(");
    out.push_str(&pretty_json(config));
    out.push_str(");\n");
    out
}

/// `lan config sync` 的一条建议。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub key: String,
    pub value: Value,
    pub reason: String,
}

/// `lan config sync` 的计划：`patched` 是「补齐后」的完整配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    pub path: PathBuf,
    pub suggestions: Vec<Suggestion>,
    pub patched: Value,
    pub snippet: String,
    /// 纯数据配置才建议真的写盘（否则只能手动粘贴）。
    pub writable: bool,
}

/// 规划 `sync`：把「配置里没说、但项目实际上需要的」补出来。
pub fn plan_sync(view: &ProjectView<'_>) -> SyncPlan {
    let mut patched = view.raw();
    let mut suggestions = Vec::new();
    let (runtime, _) = view.effective_runtime();

    if view.declared_runtime().is_none() && runtime != RuntimeKind::Node {
        suggestions.push(Suggestion {
            key: "runtime".into(),
            value: Value::String(runtime.id().to_string()),
            reason: format!(
                "检测到 {} 项目特征文件，但配置里没有显式 runtime",
                runtime.id()
            ),
        });
    }
    if view.lint_tools().is_empty() {
        suggestions.push(Suggestion {
            key: "lintTools".into(),
            value: Value::Array(
                default_lint_tools(runtime)
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
            reason: format!(
                "没有显式 lintTools，`lan lint` 会使用 {} 的默认工具",
                runtime.id()
            ),
        });
    }
    for suggestion in &suggestions {
        // 这里不会失败：key 都是已知顶层字段（`set_dotted` 会校验）。
        let _ = set_dotted(&mut patched, &suggestion.key, suggestion.value.clone());
    }

    let path = view
        .existing_path()
        .unwrap_or_else(|| init_target(view.cwd));
    let style = module_style(&path, &view.source());
    let snippet = render_config(style, &patched, CONFIG_HEADER);
    SyncPlan {
        path,
        suggestions,
        patched,
        snippet,
        writable: view.is_pure_data(),
    }
}

/// `lan config set`（项目配置部分）的计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetPlan {
    pub path: PathBuf,
    pub patched: Value,
    pub contents: String,
    /// 纯数据配置才写盘；否则只给建议。
    pub writable: bool,
}

/// 规划 `set`：`--force` 不会绕过「非纯数据不写入」这条硬约束。
pub fn plan_set(view: &ProjectView<'_>, key: &str, value: Value) -> Result<SetPlan> {
    let mut patched = view.raw();
    set_dotted(&mut patched, key, value)?;
    let path = view
        .existing_path()
        .unwrap_or_else(|| init_target(view.cwd));
    let style = module_style(&path, &view.source());
    let contents = render_config(style, &patched, CONFIG_HEADER);
    Ok(SetPlan {
        path,
        patched,
        contents,
        writable: view.is_pure_data(),
    })
}

/// `lan config doctor` 的问题项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorIssue {
    pub code: &'static str,
    pub severity: &'static str,
    pub message: String,
}

/// 规划 `doctor`：只读诊断，返回 `(报告, issues)`。
pub fn doctor_report(view: &ProjectView<'_>) -> (Value, Vec<DoctorIssue>) {
    let (runtime, source) = view.effective_runtime();
    let declared = view.declared_runtime();
    let detected = view.detected_runtime();
    let existing = view.existing_path();
    let pure = view.is_pure_data();
    let lint_tools = view.lint_tools();

    let mut issues = Vec::new();
    if existing.is_none() {
        issues.push(DoctorIssue {
            code: "config_missing",
            severity: "warning",
            message: "no lan.config found; `lan config init` can create one".into(),
        });
    }
    if let Some(declared) = declared.as_deref() {
        if RuntimeKind::parse(declared).is_some_and(|declared| declared != detected) {
            issues.push(DoctorIssue {
                code: "runtime_mismatch",
                severity: "warning",
                message: format!(
                    "config declares runtime `{declared}` but feature files suggest `{}`; \
                     the config value wins",
                    detected.id()
                ),
            });
        }
    }
    if existing.is_some() && !pure {
        issues.push(DoctorIssue {
            code: "config_not_pure_data",
            severity: "info",
            message: "config uses functions/imports; `lan config set` / `sync --write` will only \
                      print suggestions"
                .into(),
        });
    }
    if lint_tools.is_empty() {
        issues.push(DoctorIssue {
            code: "lint_tools_missing",
            severity: "info",
            message: format!(
                "no explicit lintTools; `lan lint` falls back to `{}`",
                lania_host::default_lint_tool(runtime)
            ),
        });
    }
    let validation_errors: Vec<Value> = view
        .snapshot
        .map(|snapshot| {
            snapshot
                .validation_errors
                .iter()
                .map(|error| {
                    json!({
                        "code": format!("{:?}", error.code),
                        "path": error.position.path,
                        "message": error.message,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    for error in &validation_errors {
        issues.push(DoctorIssue {
            code: "config_validation_error",
            severity: "error",
            message: format!(
                "{}: {}",
                error["path"].as_str().unwrap_or("$"),
                error["message"].as_str().unwrap_or("invalid value")
            ),
        });
    }

    let report = json!({
        "kind": "config_doctor",
        "scope": "project",
        "cwd": view.cwd.display().to_string(),
        "configPath": existing.as_ref().map(|path| path.display().to_string()),
        "exists": existing.is_some(),
        "supportedExtensions": view
            .snapshot
            .map(|snapshot| snapshot.supported_extensions.clone())
            .unwrap_or_default(),
        "runtime": {
            "declared": declared,
            "detected": detected.id(),
            "effective": runtime.id(),
            "source": source,
        },
        "runtimes": view
            .snapshot
            .map(|snapshot| serde_json::to_value(&snapshot.runtimes).unwrap_or(Value::Null))
            .unwrap_or(Value::Null),
        "lintTools": lint_tools,
        "pureData": pure,
        // 让用户/脚本知道哪些子命令能真的写入。
        "writable": { "set": pure, "syncWrite": pure },
        "validationErrors": validation_errors,
        "issues": issues
            .iter()
            .map(|issue| json!({
                "code": issue.code,
                "severity": issue.severity,
                "message": issue.message,
            }))
            .collect::<Vec<_>>(),
    });
    (report, issues)
}

/// 把 CLI 传入的字符串转成配置值。
///
/// 规则（刻意简单，文档里可预测）：
/// - `lintTools` 这类列表字段：逗号分隔（`eslint,prettier`）；
/// - 其它字段：先试 JSON（`true` / `1` / `["go","build"]`），失败则当普通字符串。
pub fn parse_value(key: &str, raw: &str) -> Value {
    let trimmed = raw.trim();
    if key == "lintTools" || key.ends_with(".lintTools") {
        return Value::Array(
            trimmed
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| Value::String(item.to_string()))
                .collect(),
        );
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) if !value.is_string() => value,
        _ => Value::String(trimmed.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pure_data_detection_matches_the_documented_markers() {
        assert!(is_pure_data("export default { runtime: 'go' };\n"));
        assert!(is_pure_data("module.exports = { runtime: 'go' };\n"));
        assert!(is_pure_data(""));
        // 下面这些一旦改写就会丢逻辑 ⇒ 不是纯数据。
        assert!(!is_pure_data("import { defineConfig } from 'lania';\n"));
        assert!(!is_pure_data("export default () => ({ runtime: 'go' });"));
        assert!(!is_pure_data(
            "export default { hooks: { onBuild: (ctx) => ctx } };"
        ));
        assert!(!is_pure_data("function build() {}\n"));
        assert!(!is_pure_data("module.exports = require('./base');"));
    }

    #[test]
    fn module_style_follows_extension_and_source() {
        assert_eq!(
            module_style(Path::new("lan.config.ts"), "export default {}"),
            ModuleStyle::Esm
        );
        assert_eq!(
            module_style(Path::new("lan.config.json"), "{}"),
            ModuleStyle::Json
        );
        assert_eq!(
            module_style(Path::new("lan.config.cjs"), ""),
            ModuleStyle::CommonJs
        );
        // `.js` 两种形态都可能：看源码里有没有 `module.exports`。
        assert_eq!(
            module_style(Path::new("lan.config.js"), "module.exports = {};"),
            ModuleStyle::CommonJs
        );
        assert_eq!(
            module_style(Path::new("lan.config.js"), "export default {};"),
            ModuleStyle::Esm
        );
    }

    #[test]
    fn render_config_keeps_the_module_style() {
        let config = json!({ "runtime": "go", "lintTools": ["go-vet"] });
        let esm = render_config(ModuleStyle::Esm, &config, &["lan.config.ts"]);
        assert!(esm.starts_with("// lan.config.ts\n"));
        assert!(esm.contains("export default {"));
        assert!(esm.contains("\"runtime\": \"go\""));
        assert!(esm.ends_with(";\n"));

        let cjs = render_config(ModuleStyle::CommonJs, &config, &[]);
        assert!(cjs.starts_with("module.exports = {"));
        assert!(!cjs.contains("//"), "{cjs}");

        let json_style = render_config(ModuleStyle::Json, &config, &["ignored"]);
        assert!(!json_style.contains("//"), "{json_style}");
        assert!(!json_style.contains("export"), "{json_style}");
        assert_eq!(
            serde_json::from_str::<Value>(&json_style).expect("valid json"),
            config
        );
    }

    #[test]
    fn set_dotted_writes_nested_paths_and_validates_top_level() {
        let mut config = json!({ "runtime": "go" });
        set_dotted(&mut config, "lintTools", json!(["go-vet"])).expect("set lintTools");
        set_dotted(
            &mut config,
            "runtimes.go.build",
            json!(["go", "build", "-o", "bin/app", "."]),
        )
        .expect("set nested");
        set_dotted(&mut config, "runtimes.go.dev", json!(["go", "run", "."]))
            .expect("set second nested");
        assert_eq!(config["lintTools"], json!(["go-vet"]));
        assert_eq!(config["runtimes"]["go"]["build"][1], "build");
        assert_eq!(config["runtimes"]["go"]["dev"], json!(["go", "run", "."]));

        let error = set_dotted(&mut config, "buildtool", json!("x")).expect_err("typo rejected");
        assert!(error.to_string().contains("buildtool"), "{error}");
        assert!(set_dotted(&mut config, "", json!(1)).is_err());
    }

    #[test]
    fn set_dotted_replaces_non_object_intermediate() {
        let mut config = json!({ "runtimes": { "go": "not-an-object" } });
        set_dotted(&mut config, "runtimes.go.build", json!(["go", "build"]))
            .expect("replace intermediate");
        assert_eq!(config["runtimes"]["go"]["build"], json!(["go", "build"]));
    }

    #[test]
    fn parse_value_handles_lists_json_and_plain_strings() {
        assert_eq!(
            parse_value("lintTools", "eslint, prettier ,"),
            json!(["eslint", "prettier"])
        );
        assert_eq!(
            parse_value("runtimes.go.build", r#"["go","build","."]"#),
            json!(["go", "build", "."])
        );
        assert_eq!(parse_value("runtime", "go"), json!("go"));
        assert_eq!(parse_value("custom.flag", "true"), json!(true));
        assert_eq!(parse_value("buildTool", "1"), json!(1));
    }

    // ---- 命令计划：init / sync / set / doctor ----

    fn temp_project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "lania-config-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp project");
        for (file, contents) in files {
            let path = root.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(path, contents).expect("seed file");
        }
        root
    }

    fn snapshot_for(cwd: &Path, raw: Value) -> LanConfigSnapshot {
        lania_config::ConfigService::load_lan_snapshot(&json!({
            "cwd": cwd.display().to_string(),
            "exists": true,
            "supportedExtensions": [".ts", ".js"],
            "config": raw,
        }))
        .expect("snapshot")
    }

    #[test]
    fn init_writes_ts_config_with_the_detected_runtime() {
        let root = temp_project("init-go", &[("go.mod", "module demo\n")]);
        let view = ProjectView::new(&root, None);
        let plan = plan_init(&view, false);
        assert!(plan.written);
        assert_eq!(plan.mode, "plain"); // 项目里没有 `lania` 包 ⇒ 不 import
        assert!(plan.path.ends_with("lan.config.ts"));
        assert!(plan.contents.contains("export default {"));
        assert!(plan.contents.contains("\"runtime\": \"go\""));
        assert!(plan
            .contents
            .contains("\"lintTools\": [\n    \"go-vet\"\n  ]"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn init_refuses_to_overwrite_without_force_but_reports_the_old_file() {
        let root = temp_project(
            "init-exists",
            &[
                ("package.json", "{\"name\":\"demo\"}"),
                ("lan.config.js", "module.exports = { runtime: 'node' };\n"),
            ],
        );
        let view = ProjectView::new(&root, None);
        let plan = plan_init(&view, false);
        assert!(!plan.written);
        assert_eq!(plan.existing.len(), 1);
        assert!(plan.existing[0].ends_with("lan.config.js"));

        // `--force` 时会写 `lan.config.ts`，但不会删掉旧的 `.js`。
        let forced = plan_init(&view, true);
        assert!(forced.written);
        assert!(forced.existing[0].ends_with("lan.config.js"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn init_uses_define_config_when_the_package_is_resolvable() {
        let root = temp_project(
            "init-typed",
            &[
                ("node_modules/lania/package.json", "{\"name\":\"lania\"}"),
                ("package.json", "{\"name\":\"demo\"}"),
            ],
        );
        let view = ProjectView::new(&root, None);
        let plan = plan_init(&view, false);
        assert_eq!(plan.mode, "typed");
        assert!(plan
            .contents
            .contains("import { defineConfig } from 'lania';"));
        assert!(plan.contents.contains("export default defineConfig({"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sync_suggests_runtime_and_lint_tools_for_a_bare_project() {
        let root = temp_project("sync", &[("Cargo.toml", "[package]\nname = \"demo\"\n")]);
        let snapshot = snapshot_for(&root, json!({}));
        let view = ProjectView::new(&root, Some(&snapshot));
        let plan = plan_sync(&view);
        assert_eq!(plan.suggestions.len(), 2);
        assert_eq!(plan.suggestions[0].key, "runtime");
        assert_eq!(plan.suggestions[0].value, json!("rust"));
        assert_eq!(plan.suggestions[1].key, "lintTools");
        assert_eq!(plan.suggestions[1].value, json!(["clippy"]));
        assert!(plan.writable);
        assert!(plan.snippet.contains("\"runtime\": \"rust\""));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sync_suggests_nothing_when_the_config_is_complete() {
        let root = temp_project(
            "sync-complete",
            &[
                ("go.mod", "module demo\n"),
                (
                    "lan.config.ts",
                    "export default { runtime: 'go', lintTools: ['go-vet'] };\n",
                ),
            ],
        );
        let snapshot = snapshot_for(&root, json!({ "runtime": "go", "lintTools": ["go-vet"] }));
        let view = ProjectView::new(&root, Some(&snapshot));
        let plan = plan_sync(&view);
        assert!(plan.suggestions.is_empty(), "{:?}", plan.suggestions);
        assert!(plan.snippet.contains("export default"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn set_is_blocked_on_function_configs_but_patches_pure_data() {
        let root = temp_project(
            "set",
            &[("lan.config.ts", "export default { runtime: 'go' };\n")],
        );
        let snapshot = snapshot_for(&root, json!({ "runtime": "go" }));
        let view = ProjectView::new(&root, Some(&snapshot));
        let plan =
            plan_set(&view, "runtimes.go.build", json!(["go", "build", "."])).expect("plan set");
        assert!(plan.writable);
        assert_eq!(plan.patched["runtimes"]["go"]["build"][1], "build");
        assert!(plan.contents.contains("\"build\""));
        let _ = std::fs::remove_dir_all(root);

        // 含函数 ⇒ 不可写入（硬约束，`--force` 也不绕过）。
        let root = temp_project(
            "set-fn",
            &[(
                "lan.config.ts",
                "export default () => ({ runtime: 'go' });\n",
            )],
        );
        let snapshot = snapshot_for(&root, json!({ "runtime": "go" }));
        let view = ProjectView::new(&root, Some(&snapshot));
        let plan = plan_set(&view, "runtime", json!("rust")).expect("plan set");
        assert!(!plan.writable);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn set_rejects_unknown_top_level_fields() {
        let root = temp_project("set-unknown", &[("go.mod", "module demo\n")]);
        let view = ProjectView::new(&root, None);
        let error = plan_set(&view, "buildtool", json!("vite")).expect_err("typo");
        assert!(error.to_string().contains("buildtool"), "{error}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_reports_missing_config_and_runtime_mismatch() {
        let root = temp_project("doctor", &[("go.mod", "module demo\n")]);
        // 没有配置文件。
        let view = ProjectView::new(&root, None);
        let (report, issues) = doctor_report(&view);
        assert_eq!(report["exists"], false);
        assert_eq!(report["runtime"]["detected"], "go");
        assert_eq!(report["runtime"]["effective"], "go");
        assert!(issues.iter().any(|issue| issue.code == "config_missing"));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "lint_tools_missing"));

        // 配置声明 python，但项目是 go ⇒ runtime_mismatch（配置优先）。
        let snapshot = snapshot_for(&root, json!({ "runtime": "python", "lintTools": ["mypy"] }));
        let view = ProjectView::new(&root, Some(&snapshot));
        let (report, issues) = doctor_report(&view);
        assert_eq!(report["runtime"]["declared"], "python");
        assert_eq!(report["runtime"]["effective"], "python");
        assert_eq!(report["runtime"]["source"], "config");
        assert!(issues.iter().any(|issue| issue.code == "runtime_mismatch"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_surfaces_validation_errors_from_the_snapshot() {
        let root = temp_project("doctor-invalid", &[("go.mod", "module demo\n")]);
        // 未知字段 ⇒ validate 会产出 validation_errors。
        let snapshot = snapshot_for(&root, json!({ "buildtool": "vite" }));
        let view = ProjectView::new(&root, Some(&snapshot));
        let (report, issues) = doctor_report(&view);
        assert!(!report["validationErrors"]
            .as_array()
            .expect("validation errors array")
            .is_empty());
        assert!(issues
            .iter()
            .any(|issue| issue.code == "config_validation_error"));
        let _ = std::fs::remove_dir_all(root);
    }
}
