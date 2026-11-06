use lania_bridge_bus::{BridgeHub, Handler, ProviderPolicy};
use lania_bridge_core::{bus_error, E_INVALID_PARAMS};
use lania_bridge_protocol::{BusErrorDetail, BusFrame, Value, E_METHOD_NOT_FOUND};
use serde_json::json;

#[derive(Default)]
struct EchoHandler {
    events: Vec<(String, Value, String)>,
    requests: Vec<(String, Value)>,
    fail_with: Option<BusErrorDetail>,
}

impl Handler for EchoHandler {
    fn on_request(&mut self, capability: &str, params: Value) -> Result<Value, BusErrorDetail> {
        self.requests.push((capability.to_string(), params.clone()));
        if let Some(err) = &self.fail_with {
            return Err(err.clone());
        }
        Ok(json!({ "capability": capability, "params": params }))
    }

    fn on_event(&mut self, topic: &str, payload: Value, from: &str) {
        self.events.push((topic.to_string(), payload, from.to_string()));
    }
}

fn pump(hub: &mut BridgeHub) {
    assert!(hub.pump_until_idle(200) < 200, "pump did not settle");
}

fn next_response(endpoint: &lania_bridge_bus::Endpoint) -> BusFrame {
    endpoint.try_recv().expect("expected a frame")
}

#[test]
fn announce_registers_capabilities_and_topics() {
    let mut hub = BridgeHub::new();
    let mut cli = hub.attach("cli");
    let mut engine = hub.attach("engine");

    cli.announce_str(&["tool.exec"], &["event.diag"]);
    engine.announce_str(&["agent.prompt", "lint.run"], &["event.agent_*"]);
    pump(&mut hub);

    assert_eq!(hub.endpoint_ids(), vec!["cli", "engine"]);
    assert_eq!(hub.providers_of("agent.prompt"), vec!["engine"]);
    assert_eq!(hub.providers_of("lint.run"), vec!["engine"]);
    assert_eq!(hub.subscribers_of("event.diag"), vec!["cli"]);
    assert_eq!(hub.subscribers_of("event.agent_start"), vec!["engine"]);
    assert!(hub.providers_of("missing.cap").is_empty());
}

#[test]
fn broadcast_fans_out_but_excludes_publisher_by_default() {
    let mut hub = BridgeHub::new();
    let mut publisher = hub.attach("cli");
    let mut first = hub.attach("subscriber-a");
    let mut second = hub.attach("subscriber-b");
    let mut silent = hub.attach("silent");

    publisher.announce_str(&[], &["event.diag"]);
    first.announce_str(&[], &["event.diag"]);
    second.announce_str(&[], &["event.agent_*"]);
    silent.announce_str(&[], &[]);
    pump(&mut hub);

    publisher.publish("event.agent_start", json!({ "text": "hello" }));
    pump(&mut hub);

    match next_response(&second) {
        BusFrame::Event { topic, payload, from } => {
            assert_eq!(topic, "event.agent_start");
            assert_eq!(payload, json!({ "text": "hello" }));
            assert_eq!(from, "cli");
        }
        other => panic!("expected event, got {other:?}"),
    }
    assert!(first.try_recv().is_none(), "non-matching subscriber got an event");
    assert!(silent.try_recv().is_none(), "non-subscriber got an event");
    assert!(publisher.try_recv().is_none(), "publisher received its own event");
    assert_eq!(hub.diagnostics().events_delivered, 1);

    // 通配订阅与普通订阅都命中时 fan-out 到两个端点，且发布者仍不收到。
    // 订阅必须先被 Hub 处理，再发布（总线不保证同一批次内的跨端点先后）。
    second.subscribe("event.diag");
    pump(&mut hub);
    publisher.publish("event.diag", json!({ "n": 1 }));
    pump(&mut hub);
    let mut handler = EchoHandler::default();
    assert_eq!(first.serve_until_idle(&mut handler, 10), 1);
    assert_eq!(second.serve_until_idle(&mut handler, 10), 1);
    assert_eq!(handler.events.len(), 2);
    assert!(handler.events.iter().all(|(topic, _, from)| topic == "event.diag" && from == "cli"));
}

#[test]
fn request_routes_to_provider_and_response_returns_to_origin() {
    let mut hub = BridgeHub::new();
    let mut cli = hub.attach("cli");
    let mut engine = hub.attach("engine");

    cli.announce_str(&["tool.exec"], &[]);
    engine.announce_str(&["agent.prompt"], &[]);
    pump(&mut hub);

    let id = cli.request("agent.prompt", json!({ "text": "hi" }));
    pump(&mut hub);

    let mut handler = EchoHandler::default();
    assert_eq!(engine.serve_until_idle(&mut handler, 10), 1);
    assert_eq!(handler.requests.len(), 1);
    assert_eq!(handler.requests[0].0, "agent.prompt");
    pump(&mut hub);

    match next_response(&cli) {
        BusFrame::Response { id: response_id, result, error } => {
            assert_eq!(response_id, id);
            assert!(error.is_none());
            assert_eq!(result, Some(json!({ "capability": "agent.prompt", "params": { "text": "hi" } })));
        }
        other => panic!("expected response, got {other:?}"),
    }
    assert_eq!(hub.pending_len(), 0);
    assert_eq!(hub.diagnostics().requests_routed, 1);
    assert_eq!(hub.diagnostics().responses_routed, 1);
}

#[test]
fn no_provider_returns_method_not_found() {
    let mut hub = BridgeHub::new();
    let mut cli = hub.attach("cli");
    cli.announce_str(&[], &[]);
    pump(&mut hub);

    let id = cli.request("missing.cap", json!({}));
    pump(&mut hub);

    match next_response(&cli) {
        BusFrame::Response { id: response_id, result, error } => {
            assert_eq!(response_id, id);
            assert!(result.is_none());
            assert_eq!(error.expect("error").code, E_METHOD_NOT_FOUND);
        }
        other => panic!("expected error response, got {other:?}"),
    }
    assert_eq!(hub.diagnostics().errors_emitted, 1);
}

#[test]
fn provider_error_is_forwarded_unchanged() {
    let mut hub = BridgeHub::new();
    let mut cli = hub.attach("cli");
    let mut engine = hub.attach("engine");
    cli.announce_str(&[], &[]);
    engine.announce_str(&["lint.run"], &[]);
    pump(&mut hub);

    let id = cli.request("lint.run", json!({}));
    pump(&mut hub);
    let mut handler = EchoHandler {
        fail_with: Some(bus_error(E_INVALID_PARAMS, "bad params")),
        ..Default::default()
    };
    engine.serve_until_idle(&mut handler, 10);
    pump(&mut hub);

    match next_response(&cli) {
        BusFrame::Response { id: response_id, error, .. } => {
            assert_eq!(response_id, id);
            assert_eq!(error.expect("error").code, E_INVALID_PARAMS);
        }
        other => panic!("expected response, got {other:?}"),
    }
}

#[test]
fn round_robin_balances_multiple_providers() {
    let mut hub = BridgeHub::with_policy(ProviderPolicy::RoundRobin);
    let mut cli = hub.attach("cli");
    let mut first = hub.attach("provider-1");
    let mut second = hub.attach("provider-2");
    cli.announce_str(&[], &[]);
    first.announce_str(&["echo.ping"], &[]);
    second.announce_str(&["echo.ping"], &[]);
    pump(&mut hub);

    assert_eq!(hub.providers_of("echo.ping").len(), 2);
    cli.request("echo.ping", json!({ "n": 1 }));
    cli.request("echo.ping", json!({ "n": 2 }));
    cli.request("echo.ping", json!({ "n": 3 }));
    pump(&mut hub);

    let mut handler = EchoHandler::default();
    let served_first = first.serve_until_idle(&mut handler, 10);
    let served_second = second.serve_until_idle(&mut handler, 10);
    assert_eq!(served_first + served_second, 3);
    assert!(served_first > 0 && served_second > 0, "round-robin must use both providers");
    assert_eq!(hub.diagnostics().requests_routed, 3);

    hub.set_policy(ProviderPolicy::First);
    cli.request("echo.ping", json!({ "n": 4 }));
    pump(&mut hub);
    let first_extra = first.serve_until_idle(&mut handler, 10);
    let second_extra = second.serve_until_idle(&mut handler, 10);
    assert_eq!(first_extra + second_extra, 1);
    let preferred = hub.providers_of("echo.ping").into_iter().next().unwrap();
    if preferred == "provider-1" {
        assert_eq!((first_extra, second_extra), (1, 0));
    } else {
        assert_eq!((first_extra, second_extra), (0, 1));
    }
}

#[test]
fn dynamic_subscribe_unsubscribe_changes_delivery() {
    let mut hub = BridgeHub::new();
    let mut publisher = hub.attach("cli");
    let mut listener = hub.attach("listener");
    publisher.announce_str(&[], &[]);
    listener.announce_str(&[], &[]);
    pump(&mut hub);

    publisher.publish("event.diag", json!({ "n": 1 }));
    pump(&mut hub);
    assert!(listener.try_recv().is_none());

    listener.subscribe("event.diag");
    pump(&mut hub);
    publisher.publish("event.diag", json!({ "n": 2 }));
    pump(&mut hub);
    assert!(matches!(next_response(&listener), BusFrame::Event { .. }));

    listener.unsubscribe("event.diag");
    pump(&mut hub);
    publisher.publish("event.diag", json!({ "n": 3 }));
    pump(&mut hub);
    assert!(listener.try_recv().is_none());
}

#[test]
fn bye_detaches_endpoint_and_cleans_indexes() {
    let mut hub = BridgeHub::new();
    let mut engine = hub.attach("engine");
    engine.announce_str(&["agent.prompt"], &["event.diag"]);
    pump(&mut hub);
    assert_eq!(hub.providers_of("agent.prompt"), vec!["engine"]);

    engine.bye();
    pump(&mut hub);

    assert!(hub.endpoint_ids().is_empty());
    assert!(hub.providers_of("agent.prompt").is_empty());
    assert!(hub.subscribers_of("event.diag").is_empty());
    assert_eq!(hub.diagnostics().endpoints_removed, 1);
}

#[test]
fn disconnected_endpoint_is_removed_automatically() {
    let mut hub = BridgeHub::new();
    {
        let mut engine = hub.attach("engine");
        engine.announce_str(&["agent.prompt"], &[]);
        pump(&mut hub);
    } // endpoint 被 drop，进程内 channel 断开
    hub.pump_until_idle(20);
    assert!(hub.endpoint_ids().is_empty());
    assert!(hub.providers_of("agent.prompt").is_empty());
}

#[test]
fn pending_requests_are_cleaned_when_origin_leaves() {
    let mut hub = BridgeHub::new();
    let mut cli = hub.attach("cli");
    let mut engine = hub.attach("engine");
    cli.announce_str(&[], &[]);
    engine.announce_str(&["agent.prompt"], &[]);
    pump(&mut hub);

    cli.request("agent.prompt", json!({}));
    pump(&mut hub);
    assert_eq!(hub.pending_len(), 1);

    cli.bye();
    pump(&mut hub);
    assert_eq!(hub.pending_len(), 0);
}

#[test]
fn frames_on_the_bus_never_contain_language_fields() {
    let mut hub = BridgeHub::new();
    let mut cli = hub.attach("cli");
    let mut engine = hub.attach("engine");
    cli.announce_str(&["tool.exec"], &[]);
    engine.announce_str(&["agent.prompt"], &[]);
    pump(&mut hub);
    cli.request("agent.prompt", json!({ "text": "hi" }));
    cli.publish("event.diag", json!({ "text": "hello" }));
    pump(&mut hub);

    let mut frames = Vec::new();
    while let Some(frame) = engine.try_recv() {
        frames.push(frame);
    }
    while let Some(frame) = cli.try_recv() {
        frames.push(frame);
    }
    assert!(!frames.is_empty());
    for frame in frames {
        let encoded = lania_bridge_protocol::encode_frame(&frame).unwrap();
        let object = serde_json::from_str::<serde_json::Value>(&encoded)
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        for forbidden in ["language", "lang", "runtime", "runtimeKind", "RuntimeKind", "protocolVersion"] {
            assert!(!object.contains_key(forbidden), "frame {encoded} contains {forbidden}");
        }
    }
}
