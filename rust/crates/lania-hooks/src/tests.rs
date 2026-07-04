//! HookBusImpl 与 hook key 辅助函数的单元测试。
//!
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::*;

// ── 测试用 MockInvoker ─────────────────────────────────────

struct MockInvoker {
    hook_key: String,
    /// Sync handler for simple cases
    handler: Box<dyn Fn(&str, &str, HookKind, &Value) -> Result<Option<Value>> + Send + Sync>,
    /// Optional async handler for timeout/cancellation tests
    async_handler:
        Option<Box<dyn Fn(&str, &str, HookKind, Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Value>>> + Send>> + Send + Sync>>,
}

#[async_trait]
impl HookInvoker for MockInvoker {
    async fn invoke(
        &self,
        source: &str,
        hook_key: &str,
        kind: HookKind,
        payload: &Value,
    ) -> Result<HookInvokeOutcome> {
        if let Some(ref async_handler) = self.async_handler {
            let result = async_handler(source, hook_key, kind, payload.clone()).await?;
            return Ok(HookInvokeOutcome { payload: result });
        }
        let result = (self.handler)(source, hook_key, kind, payload)?;
        Ok(HookInvokeOutcome { payload: result })
    }
}

fn make_invoker<F>(f: F) -> Arc<dyn HookInvoker>
where
    F: Fn(&str, &str, HookKind, &Value) -> Result<Option<Value>> + Send + Sync + 'static,
{
    Arc::new(MockInvoker {
        hook_key: "test".into(),
        handler: Box::new(f),
        async_handler: None,
    })
}

fn make_async_invoker<F, Fut>(f: F) -> Arc<dyn HookInvoker>
where
    F: Fn(&str, &str, HookKind, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Option<Value>>> + Send + 'static,
{
    Arc::new(MockInvoker {
        hook_key: "test".into(),
        handler: Box::new(|_, _, _, _| Ok(None)),
        async_handler: Some(Box::new(move |s, k, kind, p| Box::pin(f(s, k, kind, p)))),
    })
}

fn make_bus() -> HookBusImpl {
    HookBusImpl::new()
}

// ── is_known_hook_key ───────────────────────────────────────

#[test]
fn is_known_hook_key_returns_true_for_all_20_keys() {
    for key in &hook_keys::ALL {
        assert!(is_known_hook_key(key), "key should be known: {key}");
    }
}

#[test]
fn is_known_hook_key_returns_false_for_unknown() {
    assert!(!is_known_hook_key("onUnknown"));
    assert!(!is_known_hook_key(""));
}

// ── default_hook_kind ───────────────────────────────────────

#[test]
fn default_hook_kind_returns_waterfall_for_specific_keys() {
    assert_eq!(
        default_hook_kind(hook_keys::ON_COMMAND_PRE_INIT),
        HookKind::Waterfall
    );
    assert_eq!(
        default_hook_kind(hook_keys::ON_ARGS_PARSED),
        HookKind::Waterfall
    );
    assert_eq!(
        default_hook_kind(hook_keys::ON_FILES_PREPARE),
        HookKind::Waterfall
    );
    assert_eq!(
        default_hook_kind(hook_keys::ON_CONFIG_GET),
        HookKind::Waterfall
    );
}

#[test]
fn default_hook_kind_returns_parallel_for_other_keys() {
    assert_eq!(
        default_hook_kind(hook_keys::ON_INITIALIZE),
        HookKind::Parallel
    );
    assert_eq!(
        default_hook_kind(hook_keys::ON_SUCCESS),
        HookKind::Parallel
    );
    assert_eq!(
        default_hook_kind(hook_keys::ON_ERROR),
        HookKind::Parallel
    );
    assert_eq!(
        default_hook_kind(hook_keys::ON_SHUTDOWN),
        HookKind::Parallel
    );
}

// ── register ────────────────────────────────────────────────

#[test]
fn register_adds_to_registrations() {
    let mut bus = make_bus();
    bus.register(HookRegistration {
        key: "onTest".into(),
        kind: HookKind::Parallel,
        plugin: "test-plugin".into(),
        handler: "handler1".into(),
        description: "test".into(),
    });
    let snapshot = bus.snapshot();
    assert_eq!(snapshot.registrations.len(), 1);
    assert_eq!(snapshot.registrations[0].key, "onTest");
}

#[test]
fn register_multiple_invokers_for_same_key() {
    let mut bus = make_bus();
    let inv = make_invoker(|_, _, _, _| Ok(Some(json!({"v": 1}))));
    bus.register_invoker("myHook".into(), inv.clone());
    bus.register_invoker("myHook".into(), inv);
    let snapshot = bus.snapshot();
    // registrations don't track invokers directly
    // but we can verify via snapshot that bus is alive
    assert!(snapshot.registrations.is_empty());
}

// ── record_event ────────────────────────────────────────────

#[test]
fn record_event_increases_sequence() {
    let bus = make_bus();
    bus.record_event("src".into(), "onTest".into(), json!({"a": 1}));
    bus.record_event("src".into(), "onTest".into(), json!({"a": 2}));
    let snapshot = bus.snapshot();
    assert_eq!(snapshot.events.len(), 2);
    assert_eq!(snapshot.events[0].sequence, 1);
    assert_eq!(snapshot.events[1].sequence, 2);
}

// ── call_parallel ───────────────────────────────────────────

#[tokio::test]
async fn call_parallel_invokes_all_matching_invokers() {
    let mut bus = make_bus();
    let invoked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let invoked1 = invoked.clone();
    bus.register_invoker(
        "onTest".into(),
        make_invoker(move |_, _, _, _| {
            invoked1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        }),
    );
    let invoked2 = invoked.clone();
    bus.register_invoker(
        "onTest".into(),
        make_invoker(move |_, _, _, _| {
            invoked2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        }),
    );

    bus.call_parallel("src".into(), "onTest".into(), json!({"x": 1}))
        .await
        .expect("parallel call should succeed");
    assert_eq!(invoked.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn call_parallel_skips_non_matching_keys() {
    let mut bus = make_bus();
    let invoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let inv = invoked.clone();
    bus.register_invoker(
        "otherHook".into(),
        make_invoker(move |_, _, _, _| {
            inv.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        }),
    );

    bus.call_parallel("src".into(), "onTest".into(), json!({}))
        .await
        .expect("parallel call should succeed");
    assert!(!invoked.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn call_parallel_propagates_error_when_policy_is_throw() {
    let mut bus = make_bus();
    bus.register_invoker_with_options(
        "onErr".into(),
        make_invoker(|_, _, _, _| Err(anyhow::anyhow!("oops"))),
        HookInvokerOptions {
            timeout_ms: None,
            on_error: HookErrorPolicy::Throw,
        },
    );

    let result = bus
        .call_parallel("src".into(), "onErr".into(), json!({}))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn call_parallel_collects_error_when_policy_is_collect() {
    let mut bus = make_bus();
    bus.register_invoker_with_options(
        "onErr".into(),
        make_invoker(|_, _, _, _| Err(anyhow::anyhow!("oops"))),
        HookInvokerOptions {
            timeout_ms: None,
            on_error: HookErrorPolicy::Collect,
        },
    );

    let result = bus
        .call_parallel("src".into(), "onErr".into(), json!({}))
        .await;
    assert!(result.is_ok());
}

// ── call_waterfall ──────────────────────────────────────────

#[tokio::test]
async fn call_waterfall_passes_payload_through_invokers() {
    let mut bus = make_bus();
    bus.register_invoker(
        "wf".into(),
        make_invoker(|_, _, _, payload| {
            let v = payload["x"].as_i64().unwrap_or(0) + 1;
            Ok(Some(json!({"x": v})))
        }),
    );
    bus.register_invoker(
        "wf".into(),
        make_invoker(|_, _, _, payload| {
            let v = payload["x"].as_i64().unwrap_or(0) + 1;
            Ok(Some(json!({"x": v})))
        }),
    );

    let result = bus
        .call_waterfall("src".into(), "wf".into(), json!({"x": 0}))
        .await
        .expect("waterfall should succeed");
    assert_eq!(result["x"], 2);
}

#[tokio::test]
async fn call_waterfall_skips_invokers_that_return_none() {
    let mut bus = make_bus();
    bus.register_invoker(
        "wf".into(),
        make_invoker(|_, _, _, payload| {
            let v = payload["x"].as_i64().unwrap_or(0) + 1;
            Ok(Some(json!({"x": v})))
        }),
    );
    bus.register_invoker(
        "wf".into(),
        make_invoker(|_, _, _, _| Ok(None)), // observer — no mutation
    );
    bus.register_invoker(
        "wf".into(),
        make_invoker(|_, _, _, payload| {
            let v = payload["x"].as_i64().unwrap_or(0) + 1;
            Ok(Some(json!({"x": v})))
        }),
    );

    let result = bus
        .call_waterfall("src".into(), "wf".into(), json!({"x": 0}))
        .await
        .expect("waterfall should succeed");
    // First adds 1, second does nothing, third adds 1 = 2
    assert_eq!(result["x"], 2);
}

#[tokio::test]
async fn call_waterfall_propagates_error_when_policy_is_throw() {
    let mut bus = make_bus();
    bus.register_invoker_with_options(
        "wf".into(),
        make_invoker(|_, _, _, _| Err(anyhow::anyhow!("fail"))),
        HookInvokerOptions {
            timeout_ms: None,
            on_error: HookErrorPolicy::Throw,
        },
    );

    let result = bus
        .call_waterfall("src".into(), "wf".into(), json!({}))
        .await;
    assert!(result.is_err());
}

// ── timeout ─────────────────────────────────────────────────

#[tokio::test]
async fn parallel_hook_times_out() {
    let mut bus = make_bus();
    bus.register_invoker_with_options(
        "slow".into(),
        make_async_invoker(|_, _, _, _| async {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Ok(None)
        }),
        HookInvokerOptions {
            timeout_ms: Some(10),
            on_error: HookErrorPolicy::Throw,
        },
    );

    let result = bus
        .call_parallel("src".into(), "slow".into(), json!({}))
        .await;
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("timed out"), "error should mention timeout: {err}");
}

#[tokio::test]
async fn waterfall_hook_times_out() {
    let mut bus = make_bus();
    bus.register_invoker_with_options(
        "slow".into(),
        make_async_invoker(|_, _, _, _| async {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Ok(None)
        }),
        HookInvokerOptions {
            timeout_ms: Some(10),
            on_error: HookErrorPolicy::Throw,
        },
    );

    let result = bus
        .call_waterfall("src".into(), "slow".into(), json!({}))
        .await;
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("timed out"), "error should mention timeout: {err}");
}

// ── cancellation ────────────────────────────────────────────

#[tokio::test]
async fn parallel_hook_respects_cancellation() {
    let mut bus = make_bus();
    bus.register_invoker(
        "cancelTest".into(),
        make_async_invoker(|_, _, _, _| async {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Ok(None)
        }),
    );

    let token = CancellationToken::new();
    let token_clone = token.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        token_clone.cancel();
    });

    let result = bus
        .call_parallel_with_options(
            "src".into(),
            "cancelTest".into(),
            json!({}),
            HookCallOptions {
                cancellation: Some(token),
            },
        )
        .await;
    let _ = handle.await;
    assert!(result.is_err());
}

#[tokio::test]
async fn waterfall_hook_respects_cancellation() {
    let mut bus = make_bus();
    bus.register_invoker(
        "cancelTest".into(),
        make_async_invoker(|_, _, _, _| async {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Ok(None)
        }),
    );

    let token = CancellationToken::new();
    let token_clone = token.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        token_clone.cancel();
    });

    let result = bus
        .call_waterfall_with_options(
            "src".into(),
            "cancelTest".into(),
            json!({}),
            HookCallOptions {
                cancellation: Some(token),
            },
        )
        .await;
    let _ = handle.await;
    assert!(result.is_err());
}

// ── snapshot ────────────────────────────────────────────────

#[test]
fn snapshot_returns_current_registrations_and_events() {
    let mut bus = make_bus();
    bus.register(HookRegistration {
        key: "onX".into(),
        kind: HookKind::Parallel,
        plugin: "p".into(),
        handler: "h".into(),
        description: "d".into(),
    });
    bus.record_event("src".into(), "onX".into(), json!({"k": "v"}));

    let snap = bus.snapshot();
    assert_eq!(snap.registrations.len(), 1);
    assert_eq!(snap.events.len(), 1);
}

#[test]
fn snapshot_events_are_append_only() {
    let bus = make_bus();
    bus.record_event("s1".into(), "k1".into(), json!({}));
    let snap1 = bus.snapshot();
    assert_eq!(snap1.events.len(), 1);

    bus.record_event("s2".into(), "k2".into(), json!({}));
    let snap2 = bus.snapshot();
    assert_eq!(snap2.events.len(), 2);
}
