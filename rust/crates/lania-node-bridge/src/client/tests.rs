use std::{
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::json;

use super::process::{
    bridge_launch_spec_for_dir, bridge_package_dir_for_test, with_bridge_env_lock,
};
use super::{BridgeClientConfig, NodeBridgeClient};
use crate::protocol::BridgeEventMethod;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    std::env::temp_dir().join(format!("lania-node-bridge-{name}-{unique}"))
}

#[test]
fn prefers_dist_bridge_assets_for_installed_mode() {
    let root = temp_dir("dist-launch");
    fs::create_dir_all(root.join("dist/entry")).expect("dist dir created");
    fs::write(root.join("dist/entry/stdio.js"), "console.log('bridge');")
        .expect("dist entry written");

    let spec = bridge_launch_spec_for_dir(&root).expect("launch spec resolves");

    assert_eq!(spec.package_dir, root);
    assert_eq!(spec.program, PathBuf::from("node"));
    assert_eq!(
        spec.args,
        vec![
            "--import".to_string(),
            "tsx".to_string(),
            spec.package_dir
                .join("dist/entry/stdio.js")
                .display()
                .to_string(),
        ]
    );
    assert_eq!(spec.mode, "installed_dist");

    let _ = fs::remove_dir_all(spec.package_dir);
}

#[test]
fn falls_back_to_tsx_in_development_mode() {
    let root = temp_dir("tsx-launch");
    fs::create_dir_all(root.join("src/entry")).expect("src dir created");
    fs::write(root.join("src/entry/stdio.ts"), "console.log('bridge');").expect("entry written");

    let spec = bridge_launch_spec_for_dir(&root).expect("launch spec resolves");

    assert_eq!(spec.program, PathBuf::from("node"));
    assert_eq!(
        spec.args,
        vec![
            "--import".to_string(),
            "tsx".to_string(),
            root.join("src/entry/stdio.ts").display().to_string(),
        ]
    );
    assert_eq!(spec.mode, "dev_source");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn respects_env_override_for_bridge_package_dir() {
    let root = temp_dir("bridge-dir-override");
    fs::create_dir_all(&root).expect("root dir created");
    with_bridge_env_lock(|| {
        env::set_var("LANIA_NODE_BRIDGE_DIR", &root);
        let resolved = bridge_package_dir_for_test().expect("bridge dir resolves");
        env::remove_var("LANIA_NODE_BRIDGE_DIR");
        assert_eq!(resolved, root);
    });
    let _ = fs::remove_dir_all(root);
}

#[test]
fn generates_incrementing_request_ids() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let first = client.ping_request();
    let second = client.compiler_stop_request();

    assert_eq!(first.id, "req-1");
    assert_eq!(second.id, "req-2");
}

#[test]
fn builds_domain_specific_requests() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let request = client.lint_run_request("/repo", true, Some(4));

    assert_eq!(request.method, "lint.run");
    assert_eq!(request.params["cwd"], "/repo");
    assert_eq!(request.params["fix"], true);
    assert_eq!(request.params["concurrency"], 4);

    let system_request =
        client.system_list_commands_request("/repo", Some("ts".into()), Some(10), true, false);
    assert_eq!(system_request.method, "system.listCommands");
    assert_eq!(system_request.params["cwd"], "/repo");
    assert_eq!(system_request.params["filter"], "ts");
    assert_eq!(system_request.params["limit"], 10);
    assert_eq!(system_request.params["allMatches"], true);
    assert_eq!(system_request.params["includeShell"], false);
}

#[test]
fn returns_structured_events_for_dev_request() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let exchange = client.call(client.compiler_dev_request("/repo", Some(3001)));

    assert!(exchange.response.error.is_none());
    assert_eq!(exchange.events.len(), 2);
    assert!(matches!(
        exchange.events[1].method,
        BridgeEventMethod::DevUrl
    ));
}

#[test]
fn template_questions_respect_skip_git_and_skip_install_options() {
    let client = NodeBridgeClient::new(BridgeClientConfig::default());

    let visible =
        client.call(client.template_questions_request("spa-react", json!({ "skipGit": false })));
    let visible_questions = visible.response.result.expect("questions payload");
    let names: Vec<&str> = visible_questions["questions"]
        .as_array()
        .expect("questions array")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(names.contains(&"packageManager"));
    assert!(names.contains(&"repository"));

    let hidden = client.call(client.template_questions_request(
        "spa-react",
        json!({
            "skipGit": true,
            "skipInstall": true,
            "skipInstallSpecified": true,
            "packageManager": "npm"
        }),
    ));
    let hidden_questions = hidden.response.result.expect("questions payload");
    let hidden_names: Vec<&str> = hidden_questions["questions"]
        .as_array()
        .expect("questions array")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(!hidden_names.contains(&"packageManager"));
    assert!(!hidden_names.contains(&"repository"));

    let zh_exchange = client.call(
        client.template_questions_request("spa-react", json!({ "locale": "zh", "skipGit": false })),
    );
    let zh_questions = zh_exchange.response.result.expect("questions payload");
    assert_eq!(
        zh_questions["questions"][0]["message"].as_str(),
        Some("请选择 CSS 预处理器：")
    );
}

#[test]
fn supports_shutdown_exchange() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let exchange = client.call(client.shutdown_request());

    assert!(exchange.response.error.is_none());
    assert!(matches!(
        exchange.events[0].method,
        BridgeEventMethod::Shutdown
    ));
}

#[test]
fn loads_lan_config_snapshot() {
    let root = std::env::temp_dir().join(format!(
        "lania-node-bridge-config-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should work")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp dir created");
    std::fs::write(
        root.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    )
    .expect("lan config written");

    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let exchange = client.call(client.load_lan_config_request(root.display().to_string()));

    assert!(exchange.response.error.is_none());
    assert_eq!(
        exchange.response.result.as_ref().unwrap()["configPath"],
        "lan.config.js"
    );
    assert_eq!(exchange.response.result.as_ref().unwrap()["exists"], true);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn smoke_tests_build_request() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let exchange = client.call(client.compiler_build_with_options_request(
        "/repo",
        true,
        Some("development".into()),
        Some("dist-custom".into()),
    ));

    assert!(exchange.response.error.is_none());
    assert_eq!(exchange.response.result.as_ref().unwrap()["watch"], true);
    assert!(exchange
        .events
        .iter()
        .any(|event| matches!(event.method, BridgeEventMethod::BuildAsset)));
}

#[test]
fn returns_structured_lint_result() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let exchange = client.call(client.lint_run_request("/repo", true, Some(2)));

    assert!(exchange.response.error.is_none());
    assert_eq!(exchange.response.result.as_ref().unwrap()["fix"], true);
    assert_eq!(
        exchange.response.result.as_ref().unwrap()["summary"]["errors"],
        0
    );
    assert!(exchange
        .events
        .iter()
        .any(|event| matches!(event.method, BridgeEventMethod::LintStart)));
    assert!(exchange
        .events
        .iter()
        .any(|event| matches!(event.method, BridgeEventMethod::LintFile)));
    assert!(exchange
        .events
        .iter()
        .any(|event| matches!(event.method, BridgeEventMethod::LintResult)));
    assert!(exchange
        .events
        .iter()
        .any(|event| matches!(event.method, BridgeEventMethod::LintSummary)));
}

#[test]
fn returns_structured_system_command_list() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let exchange =
        client.call(client.system_list_commands_request("/repo", None, Some(2), false, true));

    assert!(exchange.response.error.is_none());
    assert_eq!(
        exchange.response.result.as_ref().unwrap()["kind"],
        json!("system_commands")
    );
    assert!(exchange.response.result.as_ref().unwrap()["commands"].is_array());
    assert_eq!(
        exchange.response.result.as_ref().unwrap()["includeShell"],
        json!(true)
    );
    assert_eq!(
        exchange.response.result.as_ref().unwrap()["limit"],
        json!(2)
    );
}

#[tokio::test]
async fn streams_bridge_events_before_collecting_response() {
    let client = NodeBridgeClient::new(BridgeClientConfig::default());
    let request = client.compiler_build_with_options_request(
        "/repo",
        true,
        Some("development".into()),
        Some("dist-custom".into()),
    );
    let mut call = client.open_call(request).expect("bridge call opens");
    let first_event = call.next_event().await.expect("receives event");
    let exchange = call.collect_exchange().await.expect("collects exchange");

    let _ = first_event;
    assert_eq!(exchange.response.result.as_ref().unwrap()["watch"], true);
    assert!(!exchange.events.is_empty());
    assert!(exchange
        .events
        .iter()
        .any(|event| matches!(event.method, BridgeEventMethod::BuildAsset)));
    client.shutdown_async().await.expect("bridge shuts down");
}

#[test]
fn renders_template_files() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let exchange = client.call(client.template_render_request(
        "spa-react",
        serde_json::json!({"projectName": "demo-app"}),
        serde_json::json!({
            "projectName": "demo-app",
            "name": "demo-app",
            "cssProcessor": "less",
            "lintTools": ["eslint", "prettier"],
            "buildTool": "vite",
            "packageManager": "npm",
            "skipGit": false,
        }),
    ));

    assert!(exchange.response.error.is_none());
    assert_eq!(
        exchange.response.result.as_ref().unwrap()["template"],
        "spa-react"
    );
    let files = exchange.response.result.as_ref().unwrap()["files"]
        .as_array()
        .expect("files array");
    let paths = files
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"src/App.less"));
    assert!(!paths.contains(&"src/App.css"));
    assert!(paths.contains(&"eslint.config.js"));

    let deps = client.call(client.template_dependencies_request(
        "spa-react",
        serde_json::json!({
            "cssProcessor": "less",
            "lintTools": ["eslint", "prettier"],
            "buildTool": "vite",
            "packageManager": "npm",
        }),
    ));
    let dev_dependencies = deps.response.result.as_ref().unwrap()["devDependencies"]
        .as_array()
        .expect("devDependencies array")
        .iter()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>();
    assert!(dev_dependencies
        .iter()
        .any(|item| item == &"eslint" || item.starts_with("eslint@")));
    assert!(dev_dependencies.contains(&"less"));
}

#[test]
fn builds_commit_message_and_validates_it() {
    let client = NodeBridgeClient::new(BridgeClientConfig {
        prefer_process_transport: false,
        ..BridgeClientConfig::default()
    });
    let commit = client.call(client.commitizen_run_request(
        ".".to_string(),
        "feat",
        Some("sync".into()),
        "ship changes",
    ));
    let message = commit.response.result.as_ref().unwrap()["message"]
        .as_str()
        .unwrap()
        .to_string();
    let lint = client.call(client.commitlint_run_request(".".to_string(), message.clone()));

    assert_eq!(message, "feat(sync): ship changes");
    assert_eq!(lint.response.result.as_ref().unwrap()["valid"], true);
}

#[test]
fn can_attempt_process_transport() {
    let client = NodeBridgeClient::new(BridgeClientConfig::default());
    let exchange = client.call(client.ping_request());

    assert!(exchange.response.error.is_none());
    client.shutdown();
}

#[tokio::test]
async fn exposes_global_event_subscription_and_metrics() {
    let client = NodeBridgeClient::new(BridgeClientConfig::default());
    let mut subscription = client.subscribe_events();
    let _ = client
        .call_async(client.compiler_build_with_options_request(
            "/repo",
            true,
            Some("development".into()),
            Some("dist".into()),
        ))
        .await
        .expect("bridge build request succeeds");

    let event = subscription.recv().await.expect("receives global event");
    let metrics = client.metrics_snapshot();
    let reported = client
        .call_async(client.metrics_request(None))
        .await
        .expect("bridge metrics succeeds");

    assert!(matches!(
        event.method,
        BridgeEventMethod::Ready
            | BridgeEventMethod::Log
            | BridgeEventMethod::BuildAsset
            | BridgeEventMethod::Heartbeat
    ));
    assert!(metrics.requests_sent >= 1);
    assert!(metrics.responses_received >= 1);
    assert!(
        reported.response.result.as_ref().unwrap()["requests"]
            .as_u64()
            .unwrap()
            >= 1
    );
    client.shutdown_async().await.expect("bridge shuts down");
}

#[tokio::test]
async fn loads_project_local_runtime_plugin_with_policy() {
    let root = temp_dir("dynamic-plugin");
    fs::create_dir_all(&root).expect("temp dir created");
    fs::write(
        root.join("lan.config.js"),
        "export default { plugins: [{ package: './demo-plugin.js', methods: ['demo.echo'] }] };\n",
    )
    .expect("lan config written");
    fs::write(
        root.join("demo-plugin.js"),
        "export default { name: 'demo-plugin', methods: ['demo.echo'], async handle(method, params) { return { result: { echoed: params.message ?? 'missing' }, events: [{ method: 'event.log', params: { scope: 'plugin', message: 'echoed' } }] }; } };\n",
    )
    .expect("plugin file written");

    let client = NodeBridgeClient::new(BridgeClientConfig::default());
    let exchange = client
        .call_async(client.request(
            "demo.echo",
            serde_json::json!({
                "cwd": root.display().to_string(),
                "message": "hello",
            }),
        ))
        .await
        .expect("dynamic plugin request succeeds");

    assert_eq!(
        exchange.response.result.as_ref().unwrap()["echoed"],
        "hello"
    );
    assert!(exchange
        .events
        .iter()
        .any(|event| matches!(event.method, BridgeEventMethod::Log)));

    client.shutdown_async().await.expect("bridge shuts down");
    let _ = fs::remove_dir_all(root);
}
