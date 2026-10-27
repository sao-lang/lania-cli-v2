//! 项目运行时判定（`docs/design/multi-runtime-bridge.zh-CN.md` §5.1）。
//!
//! 这是 **Rust 侧唯一的判定入口**：`build` / `lint` / `dev` 都从这里拿运行时，
//! 再决定把请求发给哪个 bridge。TS 侧的同名知识在
//! `ts/packages/node-bridge/src/plugins/toolchain.ts`（供 JS 簇 bridge 与
//! `toolchain.describe` 使用），两边刻意保持同一份优先级顺序。
//!
//! 优先级必须固定，否则同一项目两次运行可能判定不同：
//!
//! ```text
//! 1. CLI 参数 --runtime
//! 2. lan.config.ts 的 runtime 字段
//! 3. 特征文件（下面的 FEATURE_PRIORITY 顺序）
//! 4. 回退 node
//! ```
//!
//! 为什么非 JS 强信号排在 `package.json` 之前：Go / Rust / Python / Dart 项目
//! 常常同时带一个 `package.json`（前端资源、脚本工具链）。若让 `package.json`
//! 先命中，这些项目会被判成 web 项目，`build` 会走错工具链。

use std::path::Path;

use lania_config::LanConfigSnapshot;

use super::bridges::RuntimeKind;

/// 特征文件 → 运行时，**数组顺序即优先级**。
///
/// `deno.json` / `bun.lock` 排在最前：它们是「同一项目里还留着一个 package.json」
/// 时唯一可靠的信号（Deno 项目常同时有 package.json 做 npm: 说明符）。
pub const FEATURE_PRIORITY: &[(&str, RuntimeKind)] = &[
    ("deno.json", RuntimeKind::Deno),
    ("deno.jsonc", RuntimeKind::Deno),
    ("bun.lockb", RuntimeKind::Bun),
    ("bun.lock", RuntimeKind::Bun),
    ("pyproject.toml", RuntimeKind::Python),
    ("setup.py", RuntimeKind::Python),
    ("requirements.txt", RuntimeKind::Python),
    ("requirements-dev.txt", RuntimeKind::Python),
    ("go.mod", RuntimeKind::Go),
    ("Cargo.toml", RuntimeKind::Rust),
    ("pubspec.yaml", RuntimeKind::Dart),
    // node 兜底放最后：`package.json` 几乎总是和别的特征文件共存。
    ("package.json", RuntimeKind::Node),
];

/// 只按特征文件判定（不看配置与 CLI 覆盖）。
pub fn detect_project_runtime(cwd: &Path) -> RuntimeKind {
    for (file, kind) in FEATURE_PRIORITY {
        if cwd.join(file).is_file() {
            return *kind;
        }
    }
    // 单文件脚本项目没有任何 manifest：有 `.dart` / `.py` 文件时仍然给出正确判定，
    // 否则会退回 node 并让 lint 白跑一趟。
    if has_extension(cwd, "dart") {
        return RuntimeKind::Dart;
    }
    if has_extension(cwd, "py") {
        return RuntimeKind::Python;
    }
    RuntimeKind::Node
}

/// 完整判定：CLI 覆盖 > 配置 `runtime` > 特征文件 > node。
pub fn detect_runtime(
    cwd: &Path,
    config: Option<&LanConfigSnapshot>,
    cli_runtime: Option<&str>,
) -> RuntimeKind {
    if let Some(kind) = cli_runtime.and_then(RuntimeKind::parse) {
        return kind;
    }
    if let Some(kind) = config
        .and_then(|config| config.runtime.as_deref())
        .and_then(RuntimeKind::parse)
    {
        return kind;
    }
    detect_project_runtime(cwd)
}

/// 判定结果 + 「为什么这么判」的提示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDecision {
    pub kind: RuntimeKind,
    /// 命中多个运行时信号、且用户没显式指定时才有值（§5.1 验收：判定结果有提示）。
    pub notice: Option<String>,
}

/// 带提示的判定入口：CLI / 配置显式指定时保持安静，只有「多信号」才提醒。
///
/// 为什么只在多信号时提醒：单一信号（只有 `go.mod`）没有歧义，多说一句只是噪声；
/// 而 `deno.json` + `package.json` 这种双运行时项目，用户确实需要知道我们选了哪个。
pub fn decide_runtime(
    cwd: &Path,
    config: Option<&LanConfigSnapshot>,
    cli_runtime: Option<&str>,
) -> RuntimeDecision {
    let kind = detect_runtime(cwd, config, cli_runtime);
    let explicit = cli_runtime.and_then(RuntimeKind::parse).is_some()
        || config
            .and_then(|config| config.runtime.as_deref())
            .and_then(RuntimeKind::parse)
            .is_some();
    if explicit {
        return RuntimeDecision { kind, notice: None };
    }
    let matched: Vec<&str> = FEATURE_PRIORITY
        .iter()
        .filter(|(file, _)| cwd.join(file).is_file())
        .map(|(file, _)| *file)
        .collect();
    let notice = (matched.len() > 1).then(|| {
        format!(
            "detected {}; using `{}` — pass `--runtime <id>` to override",
            matched.join(", "),
            kind.id()
        )
    });
    RuntimeDecision { kind, notice }
}

fn has_extension(cwd: &Path, extension: &str) -> bool {
    std::fs::read_dir(cwd)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == extension))
        })
        .unwrap_or(false)
}

/// 该运行时的**默认 lint 工具**：选「随工具链自带」的那个。
///
/// 用途：`lan config init` 写默认 `lintTools`、`lan lint` 在没配置时决定跑什么。
/// 放在这里（而不是各处 match 一遍）是因为「默认工具」属于运行时知识，
/// 和 [`detect_runtime`] 一样只该有一份。
pub fn default_lint_tool(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Node | RuntimeKind::Bun => "eslint",
        RuntimeKind::Deno => "deno-lint",
        RuntimeKind::Python => "mypy",
        RuntimeKind::Go => "go-vet",
        RuntimeKind::Rust => "clippy",
        RuntimeKind::Dart => "dart-analyze",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(name: &str, files: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "lania-detect-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp project");
        for file in files {
            let path = root.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(path, "").expect("seed file");
        }
        root
    }

    #[test]
    fn feature_files_map_to_the_expected_runtime() {
        for (file, expected) in [
            ("deno.json", RuntimeKind::Deno),
            ("bun.lockb", RuntimeKind::Bun),
            ("bun.lock", RuntimeKind::Bun),
            ("pyproject.toml", RuntimeKind::Python),
            ("setup.py", RuntimeKind::Python),
            ("requirements.txt", RuntimeKind::Python),
            ("go.mod", RuntimeKind::Go),
            ("Cargo.toml", RuntimeKind::Rust),
            ("pubspec.yaml", RuntimeKind::Dart),
            ("package.json", RuntimeKind::Node),
        ] {
            let root = temp_project("feature", &[file]);
            assert_eq!(detect_project_runtime(&root), expected, "{file}");
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn deno_and_bun_win_over_package_json() {
        // §5.1 的验收项：deno.json + package.json 必须判成 deno。
        let root = temp_project("deno-dual", &["package.json", "deno.json"]);
        assert_eq!(detect_project_runtime(&root), RuntimeKind::Deno);
        let _ = std::fs::remove_dir_all(root);

        let root = temp_project("bun-dual", &["package.json", "bun.lockb"]);
        assert_eq!(detect_project_runtime(&root), RuntimeKind::Bun);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn non_js_signals_win_over_package_json() {
        // Go 项目里带一个 package.json（前端资源）仍必须判成 go。
        let root = temp_project("go-frontend", &["package.json", "go.mod"]);
        assert_eq!(detect_project_runtime(&root), RuntimeKind::Go);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn configured_runtime_beats_feature_files_and_cli_beats_config() {
        let root = temp_project("override", &["go.mod"]);
        // 用 `ConfigService::load_lan_snapshot` 构造快照：手写整个结构体会跟字段增删漂移。
        let mut config = lania_config::ConfigService::load_lan_snapshot(&serde_json::json!({
            "cwd": root.display().to_string(),
            "exists": true,
            "config": { "runtime": "python" },
        }))
        .expect("snapshot");
        assert_eq!(
            detect_runtime(&root, Some(&config), None),
            RuntimeKind::Python
        );
        assert_eq!(
            detect_runtime(&root, Some(&config), Some("rust")),
            RuntimeKind::Rust
        );

        // 未知的配置值不能把判定卡死：退回特征文件。
        config.runtime = Some("cobol".into());
        assert_eq!(detect_runtime(&root, Some(&config), None), RuntimeKind::Go);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn extension_fallback_handles_manifest_less_projects() {
        let root = temp_project("bare", &["main.py"]);
        assert_eq!(detect_project_runtime(&root), RuntimeKind::Python);
        let _ = std::fs::remove_dir_all(root);

        let root = temp_project("bare-dart", &["lib/main.dart"]);
        // 嵌套目录不算（只看 cwd 顶层），因此这里仍是 node 兜底。
        assert_eq!(detect_project_runtime(&root), RuntimeKind::Node);
        let _ = std::fs::remove_dir_all(root);

        let root = temp_project("bare-dart-flat", &["tool.dart"]);
        assert_eq!(detect_project_runtime(&root), RuntimeKind::Dart);
        let _ = std::fs::remove_dir_all(root);
    }

    // ---- 单一来源：Rust 判定顺序 vs TS `toolchain.ts` ----

    /// 从 `toolchain.ts` 里抽出 `DETECTION_PRIORITY` 的 `(file, runtime)` 列表。
    ///
    /// 不引入 TS 解析器：这段常量是纯字面量数组，按 `{ file: '…', runtime: '…' }`
    /// 逐条扫描足够稳，而它换来的是「两边漂移立刻红灯」。
    fn ts_detection_priority() -> Vec<(String, String)> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../ts/packages/node-bridge/src/plugins/toolchain.ts");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let start = source
            .find("export const DETECTION_PRIORITY")
            .expect("DETECTION_PRIORITY must exist in toolchain.ts");
        let block = &source[start..];
        let end = block
            .find("];")
            .expect("DETECTION_PRIORITY array must close");
        let block = &block[..end];

        let mut entries = Vec::new();
        for chunk in block.split('{').skip(1) {
            let file = quoted_value(chunk, "file:");
            let runtime = quoted_value(chunk, "runtime:");
            if let (Some(file), Some(runtime)) = (file, runtime) {
                entries.push((file, runtime));
            }
        }
        entries
    }

    fn quoted_value(chunk: &str, key: &str) -> Option<String> {
        let rest = &chunk[chunk.find(key)? + key.len()..];
        let quote = rest.find(['\'', '"'])?;
        let quote_char = rest.as_bytes()[quote] as char;
        let tail = &rest[quote + 1..];
        let end = tail.find(quote_char)?;
        Some(tail[..end].to_string())
    }

    /// §12 验收：特征文件顺序在仓库内只有一处定义。
    ///
    /// TS 的 `toolchain.ts`（JS 簇 / `toolchain.describe` 的权威来源）与 Rust 的
    /// [`FEATURE_PRIORITY`]（命令路由的权威来源）必须是同一份顺序；
    /// 这个用例把「两边各写一份」变成「两边漂移就红」。
    ///
    /// 允许 Rust 侧多出 Python 专属信号（`setup.py` / `requirements*.txt`）：
    /// 它们是 B8 为了让「只有 setup.py 的 Python 项目」也能走 Python bridge 而加的，
    /// TS 侧不参与路由（JS 簇路径对它走 node 默认工具）。多出来的条目必须排在
    /// 同一运行时的既有条目附近，不影响其它运行时的相对顺序。
    #[test]
    fn rust_detection_order_matches_the_ts_toolchain_table() {
        let ts = ts_detection_priority();
        assert!(!ts.is_empty(), "failed to parse DETECTION_PRIORITY");

        let rust: Vec<(String, String)> = FEATURE_PRIORITY
            .iter()
            .map(|(file, kind)| ((*file).to_string(), kind.id().to_string()))
            .collect();

        // TS 的每一条都必须在 Rust 表里，且相对顺序一致。
        let mut cursor = 0usize;
        for entry in &ts {
            let found = rust
                .iter()
                .skip(cursor)
                .position(|candidate| candidate == entry)
                .unwrap_or_else(|| {
                    panic!(
                        "toolchain.ts entry {entry:?} is missing (or out of order) in Rust FEATURE_PRIORITY\n\
                         ts:   {ts:?}\nrust: {rust:?}"
                    )
                });
            cursor += found + 1;
        }

        // Rust 多出来的条目只允许是 Python 的补充信号。
        for entry in &rust {
            if !ts.contains(entry) {
                assert_eq!(
                    entry.1, "python",
                    "only python has extra feature files in Rust (got {entry:?})"
                );
            }
        }
    }

    #[test]
    fn ambiguous_projects_get_a_notice_but_explicit_runtime_does_not() {
        // deno.json + package.json：双运行时 ⇒ 有提示，且提到了两个文件。
        let root = temp_project("notice", &["deno.json", "package.json"]);
        let decision = decide_runtime(&root, None, None);
        assert_eq!(decision.kind, RuntimeKind::Deno);
        let notice = decision.notice.expect("notice expected");
        assert!(notice.contains("deno.json"), "{notice}");
        assert!(notice.contains("package.json"), "{notice}");
        assert!(notice.contains("--runtime"), "{notice}");

        // 显式 `--runtime` 时保持安静。
        assert!(decide_runtime(&root, None, Some("node")).notice.is_none());
        let _ = std::fs::remove_dir_all(root);

        // 单一信号不啰嗦。
        let root = temp_project("notice-single", &["go.mod"]);
        let decision = decide_runtime(&root, None, None);
        assert_eq!(decision.kind, RuntimeKind::Go);
        assert!(decision.notice.is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}
