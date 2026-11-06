use super::{
    IndicatifProgressRenderer, JsonProgressRenderer, ProgressKind, ProgressService, ProgressStatus,
};
use lania_task::TaskService;

#[test]
fn tracks_progress_tree_and_percent() {
    let service = ProgressService::default();
    service.begin_group("lint", Some(4), ProgressKind::ProgressBar);
    service.begin_step("eslint", "lint", Some(2), ProgressKind::ProgressBar);
    service.begin_item("file-a", "eslint", None, ProgressKind::StaticStep);
    service.advance("lint", 1);
    service.advance("eslint", 2);
    service.finish("file-a");
    service.link_task("eslint", "task-eslint");

    let progress = service.snapshot();
    let lint = progress
        .iter()
        .find(|item| item.id == "lint")
        .expect("lint progress");
    let eslint = progress
        .iter()
        .find(|item| item.id == "eslint")
        .expect("eslint progress");
    assert_eq!(lint.percent(), Some(25));
    assert_eq!(eslint.parent_id.as_deref(), Some("lint"));
    assert_eq!(eslint.task_id.as_deref(), Some("task-eslint"));
}

#[test]
fn records_event_stream_and_status_changes() {
    let service = ProgressService::default();
    service.begin("dev", None);
    service.message("dev", "starting server");
    service.detail("dev", "port 3000");
    service.cancel("dev", "interrupted");

    let events = service.events();
    assert_eq!(events.len(), 4);
    let snapshot = service
        .snapshot()
        .into_iter()
        .find(|item| item.id == "dev")
        .expect("dev progress");
    assert_eq!(snapshot.status, ProgressStatus::Cancelled);
    assert!(snapshot.finished_at_ms.is_some());
}

#[test]
fn supports_update_total_and_reset() {
    let service = ProgressService::default();
    service.begin_group("download", None, ProgressKind::Spinner);
    service.update_total("download", Some(10));
    service.advance("download", 3);
    let snapshot = service
        .snapshot()
        .into_iter()
        .find(|item| item.id == "download")
        .expect("download progress");
    assert_eq!(snapshot.total, Some(10));
    assert_eq!(snapshot.current, 3);

    service.reset("download");
    assert!(service
        .snapshot()
        .into_iter()
        .all(|item| item.id != "download"));
}

#[test]
fn separates_json_summary_from_terminal_renderer() {
    let service = ProgressService::default();
    service.begin_group("build", Some(2), ProgressKind::ProgressBar);
    service.advance("build", 1);
    service.message("build", "bundling");
    service.finish("build");

    let json_lines = service.render(&JsonProgressRenderer);
    assert!(json_lines[0].contains("\"status\":\"completed\""));

    let text_lines = service.render(&IndicatifProgressRenderer::default());
    assert!(text_lines[0].contains("status=completed"));
    assert!(text_lines[0].contains("bundling"));
}

#[test]
fn computes_duration_rate_and_eta() {
    let service = ProgressService::default();
    service.begin_group("create", Some(10), ProgressKind::ProgressBar);
    service.advance("create", 5);
    std::thread::sleep(std::time::Duration::from_millis(5));
    service.finish("create");

    let snapshot = service.summary().items.remove(0);
    assert!(snapshot.duration_ms().is_some());
    assert!(snapshot.rate_per_sec().is_some());
    assert_eq!(snapshot.eta_ms(), Some(0));
}

#[test]
fn bridges_task_events_into_progress_snapshots() {
    let tasks = TaskService::default();
    let progress = ProgressService::default();
    tasks.add_sink(progress.task_sink());

    tasks.register(
        "build",
        "Compile app",
        "build",
        lania_task::TaskPriority::Medium,
    );
    tasks.start("build", "Compile app");
    tasks.update("build", "Bundling");
    tasks.complete("build", "Done");

    let snapshot = progress
        .snapshot()
        .into_iter()
        .find(|item| item.id == "task.build")
        .expect("task progress exists");
    assert_eq!(snapshot.task_id.as_deref(), Some("build"));
    assert_eq!(snapshot.message.as_deref(), Some("Compile app"));
    assert_eq!(snapshot.detail.as_deref(), Some("Done"));
    assert_eq!(snapshot.status, ProgressStatus::Completed);

    let group = progress
        .snapshot()
        .into_iter()
        .find(|item| item.id == "task_group.build")
        .expect("task group progress exists");
    assert_eq!(group.total, Some(1));
    assert_eq!(group.current, 1);
}
