//! `host.interaction.*` 的 RPC 处理实现（从 `runtime/host_rpc.rs` 拆出的子模块）。
//!
//! 设计说明：
//! - 这是最早被拆到子模块的一组 host-rpc handler。
//! - 该模块尽量自洽：payload 的解析与 wire -> prompt DSL 转换放在这里，
//!   父文件只保留“路由/策略/审计”等公共职责，避免 host_rpc.rs 继续膨胀。

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use lania_prompt::{PromptFallbackStrategy, PromptFlow, PromptRunOptions, PromptState};
use serde_json::Value;

use super::{HostPayload, HostRpcAdapter, HostRpcResponse};
use crate::runtime::dynamic::prompt_step_from_wire;

/// interaction 域的 handler 入口：
/// - 单步输入（input/confirm/select/...）会被统一归约成一个 step 并执行
/// - flow/prompt 会执行完整的 PromptFlow 并返回 `PromptState`
pub(super) fn handle_interaction_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<HostRpcResponse> {
    match method {
        "host.interaction.input"
        | "host.interaction.confirm"
        | "host.interaction.select"
        | "host.interaction.multiSelect"
        | "host.interaction.password"
        | "host.interaction.editor" => {
            let kind = method
                .strip_prefix("host.interaction.")
                .ok_or_else(|| anyhow!("invalid interaction method: {method}"))?;
            let answer = run_prompt_step(&adapter.prompt, kind, payload)?;
            Ok((serde_json::json!({ "answer": answer }), Vec::new()))
        }
        "host.interaction.prompt" | "host.interaction.flow.execute" => {
            let state = run_prompt_flow(&adapter.prompt, payload)?;
            Ok((serde_json::to_value(state)?, Vec::new()))
        }
        "host.interaction.resetAccumulated" => {
            adapter.prompt.reset_accumulated();
            Ok((serde_json::json!({ "ok": true }), Vec::new()))
        }
        other => Err(anyhow!("unsupported host rpc method: {other}")),
    }
}

fn run_prompt_step(
    prompt: &lania_prompt::PromptService,
    kind: &str,
    payload: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let mut step = prompt_step_from_wire(
        &single_step_wire(kind, payload),
        &lania_command::ParsedArgv::default(),
        payload
            .get("locale")
            .and_then(Value::as_str)
            .unwrap_or("en"),
    )
    .ok_or_else(|| anyhow!("failed to build prompt step from interaction payload"))?;
    if step.id.is_empty() {
        step.id = step.field.clone();
    }
    let flow = PromptFlow::new().step(step);
    let state = prompt.run_cli_with_options(
        &flow,
        PromptRunOptions {
            context: value_object_to_btreemap(payload.get("context")),
            answers: value_object_to_btreemap(payload.get("answers")),
            fallback: parse_prompt_fallback(payload.get("fallback")),
            accumulate: payload
                .get("accumulate")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            reset_accumulated: payload
                .get("resetAccumulated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            ..PromptRunOptions::default()
        },
    )?;
    Ok(state
        .answers
        .get(kind_step_field(payload))
        .cloned()
        .unwrap_or(Value::Null))
}

fn run_prompt_flow(
    prompt: &lania_prompt::PromptService,
    payload: &serde_json::Map<String, Value>,
) -> Result<PromptState> {
    let locale = payload
        .get("locale")
        .and_then(Value::as_str)
        .unwrap_or("en");
    let flow = build_prompt_flow(
        payload
            .get("questions")
            .or_else(|| payload.get("steps"))
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("interaction flow requires `questions` or `steps` array"))?,
        locale,
    )?;
    let mut options = PromptRunOptions {
        context: value_object_to_btreemap(payload.get("context")),
        answers: value_object_to_btreemap(payload.get("answers")),
        fallback: parse_prompt_fallback(payload.get("fallback")),
        accumulate: payload
            .get("accumulate")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        reset_accumulated: payload
            .get("resetAccumulated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ..PromptRunOptions::default()
    };
    if let Some(resume_from) = payload.get("resumeFrom") {
        options.resume_from = Some(serde_json::from_value(resume_from.clone())?);
    }
    prompt.run_cli_with_options(&flow, options)
}

fn build_prompt_flow(items: &[Value], locale: &str) -> Result<PromptFlow> {
    let argv = lania_command::ParsedArgv::default();
    let mut flow = PromptFlow::new();
    for item in items {
        let step = prompt_step_from_wire(item, &argv, locale)
            .ok_or_else(|| anyhow!("failed to parse interaction question"))?;
        flow = flow.step(step);
    }
    Ok(flow)
}

fn single_step_wire(kind: &str, payload: &serde_json::Map<String, Value>) -> Value {
    let message = payload
        .get("message")
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()));
    let field = kind_step_field(payload).to_string();
    let mut object = serde_json::Map::new();
    object.insert(
        "id".into(),
        payload
            .get("id")
            .cloned()
            .unwrap_or_else(|| Value::String(field.clone())),
    );
    object.insert("field".into(), Value::String(field));
    object.insert("message".into(), message);
    object.insert(
        "kind".into(),
        Value::String(kind_to_wire_kind(kind).to_string()),
    );
    for key in [
        "choices",
        "defaultValue",
        "when",
        "goto",
        "validate",
        "timeoutMs",
        "contextKey",
        "accumulation",
        "returnable",
        "detail",
        "mapFunctions",
        "onAnswered",
        "whenMissing",
    ] {
        if let Some(value) = payload.get(key) {
            object.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(object)
}

fn kind_step_field(payload: &serde_json::Map<String, Value>) -> &str {
    payload
        .get("field")
        .and_then(Value::as_str)
        .or_else(|| payload.get("name").and_then(Value::as_str))
        .unwrap_or("value")
}

fn kind_to_wire_kind(kind: &str) -> &str {
    match kind {
        "multiSelect" => "multi_select",
        other => other,
    }
}

fn parse_prompt_fallback(value: Option<&Value>) -> Option<PromptFallbackStrategy> {
    let value = value?;
    let record = value.as_object()?;
    let kind = record
        .get("type")
        .and_then(Value::as_str)?
        .to_ascii_lowercase();
    match kind.as_str() {
        "use_default" | "use_defaults" => Some(PromptFallbackStrategy::UseDefault),
        "skip" => Some(PromptFallbackStrategy::Skip),
        "error" => Some(PromptFallbackStrategy::Error),
        "use_value" => Some(PromptFallbackStrategy::UseValue(
            record.get("value").cloned().unwrap_or(Value::Null),
        )),
        _ => None,
    }
}

fn value_object_to_btreemap(value: Option<&Value>) -> BTreeMap<String, Value> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}
