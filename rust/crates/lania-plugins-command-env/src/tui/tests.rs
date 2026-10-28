//! `App` 状态机测试：光标移动、模式切换、计划生成。

use super::app::{App, InputKind, Mode, Row};
use std::path::PathBuf;

fn app() -> App {
    let mut app = App::new(PathBuf::from("."), "zh");
    app.refresh();
    app
}

#[test]
fn rows_interleave_groups_and_tools() {
    let app = app();
    assert!(!app.rows.is_empty());
    // 首行必须是分组标题
    assert!(
        matches!(app.rows[0], Row::Group(_)),
        "first row: {:?}",
        app.rows[0]
    );
    // 存在工具行
    assert!(app.rows.iter().any(|row| matches!(row, Row::Tool(_))));
}

#[test]
fn cursor_never_lands_on_group_row() {
    let mut app = app();
    for _ in 0..app.rows.len() + 5 {
        assert!(
            matches!(app.rows[app.cursor], Row::Tool(_)),
            "cursor landed on {:?} at {}",
            app.rows[app.cursor],
            app.cursor
        );
        app.move_cursor(1);
    }
    for _ in 0..app.rows.len() + 5 {
        assert!(matches!(app.rows[app.cursor], Row::Tool(_)));
        app.move_cursor(-1);
    }
}

#[test]
fn move_cursor_clamps_at_both_ends() {
    let mut app = app();
    app.move_cursor(-100);
    // 首端落在第一个可选中的工具上（分组标题不可选中）。
    let first_index = app.selected_tool_index().expect("tool selected");
    assert_eq!(first_index, 0);

    app.move_cursor(1000);
    let last_index = app.selected_tool_index().expect("tool selected");
    let last_row_tool = app
        .rows
        .iter()
        .rev()
        .find_map(|row| match row {
            Row::Tool(index) => Some(*index),
            Row::Group(_) => None,
        })
        .expect("last tool row");
    assert_eq!(last_index, last_row_tool);
}

#[test]
fn input_mode_collects_version_and_builds_plan() {
    let mut app = app();
    app.begin_input(InputKind::Install);
    assert!(matches!(app.mode, Mode::Input(InputKind::Install)));

    app.input = "20".into();
    app.submit_input();
    assert!(matches!(app.mode, Mode::Confirm));
    assert!(app.pending_plan.is_some());
}

#[test]
fn input_rejects_invalid_version_characters_at_state_level() {
    // 状态机本身不做过滤（过滤在按键处理层），但计划构建必须拒绝注入串。
    let mut app = app();
    app.begin_input(InputKind::Use);
    app.input = "20;whoami".into();
    app.submit_input();
    let plan = app.pending_plan.as_ref().expect("plan exists");
    assert!(!plan.is_runnable(), "injection must be rejected");
    assert!(plan
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("invalid version"));
}

#[test]
fn project_pin_toggles() {
    let mut app = app();
    assert!(!app.project_pin);
    app.project_pin = true;
    app.begin_input(InputKind::Use);
    app.input = "3.12".into();
    app.submit_input();
    let plan = app.pending_plan.as_ref().expect("plan");
    // 开启项目锁定时，计划里应包含写版本文件的步骤。
    assert!(
        plan.steps.iter().any(|step| step.kind == "write-file"),
        "expected a write-file step, got {:?}",
        plan.steps
    );
}

#[test]
fn managed_flag_explains_self_managed_support() {
    let mut app = App::new_dry_run(PathBuf::from("."), "en");
    app.set_tools_for_test(vec![node_tool(), tool("ruby", "ruby")]);

    app.move_cursor(-100);
    assert_eq!(
        app.current().map(|tool| tool.language.as_str()),
        Some("node")
    );
    assert!(app.is_managed(), "node is a batch-1 self-managed language");
    assert_eq!(app.current_source(), Some("lania"));

    app.move_cursor(1000);
    assert_eq!(
        app.current().map(|tool| tool.language.as_str()),
        Some("ruby")
    );
    assert!(!app.is_managed(), "ruby has no provider yet");
}

#[test]
fn result_mode_refresh_flag_round_trip() {
    let mut app = app();
    app.mode = Mode::Result;
    app.pending_refresh = true;
    app.log = vec!["done".into()];
    assert!(app.pending_refresh);
}

fn node_tool() -> lania_runtime_manager::ToolStatus {
    lania_runtime_manager::ToolStatus {
        id: "node".into(),
        name: "Node.js".into(),
        language: "node".into(),
        installed: true,
        version: Some("20.0.0".into()),
        binary_path: None,
        source: Some("lania".into()),
        managed: true,
        upstream: Some("nodejs.org/dist".into()),
        installed_count: Some(1),
        project_version: None,
    }
}

fn plan(
    action: lania_runtime_manager::EnvAction,
    steps: Vec<lania_runtime_manager::PlanStep>,
) -> lania_runtime_manager::EnvPlan {
    lania_runtime_manager::EnvPlan {
        action,
        language: lania_runtime_manager::Language::Node,
        tool: "node".into(),
        version: Some("20".into()),
        managed: true,
        steps,
        error: None,
        notes: Vec::new(),
    }
}

#[test]
fn execute_plan_reports_error_for_unsupported_language() {
    let mut app = App::new_dry_run(PathBuf::from("."), "en");
    app.set_tools_for_test(vec![node_tool()]);
    let mut plan = plan(lania_runtime_manager::EnvAction::Install, Vec::new());
    plan.error = Some("ruby is not self-managed yet".into());
    app.pending_plan = Some(plan);

    app.execute_pending();

    assert!(matches!(app.mode, Mode::Result));
    assert!(app.log.iter().any(|line| line.contains("not self-managed")));
    assert!(!app.pending_refresh, "nothing ran, nothing to refresh");
}

#[test]
fn execute_write_file_step_writes_project_pin() {
    let dir = std::env::temp_dir().join(format!("lania-tui-pin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let pin = dir.join(".nvmrc").display().to_string();

    let mut app = App::new_dry_run(dir.clone(), "zh");
    app.set_tools_for_test(vec![node_tool()]);
    app.pending_plan = Some(plan(
        lania_runtime_manager::EnvAction::Use,
        vec![lania_runtime_manager::PlanStep {
            kind: "write-file".into(),
            label: "pin .nvmrc to 20".into(),
            command: format!("write {pin} <- 20"),
            path: Some(pin.clone()),
            content: Some("20".into()),
        }],
    ));

    app.execute_pending();

    assert!(app.status.contains("完成"), "status: {}", app.status);
    assert_eq!(std::fs::read_to_string(&pin).expect("pin file"), "20\n");
    assert!(app.pending_refresh);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_input_builds_remove_plan() {
    let mut app = App::new_dry_run(PathBuf::from("."), "en");
    app.set_tools_for_test(vec![node_tool()]);
    app.begin_input(InputKind::Remove);
    app.input = "20".into();
    app.submit_input();

    let plan = app.pending_plan.as_ref().expect("plan");
    assert_eq!(plan.action, lania_runtime_manager::EnvAction::Remove);
    assert!(matches!(app.mode, Mode::Confirm));
}

fn tool(id: &str, language: &str) -> lania_runtime_manager::ToolStatus {
    lania_runtime_manager::ToolStatus {
        id: id.into(),
        name: id.into(),
        language: language.into(),
        installed: true,
        version: None,
        binary_path: None,
        source: Some("system".into()),
        managed: false,
        upstream: None,
        installed_count: None,
        project_version: None,
    }
}

#[test]
fn companions_are_grouped_under_their_language() {
    let mut app = App::new_dry_run(PathBuf::from("."), "en");
    app.set_tools_for_test(vec![
        tool("node", "node"),
        tool("python", "python"),
        tool("npm", "node"),
        tool("pip", "python"),
    ]);

    let ordered: Vec<&str> = app
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Tool(index) => Some(app.tools[*index].id.as_str()),
            Row::Group(_) => None,
        })
        .collect();
    assert_eq!(ordered, ["node", "npm", "python", "pip"]);
}

#[test]
fn startup_reads_git_branch_and_repo_name() {
    let dir = std::env::temp_dir().join(format!("lania-tui-git-{}", std::process::id()));
    let git = dir.join(".git");
    std::fs::create_dir_all(&git).expect("git dir");
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");

    let app = App::new_dry_run(dir.clone(), "en");
    assert_eq!(
        app.repo.as_deref(),
        dir.file_name().and_then(|n| n.to_str())
    );
    assert_eq!(app.git_branch.as_deref(), Some("main"));
    assert!(!app.system.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}
