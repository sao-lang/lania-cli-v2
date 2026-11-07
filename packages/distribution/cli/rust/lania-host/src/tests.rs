use serde_json::json;

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

    let redacted = super::redact_secret_fields(value, &["password".to_string()]);

    assert_eq!(redacted["prompt"]["answers"]["password"], json!("***"));
    assert_eq!(redacted["prompt"]["answers"]["user"], json!("demo"));
    assert_eq!(redacted["items"][0]["password"], json!("***"));
    assert_eq!(redacted["items"][1]["value"], json!(1));
}
