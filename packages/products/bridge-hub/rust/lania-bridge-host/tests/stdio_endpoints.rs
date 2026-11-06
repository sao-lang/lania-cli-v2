//! 跨语言 stdio 端点集成验证：Rust Hub 与 TS/Python 端点通信，帧里不含语言字段，
//! 寻址只靠 capability / topic / opaque endpointId。

use std::time::{Duration, Instant};

use lania_bridge_bus::{BridgeHub, BusFrame, Endpoint};
use lania_bridge_host::{default_asset_root, BridgeHost};
use lania_bridge_protocol::{E_METHOD_NOT_FOUND, E_TIMEOUT};
use serde_json::json;

fn host_for(name: &str) -> BridgeHost {
    BridgeHost::with_asset_root(default_asset_root().join(format!("test-{name}")))
        .expect("create bridge host")
}

fn wait_for<F>(host: &mut BridgeHost, mut predicate: F, timeout: Duration) -> bool
where
    F: FnMut(&mut BridgeHub) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        host.pump_until_idle(200);
        if predicate(host.hub()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn wait_frame(host: &mut BridgeHost, endpoint: &Endpoint, timeout: Duration) -> BusFrame {
    let deadline = Instant::now() + timeout;
    loop {
        host.pump_until_idle(200);
        if let Some(frame) = endpoint.try_recv() {
            return frame;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for a frame ({E_TIMEOUT})");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn staged_assets_contain_all_bindings_and_adaptors() {
    let host = host_for("assets");
    let root = host.stage_assets().expect("stage assets");
    for relative in [
        "protocol/gen/ts/protocol.ts",
        "protocol/gen/ts/runtime.ts",
        "protocol/gen/py/lania_bridge_protocol.py",
        "protocol/gen/py/lania_bridge_runtime.py",
        "protocol/gen/go/protocol.go",
        "protocol/gen/dart/runtime.dart",
        "adaptors/ts/endpoint.ts",
        "adaptors/py/endpoint.py",
        "adaptors/go/endpoint.go",
        "adaptors/go/go.mod",
        "adaptors/dart/endpoint.dart",
        "adaptors/dart/pubspec.yaml",
    ] {
        assert!(root.join(relative).is_file(), "missing staged asset: {relative}");
    }
}

#[test]
fn python_endpoint_answers_request_and_acks_events() {
    let mut host = host_for("python");
    host.attach_python("py-endpoint").expect("spawn python endpoint");
    let mut cli = host.attach("cli");
    cli.announce_str(&[], &["event.echo_ack"]);

    assert!(
        wait_for(
            &mut host,
            |hub| hub.providers_of("echo.ping").iter().any(|id| id == "py-endpoint"),
            Duration::from_secs(25),
        ),
        "python endpoint did not announce its capabilities"
    );

    let id = cli.request("echo.upper", json!({ "text": "hello" }));
    match wait_frame(&mut host, &cli, Duration::from_secs(15)) {
        BusFrame::Response {
            id: response_id,
            result,
            error,
        } => {
            assert_eq!(response_id, id);
            assert!(error.is_none(), "unexpected error: {error:?}");
            let result = result.expect("result");
            assert_eq!(result["text"], "HELLO");
            assert_eq!(result["endpointId"], "py-endpoint");
        }
        other => panic!("expected response, got {other:?}"),
    }

    cli.publish("event.diag", json!({ "text": "hi" }));
    match wait_frame(&mut host, &cli, Duration::from_secs(15)) {
        BusFrame::Event { topic, payload, from } => {
            assert_eq!(topic, "event.echo_ack");
            assert_eq!(from, "py-endpoint");
            assert_eq!(payload["receivedTopic"], "event.diag");
            assert_eq!(payload["from"], "cli");
        }
        other => panic!("expected ack event, got {other:?}"),
    }
}

#[test]
fn node_endpoint_answers_request_and_acks_events() {
    let mut host = host_for("node");
    host.attach_node("ts-endpoint").expect("spawn node endpoint");
    let mut cli = host.attach("cli");
    cli.announce_str(&[], &["event.echo_ack"]);

    assert!(
        wait_for(
            &mut host,
            |hub| hub.providers_of("echo.ping").iter().any(|id| id == "ts-endpoint"),
            Duration::from_secs(25),
        ),
        "node endpoint did not announce its capabilities"
    );

    let id = cli.request("echo.ping", json!({ "text": "hi" }));
    match wait_frame(&mut host, &cli, Duration::from_secs(15)) {
        BusFrame::Response { id: response_id, result, error } => {
            assert_eq!(response_id, id);
            assert!(error.is_none(), "unexpected error: {error:?}");
            let result = result.expect("result");
            assert_eq!(result["pong"], true);
            assert_eq!(result["endpointId"], "ts-endpoint");
        }
        other => panic!("expected response, got {other:?}"),
    }

    cli.publish("event.agent_start", json!({ "turn": 1 }));
    match wait_frame(&mut host, &cli, Duration::from_secs(15)) {
        BusFrame::Event { topic, payload, from } => {
            assert_eq!(topic, "event.echo_ack");
            assert_eq!(from, "ts-endpoint");
            assert_eq!(payload["receivedTopic"], "event.agent_start");
        }
        other => panic!("expected ack event, got {other:?}"),
    }
}

/// Go / Dart 端点需要本机安装对应 SDK，默认不进 CI；用
/// `cargo test -p lania-bridge-host -- --ignored` 显式验证。
#[test]
#[ignore = "requires local Go and Dart SDKs"]
fn go_and_dart_endpoints_announce() {
    let mut host = host_for("go-dart");
    host.attach_go("go-endpoint").expect("spawn go endpoint");
    host.attach_dart("dart-endpoint").expect("spawn dart endpoint");
    assert!(
        wait_for(
            &mut host,
            |hub| {
                let providers = hub.providers_of("echo.ping");
                providers.iter().any(|id| id == "go-endpoint")
                    && providers.iter().any(|id| id == "dart-endpoint")
            },
            Duration::from_secs(60),
        ),
        "go / dart endpoints did not announce"
    );

    let mut cli = host.attach("cli");
    cli.announce_str(&[], &[]);
    let _id = cli.request("echo.upper", json!({ "text": "hello" }));
    match wait_frame(&mut host, &cli, Duration::from_secs(20)) {
        BusFrame::Response { result, error, .. } => {
            assert!(error.is_none(), "unexpected error: {error:?}");
            assert_eq!(result.expect("result")["text"], "HELLO");
        }
        other => panic!("expected response, got {other:?}"),
    }
}

#[test]
fn two_language_endpoints_fan_out_and_share_one_capability() {
    let mut host = host_for("cross-language");
    host.attach_python("py-endpoint").expect("spawn python endpoint");
    host.attach_node("ts-endpoint").expect("spawn node endpoint");
    let mut cli = host.attach("cli");
    cli.announce_str(&["tool.exec"], &["event.echo_ack"]);

    assert!(
        wait_for(
            &mut host,
            |hub| {
                let providers = hub.providers_of("echo.ping");
                providers.iter().any(|id| id == "py-endpoint")
                    && providers.iter().any(|id| id == "ts-endpoint")
            },
            Duration::from_secs(25),
        ),
        "both language endpoints must announce the same capability"
    );

    // 一次 publish 广播给两个语言端点，两个端点各自回 event.echo_ack 给 Rust 端点。
    cli.publish("event.diag", json!({ "text": "hello" }));
    let mut senders = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while senders.len() < 2 {
        host.pump_until_idle(200);
        while let Some(frame) = cli.try_recv() {
            if let BusFrame::Event { topic, payload, from } = frame {
                assert_eq!(topic, "event.echo_ack");
                assert_eq!(payload["from"], "cli");
                senders.push(from);
            }
        }
        if Instant::now() >= deadline {
            panic!("only received acks from {senders:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    senders.sort();
    assert_eq!(senders, vec!["py-endpoint", "ts-endpoint"]);

    // 同一 capability 有两个 provider，Hub 按 round-robin 路由，发起方不需要指定语言。
    let mut answered_by = Vec::new();
    for index in 0..2 {
        let _id = cli.request("echo.upper", json!({ "text": format!("msg-{index}") }));
        match wait_frame(&mut host, &cli, Duration::from_secs(15)) {
            BusFrame::Response { result, error, .. } => {
                assert!(error.is_none(), "unexpected error: {error:?}");
                let result = result.expect("result");
                assert_eq!(result["text"], format!("MSG-{index}"));
                answered_by.push(result["endpointId"].as_str().unwrap().to_string());
            }
            other => panic!("expected response, got {other:?}"),
        }
    }
    answered_by.sort();
    assert_eq!(answered_by, vec!["py-endpoint", "ts-endpoint"]);

    // 没有 provider 的能力由 Hub 直接回错。
    let _id = cli.request("missing.cap", json!({}));
    match wait_frame(&mut host, &cli, Duration::from_secs(10)) {
        BusFrame::Response { error, .. } => {
            assert_eq!(error.expect("error").code, E_METHOD_NOT_FOUND);
        }
        other => panic!("expected error response, got {other:?}"),
    }
}
