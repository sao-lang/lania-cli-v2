//! 人类可读输出：`env_*` 结果展示。
//!
//! `lan env` 的结果种类较多，但用户真正关心的信息很集中：
//! 「哪个语言、当前什么版本、从哪里来、装了几份、下一步该敲什么」。
//! 这里按 kind 分别渲染，JSON 模式不受影响。

use serde_json::Value;

use crate::cli_text;

use super::common::human_block;

pub(super) fn render_env_human(value: &Value, locale: &str) -> Option<String> {
    let kind = value.get("kind").and_then(Value::as_str)?;
    Some(match kind {
        "env_toolchains" => render_toolchains(value, locale),
        "env_installed" => render_installed(value, locale),
        "env_plan" => render_plan(value, locale),
        "env_toolchain_info" => render_info(value, locale),
        "env_versions" => render_versions(value, locale),
        "env_which" => render_which(value, locale),
        "env_init" => render_init(value, locale),
        "env_doctor" => render_doctor(value, locale),
        _ => return None,
    })
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn render_toolchains(value: &Value, locale: &str) -> String {
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut lines = Vec::new();
    for tool in &tools {
        let id = text(tool, "id");
        let version = tool
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_else(|| cli_text(locale, "not installed", "未安装"));
        let source = match tool.get("source").and_then(Value::as_str) {
            Some("lania") => cli_text(locale, "lania", "lania"),
            Some("system") => cli_text(locale, "system", "系统"),
            _ => "-",
        };
        let managed = tool
            .get("managed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let marker = if managed { "*" } else { " " };
        lines.push(format!("{marker} {id:<10} {version:<16} {source}"));
    }
    if lines.is_empty() {
        lines.push(cli_text(locale, "(nothing to show)", "（无）").to_string());
    }
    human_block(cli_text(locale, "Toolchains", "工具链"), lines.join("\n"))
}

fn render_installed(value: &Value, locale: &str) -> String {
    let entries = value
        .get("installed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut lines = Vec::new();
    if let Some(home) = value.get("home").and_then(Value::as_str) {
        lines.push(format!("{}: {home}", cli_text(locale, "Home", "根目录")));
        lines.push(String::new());
    }
    if entries.is_empty() {
        lines.push(
            cli_text(
                locale,
                "no runtime installed by lania yet; run `lan env install <tool> <version>`",
                "lania 尚未安装任何运行时；可执行 `lan env install <语言> <版本>`",
            )
            .to_string(),
        );
    }
    for entry in &entries {
        let current = entry
            .get("current")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let marker = if current { "*" } else { " " };
        lines.push(format!(
            "{marker} {:<10} {:<14} {}",
            text(entry, "language"),
            text(entry, "version"),
            text(entry, "path")
        ));
    }
    human_block(
        cli_text(locale, "Installed runtimes", "已安装运行时"),
        lines.join("\n"),
    )
}

fn render_plan(value: &Value, locale: &str) -> String {
    let status = text(value, "status");
    let title = if status == "dry_run" {
        cli_text(locale, "Plan (dry run)", "计划（演练）")
    } else {
        cli_text(locale, "Plan (executed)", "计划（已执行）")
    };
    let mut lines = vec![format!(
        "{} {} {}",
        text(value, "action"),
        text(value, "tool"),
        value
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
    )];
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        lines.push(format!("x {error}"));
        return human_block(title, lines.join("\n"));
    }
    if let Some(steps) = value.get("steps").and_then(Value::as_array) {
        for step in steps {
            lines.push(format!("> {}", text(step, "command")));
            lines.push(format!("  {}", text(step, "label")));
        }
    }
    if status == "dry_run" {
        lines.push(String::new());
        lines.push(cli_text(locale, "add --yes to execute", "确认无误后加 --yes 执行").to_string());
    }
    for note in value
        .get("notes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        lines.push(format!("! {note}"));
    }
    human_block(title, lines.join("\n"))
}

fn render_info(value: &Value, locale: &str) -> String {
    let tool = value.get("tool").cloned().unwrap_or(Value::Null);
    let mut lines = vec![
        format!(
            "{}: {}",
            cli_text(locale, "Version", "版本"),
            tool.get("version")
                .and_then(Value::as_str)
                .unwrap_or_else(|| cli_text(locale, "not installed", "未安装"))
        ),
        format!(
            "{}: {}",
            cli_text(locale, "Binary", "可执行文件"),
            tool.get("binaryPath")
                .and_then(Value::as_str)
                .unwrap_or("—")
        ),
        format!(
            "{}: {}",
            cli_text(locale, "Source", "来源"),
            tool.get("source").and_then(Value::as_str).unwrap_or("—")
        ),
        format!(
            "{}: {}",
            cli_text(locale, "Self-managed", "自管"),
            if tool
                .get("managed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "yes"
            } else {
                "no"
            }
        ),
    ];
    if let Some(upstream) = tool.get("upstream").and_then(Value::as_str) {
        lines.push(format!(
            "{}: {upstream}",
            cli_text(locale, "Upstream", "上游")
        ));
    }
    if let Some(pin) = tool.get("projectVersion") {
        let version = text(pin, "version");
        if !version.is_empty() {
            lines.push(format!(
                "{}: {} ({})",
                cli_text(locale, "Project pin", "项目固定"),
                version,
                text(pin, "file")
            ));
        }
    }
    let installed = value
        .get("installedVersions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    lines.push(String::new());
    lines.push(format!(
        "{} ({})",
        cli_text(locale, "Installed versions", "已安装版本"),
        installed.len()
    ));
    if installed.is_empty() {
        lines.push(format!("  {}", cli_text(locale, "none", "无")));
    }
    for item in installed {
        let marker = if item
            .get("current")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "*"
        } else {
            " "
        };
        lines.push(format!(
            "{marker} {:<12} {}",
            text(&item, "version"),
            text(&item, "source")
        ));
    }
    human_block(&text(&tool, "name"), lines.join("\n"))
}

fn render_versions(value: &Value, locale: &str) -> String {
    let versions = value
        .get("versions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut lines = Vec::new();
    if versions.is_empty() {
        lines.push(
            cli_text(
                locale,
                "no version installed by lania",
                "lania 尚未安装任何版本",
            )
            .to_string(),
        );
    }
    for item in &versions {
        let marker = if item
            .get("current")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "*"
        } else {
            " "
        };
        lines.push(format!("{marker} {}", text(item, "version")));
    }
    if let Some(available) = value.get("available").and_then(Value::as_array) {
        lines.push(String::new());
        lines.push(format!(
            "{} ({}): {}",
            cli_text(locale, "Available upstream", "上游可安装"),
            available.len(),
            available
                .iter()
                .take(10)
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    } else if let Some(error) = value
        .get("available")
        .and_then(|available| available.get("error"))
        .and_then(Value::as_str)
    {
        lines.push(String::new());
        lines.push(format!("! {error}"));
    }
    human_block(
        &format!(
            "{} {}",
            text(value, "tool"),
            cli_text(locale, "versions", "版本")
        ),
        lines.join("\n"),
    )
}

fn render_which(value: &Value, locale: &str) -> String {
    let name = text(value, "name");
    let path = text(value, "path");
    human_block(
        cli_text(locale, "Resolved entrypoint", "入口解析结果"),
        format!("{name} -> {path}"),
    )
}

fn render_init(value: &Value, locale: &str) -> String {
    let snippet = text(value, "snippet");
    human_block(
        &format!(
            "{} ({})",
            cli_text(locale, "Shell snippet", "Shell 片段"),
            text(value, "shell")
        ),
        format!(
            "{}: {}\n\n{snippet}",
            cli_text(locale, "Shims", "shims 目录"),
            text(value, "shims")
        ),
    )
}

fn render_doctor(value: &Value, locale: &str) -> String {
    let mut lines = Vec::new();
    let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
    lines.push(format!(
        "{}: {}",
        cli_text(locale, "Status", "状态"),
        if ok {
            cli_text(locale, "ok", "正常")
        } else {
            cli_text(locale, "attention needed", "需要关注")
        }
    ));

    // 自管语言：分「已装」和「还没装」两栏，避免和 unsupported 混在一起。
    let managed = value
        .get("managed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (installed, empty): (Vec<&Value>, Vec<&Value>) = managed
        .iter()
        .partition(|item| item.get("current").and_then(Value::as_str).is_some());
    if !installed.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "{} ({})",
            cli_text(locale, "Self-managed, installed", "自管已安装"),
            installed.len()
        ));
        for item in installed {
            lines.push(format!(
                "  {:<10} {}",
                text(item, "id"),
                text(item, "current")
            ));
        }
    }
    if !empty.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "{} ({})",
            cli_text(locale, "Self-managed, not installed", "自管但未安装"),
            empty.len()
        ));
        lines.push(format!(
            "  {}",
            empty
                .iter()
                .map(|item| text(item, "id"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    for (key, label_en, label_zh) in [
        ("missing", "Missing (not self-managed)", "缺失（非自管）"),
        ("external", "External installs", "外部安装"),
        ("unsupported", "Not self-managed yet", "尚未支持自管"),
    ] {
        let items = value
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        lines.push(String::new());
        lines.push(format!(
            "{} ({})",
            cli_text(locale, label_en, label_zh),
            items.len()
        ));
        if key == "unsupported" {
            lines.push(format!(
                "  {}",
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            continue;
        }
        for item in items {
            let id = text(&item, "id");
            let detail = item
                .get("current")
                .and_then(Value::as_str)
                .or_else(|| item.get("binary").and_then(Value::as_str))
                .unwrap_or_default();
            lines.push(format!("  {id:<10} {detail}"));
        }
    }

    if let Some(diagnostics) = value.get("diagnostics").and_then(Value::as_array) {
        if !diagnostics.is_empty() {
            lines.push(String::new());
            lines.push(cli_text(locale, "Diagnostics", "诊断").to_string());
            for item in diagnostics {
                let marker = match text(item, "level").as_str() {
                    "error" => "x",
                    _ => "!",
                };
                lines.push(format!("  {marker} {}", text(item, "message")));
            }
        }
    }
    human_block(cli_text(locale, "Env doctor", "环境体检"), lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn render(value: serde_json::Value) -> String {
        render_env_human(&value, "en").expect("env kind is supported")
    }

    #[test]
    fn renders_installed_listing() {
        let out = render(json!({
            "kind": "env_installed",
            "home": "/runex",
            "installed": [
                { "language": "node", "version": "20.11.1", "current": true, "path": "/runex/node/20.11.1" },
                { "language": "node", "version": "22.21.1", "current": false, "path": "/runex/node/22.21.1" }
            ]
        }));
        assert!(out.contains("Installed runtimes"));
        assert!(out.contains("* node"));
        assert!(out.contains("20.11.1"));
        assert!(out.contains("22.21.1"));
    }

    #[test]
    fn renders_empty_installed_listing_with_hint() {
        let out = render(json!({ "kind": "env_installed", "installed": [] }));
        assert!(out.contains("lan env install"), "{out}");
    }

    #[test]
    fn renders_dry_run_plan_with_next_step() {
        let out = render(json!({
            "kind": "env_plan",
            "status": "dry_run",
            "action": "install",
            "tool": "node",
            "version": "20",
            "steps": [{ "kind": "install", "command": "lan env install node 20 --yes", "label": "download node 20" }]
        }));
        assert!(out.contains("Plan (dry run)"));
        assert!(out.contains("download node 20"));
        assert!(out.contains("--yes"));
    }

    #[test]
    fn renders_plan_error() {
        let out = render(json!({
            "kind": "env_plan",
            "status": "dry_run",
            "action": "install",
            "tool": "ruby",
            "version": "3.3.0",
            "steps": [],
            "error": "ruby is not self-managed yet"
        }));
        assert!(out.contains("not self-managed"), "{out}");
    }

    #[test]
    fn renders_versions_with_upstream_availability() {
        let out = render(json!({
            "kind": "env_versions",
            "tool": "go",
            "versions": [{ "version": "1.22.12", "source": "lania", "current": true }],
            "available": ["1.24.0", "1.23.4"]
        }));
        assert!(out.contains("* 1.22.12"));
        assert!(out.contains("Available upstream (2)"));
    }

    #[test]
    fn renders_info_without_project_pin() {
        let out = render(json!({
            "kind": "env_toolchain_info",
            "tool": {
                "name": "Deno",
                "version": "1.46.3",
                "binaryPath": "/runex/deno/1.46.3/deno",
                "source": "lania",
                "managed": true,
                "upstream": "github.com/denoland/deno/releases",
                "projectVersion": { "file": "", "version": "" }
            },
            "installedVersions": [{ "version": "1.46.3", "source": "lania", "current": true }]
        }));
        assert!(out.contains("Source: lania"));
        assert!(out.contains("Self-managed: yes"));
        // 空的项目 pin 不应出现。
        assert!(!out.contains("Project pin"), "{out}");
    }

    #[test]
    fn renders_which_and_init() {
        assert!(render(json!({
            "kind": "env_which",
            "name": "node",
            "path": "/runex/node/20/bin/node"
        }))
        .contains("node -> /runex/node/20/bin/node"));

        let init = render(json!({
            "kind": "env_init",
            "shell": "bash",
            "shims": "/runex/shims",
            "snippet": "export PATH=\"/runex/shims:$PATH\""
        }));
        assert!(init.contains("Shell snippet (bash)"));
        assert!(init.contains("export PATH"));
    }

    #[test]
    fn renders_doctor_groups() {
        let out = render(json!({
            "kind": "env_doctor",
            "ok": false,
            "managed": [
                { "id": "node", "current": "20.11.1" },
                { "id": "go", "current": null }
            ],
            "missing": [{ "id": "java", "name": "Java" }],
            "external": [],
            "unsupported": ["ruby"],
            "diagnostics": [{ "level": "warn", "message": "shims not on PATH" }]
        }));
        assert!(out.contains("Self-managed, installed (1)"));
        assert!(out.contains("Self-managed, not installed (1)"));
        assert!(out.contains("go"), "{out}");
        assert!(out.contains("Not self-managed yet (1)"));
        assert!(out.contains("ruby"));
        assert!(out.contains("shims not on PATH"));
    }

    #[test]
    fn unknown_env_kind_is_not_rendered() {
        assert!(render_env_human(&json!({ "kind": "env_ui" }), "en").is_none());
    }
}
