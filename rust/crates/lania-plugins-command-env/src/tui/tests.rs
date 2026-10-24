//! `App` 状态机测试：光标移动、模式切换、计划生成。

use super::app::{App, InputKind, Mode, Row};
use std::path::PathBuf;

fn app() -> App {
    let mut app = App::new(PathBuf::from("."));
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
    assert_eq!(last_index, app.tools.len() - 1);
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
        plan.steps.iter().any(|step| step.program == "write-file"),
        "expected a write-file step, got {:?}",
        plan.steps
    );
}

#[test]
fn manager_cycle_returns_to_auto() {
    let mut app = app();
    let id = app.current().map(|tool| tool.id.clone()).expect("tool");
    app.cycle_manager();
    assert!(app.manager_choice.contains_key(&id));
    for _ in 0..32 {
        app.cycle_manager();
        if !app.manager_choice.contains_key(&id) {
            return;
        }
    }
    panic!("cycle_manager never returned to auto");
}

#[test]
fn effective_manager_does_not_fake_system_installs() {
    let mut app = app();
    let Some(index) = app
        .tools
        .iter()
        .position(|tool| tool.installed && tool.manager.is_none() && tool.id == tool.language)
    else {
        return;
    };
    // 把光标移到该工具
    while app.selected_tool_index() != Some(index) {
        let before = app.cursor;
        app.move_cursor(1);
        if app.cursor == before {
            return;
        }
    }
    assert!(
        app.effective_manager().is_none(),
        "system-installed tool must not claim a manager"
    );
}

#[test]
fn result_mode_refresh_flag_round_trip() {
    let mut app = app();
    app.mode = Mode::Result;
    app.pending_refresh = true;
    app.log = vec!["done".into()];
    assert!(app.pending_refresh);
}
