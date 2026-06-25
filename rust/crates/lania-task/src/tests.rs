use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::anyhow;
use tokio_util::sync::CancellationToken;

use super::{
    TaskDefinition, TaskEventKind, TaskExecutor, TaskPriority, TaskRunMode, TaskRunOptions,
    TaskService, TaskSink, TaskState,
};

#[test]
fn tracks_task_lifecycle() {
    let service = TaskService::default();
    service.start("build", "Compile app");
    service.update("build", "Collecting inputs");
    service.complete("build", "Done");

    let tasks = service.snapshot();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].state, TaskState::Completed);
    assert_eq!(tasks[0].detail.as_deref(), Some("Done"));
}

#[test]
fn notifies_registered_sinks_on_events() {
    #[derive(Default)]
    struct RecordingSink {
        kinds: Arc<std::sync::Mutex<Vec<TaskEventKind>>>,
    }

    impl TaskSink for RecordingSink {
        fn on_event(&self, _record: &super::TaskRecord, event: &super::TaskEvent) {
            self.kinds.lock().expect("lock").push(event.kind.clone());
        }
    }

    let service = TaskService::default();
    let recording = Arc::new(RecordingSink::default());
    let sink: Arc<dyn TaskSink> = recording.clone();
    service.add_sink(sink);
    assert_eq!(service.sink_count(), 1);

    service.start("build", "Compile app");
    service.update("build", "Collecting inputs");
    service.complete("build", "Done");

    let kinds = recording.kinds.lock().expect("lock").clone();
    // start() may emit Registered first (if not previously present) then Started.
    assert!(kinds.contains(&TaskEventKind::Registered));
    assert!(kinds.contains(&TaskEventKind::Started));
    assert!(kinds.contains(&TaskEventKind::Updated));
    assert!(kinds.contains(&TaskEventKind::Completed));
}

#[tokio::test]
async fn runs_tasks_by_priority_with_serial_mode() {
    let service = TaskService::default();
    let order = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let make_task =
        |id: &'static str, priority: TaskPriority, order: Arc<tokio::sync::Mutex<Vec<String>>>| {
            TaskDefinition::new_text(id, id, move |_| {
                let order = Arc::clone(&order);
                async move {
                    order.lock().await.push(id.to_string());
                    Ok(id.to_string())
                }
            })
            .priority(priority)
        };

    let report = service
        .run_all(
            vec![
                make_task("low", TaskPriority::Low, Arc::clone(&order)),
                make_task("high", TaskPriority::High, Arc::clone(&order)),
                make_task("medium", TaskPriority::Medium, Arc::clone(&order)),
            ],
            TaskRunOptions {
                mode: TaskRunMode::Serial,
                ..TaskRunOptions::default()
            },
        )
        .await
        .expect("task run succeeds");

    assert!(report.failures.is_empty());
    assert_eq!(
        order.lock().await.clone(),
        vec!["high".to_string(), "medium".to_string(), "low".to_string()]
    );
}

#[tokio::test]
async fn limits_parallelism_with_semaphore() {
    let service = TaskService::default();
    let current = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let tasks = (0..4)
        .map(|index| {
            let current = Arc::clone(&current);
            let max_seen = Arc::clone(&max_seen);
            TaskDefinition::new_text(format!("task-{index}"), "parallel", move |_| {
                let current = Arc::clone(&current);
                let max_seen = Arc::clone(&max_seen);
                async move {
                    let running = current.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(running, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok("done".into())
                }
            })
            .group("build")
        })
        .collect();

    service
        .run_all(
            tasks,
            TaskRunOptions {
                max_concurrency: 4,
                group_concurrency: BTreeMap::from([("build".into(), 2)]),
                ..TaskRunOptions::default()
            },
        )
        .await
        .expect("task run succeeds");

    assert_eq!(max_seen.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn propagates_cancellation_and_rolls_back_successes() {
    let service = TaskService::default();
    let cancelled = CancellationToken::new();
    let cancel_child = cancelled.clone();
    let rollback_hits = Arc::new(AtomicUsize::new(0));

    let fast =
        TaskDefinition::new_text("fast", "fast", |_| async { Ok("fast".into()) }).rollback({
            let rollback_hits = Arc::clone(&rollback_hits);
            move || {
                let rollback_hits = Arc::clone(&rollback_hits);
                async move {
                    rollback_hits.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }
        });
    let slow = TaskDefinition::new_text("slow", "slow", move |token| async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => Ok("slow".into()),
            _ = token.cancelled() => Err(anyhow!("cancelled from token")),
        }
    });

    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel_child.cancel();
    });

    let report = service
        .run_all(
            vec![fast, slow],
            TaskRunOptions {
                cancellation: Some(cancelled),
                rollback_on_error: true,
                stop_on_error: true,
                ..TaskRunOptions::default()
            },
        )
        .await
        .expect("task run returns report");
    canceller.await.expect("canceller joins");

    assert!(report.cancelled);
    assert_eq!(rollback_hits.load(Ordering::SeqCst), 1);
    assert!(service
        .events()
        .iter()
        .any(|event| event.kind == super::TaskEventKind::RollbackCompleted));
}

#[tokio::test]
async fn aggregates_failures_and_emits_event_stream() {
    let service = TaskService::default();
    let report = service
        .run_all(
            vec![
                TaskDefinition::new_text("fail-a", "fail-a", |_| async { Err(anyhow!("a")) }),
                TaskDefinition::new_text("fail-b", "fail-b", |_| async { Err(anyhow!("b")) }),
            ],
            TaskRunOptions {
                stop_on_error: false,
                ..TaskRunOptions::default()
            },
        )
        .await
        .expect("task run returns report");

    assert_eq!(report.failures.len(), 2);
    let events = service.events();
    assert!(events
        .iter()
        .any(|event| event.kind == super::TaskEventKind::Registered));
    assert!(events
        .iter()
        .any(|event| event.kind == super::TaskEventKind::Failed));
}

#[tokio::test]
async fn task_executor_supports_pause_resume_and_dynamic_add() {
    let service = TaskService::default();
    let executor = TaskExecutor::new(
        service,
        TaskRunOptions {
            mode: TaskRunMode::Parallel,
            max_concurrency: 1,
            stop_on_error: true,
            ..TaskRunOptions::default()
        },
    );

    let started = Arc::new(AtomicUsize::new(0));
    let started_slow = Arc::clone(&started);
    executor.add_task(
        TaskDefinition::new_text("slow", "slow", move |_| {
            let started = Arc::clone(&started_slow);
            async move {
                started.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(80)).await;
                Ok("slow".into())
            }
        })
        .group("g"),
    );

    let report = tokio::task::LocalSet::new()
        .run_until(async {
            executor.pause();
            let handle = {
                let executor = executor.clone();
                tokio::task::spawn_local(async move { executor.run().await.expect("run ok") })
            };

            tokio::time::sleep(Duration::from_millis(20)).await;
            assert_eq!(started.load(Ordering::SeqCst), 0);

            executor.resume();
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert_eq!(started.load(Ordering::SeqCst), 1);
            assert!(executor.running_task_ids().contains(&"slow".to_string()));

            // Add another task while running.
            executor.add_task(
                TaskDefinition::new_text("fast", "fast", |_| async { Ok("fast".into()) })
                    .group("g"),
            );

            handle.await.expect("join")
        })
        .await;
    assert!(report.failures.is_empty());
    assert_eq!(report.outcomes.len(), 2);
    assert!(report
        .outcomes
        .iter()
        .all(|outcome| outcome.state == TaskState::Completed));
}

#[tokio::test]
async fn task_executor_supports_group_cancel() {
    let service = TaskService::default();
    let executor = TaskExecutor::new(
        service,
        TaskRunOptions {
            mode: TaskRunMode::Parallel,
            max_concurrency: 2,
            stop_on_error: false,
            ..TaskRunOptions::default()
        },
    );

    executor.add_task(
        TaskDefinition::new_text("a", "a", |token| async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => Ok("a".into()),
                _ = token.cancelled() => Err(anyhow!("cancelled")),
            }
        })
        .group("cancel-group"),
    );
    executor.add_task(
        TaskDefinition::new_text("b", "b", |token| async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => Ok("b".into()),
                _ = token.cancelled() => Err(anyhow!("cancelled")),
            }
        })
        .group("cancel-group"),
    );

    let report = tokio::task::LocalSet::new()
        .run_until(async {
            let handle = {
                let executor = executor.clone();
                tokio::task::spawn_local(async move { executor.run().await.expect("run ok") })
            };

            tokio::time::sleep(Duration::from_millis(30)).await;
            executor.cancel_group("cancel-group");

            handle.await.expect("join")
        })
        .await;
    assert!(report.failures.is_empty());
    assert!(report.cancelled);
    assert_eq!(report.outcomes.len(), 2);
    assert!(report
        .outcomes
        .iter()
        .all(|outcome| outcome.state == TaskState::Cancelled));
}
