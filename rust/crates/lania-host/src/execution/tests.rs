use serde_json::json;

use super::bridge_helpers::format_diag_line;

#[test]
fn diag_lines_are_prefixed_by_channel() {
    use lania_node_bridge::DiagChannel;

    // §7.5.3：`app` 日志必须带 `[app]` 前缀，才能与构建日志在视觉上区分。
    assert_eq!(
        format_diag_line(DiagChannel::App, "GET /api/user 200 12ms"),
        "[app] GET /api/user 200 12ms"
    );
    assert_eq!(
        format_diag_line(DiagChannel::Build, "compiling foo"),
        "[build] compiling foo"
    );
    assert_eq!(format_diag_line(DiagChannel::Dev, "ready"), "[dev] ready");
    assert_eq!(format_diag_line(DiagChannel::Lint, "ok"), "[lint] ok");
}

#[test]
fn recursively_redacts_secret_fields_in_nested_payloads() {
    let value = json!({
        "prompt": {
            "answers": {
                "password": "super-secret",
                "user": "demo"
            }
        },
        "items": [
            { "password": "inner-secret" },
            { "value": 1 }
        ]
    });

    let redacted = super::utils::redact_secret_fields(value, &["password".to_string()]);

    assert_eq!(redacted["prompt"]["answers"]["password"], json!("***"));
    assert_eq!(redacted["prompt"]["answers"]["user"], json!("demo"));
    assert_eq!(redacted["items"][0]["password"], json!("***"));
    assert_eq!(redacted["items"][1]["value"], json!(1));
}
