//! 动态命令 prompt 解析与执行辅助逻辑。
//!
//! 这里处理两类事情：
//! - 运行时是否需要在宿主侧补问，以及补问后的答案如何回写 argv
//! - Node/config 传来的 prompt wire 格式如何转换为 `PromptFlow`

use std::{collections::BTreeMap, io::IsTerminal};

use anyhow::Result;
use lania_command::ParsedArgv;
use lania_hooks::hook_keys;
use lania_prompt::{
    AccumulationMode, OnAnsweredAction, PromptFallbackStrategy, PromptFlow, PromptMapFunction,
    PromptRunOptions, PromptStep, PromptStepKind, ValidationRule, WhenCondition,
};
use serde_json::{json, Value};

use crate::execution::CommandExecutionContext;

pub(in crate::runtime) async fn maybe_prompt_dynamic_command(
    ctx: &CommandExecutionContext<'_>,
    method: &str,
    target: &Value,
    argv: &mut ParsedArgv,
) -> Result<()> {
    if method != "command.invokeDynamic" {
        return Ok(());
    }
    let kind = target.get("kind").and_then(Value::as_str).unwrap_or("");
    if kind != "openapi_operation" && kind != "manifest_command" {
        return Ok(());
    }

    // 动态命令的交互策略完全复用项目配置里的 interaction 设置，
    // 这样“内置命令”和“动态命令”在交互行为上保持一致。
    let interaction_mode = ctx
        .project_config()
        .map(|config| config.ui.interaction.mode.as_str())
        .unwrap_or("auto");
    let default_strategy = ctx
        .project_config()
        .map(|config| config.ui.interaction.default_strategy.as_str())
        .unwrap_or("use_defaults");
    let interactive_tty = std::io::stdin().is_terminal();

    // `non_interactive` 的语义是“绝不弹交互问题”。
    if interaction_mode == "non_interactive" {
        return Ok(());
    }
    // `auto + 非 TTY + fail` 的组合也意味着“绝不弹交互问题”。
    if interaction_mode == "auto" && !interactive_tty && default_strategy == "fail" {
        return Ok(());
    }

    let interaction_timeout_ms = ctx
        .project_config()
        .and_then(|config| config.ui.interaction.timeout_ms);
    let mut steps = prompt_steps_from_target(target, argv, ctx.locale());
    // 超时配置是“批量灌到每个 step 上”的，而不是在 PromptService 外层包一个总超时。
    if let Some(timeout_ms) = interaction_timeout_ms {
        steps = steps
            .into_iter()
            .map(|step| {
                if step.timeout_ms.is_some() {
                    step
                } else {
                    step.timeout_ms(timeout_ms)
                }
            })
            .collect();
    }
    if steps.is_empty() {
        return Ok(());
    }
    let secret_fields = steps
        .iter()
        .filter(|step| matches!(step.kind, PromptStepKind::Password))
        .map(|step| step.field.clone())
        .collect::<Vec<_>>();

    let prompt_before: Value = ctx
        .hooks()
        .call_waterfall(
            "host-runtime".to_string(),
            hook_keys::ON_INTERACTION_PROMPT.to_string(),
            json!({
                "cwd": ctx.command().cwd,
                "traceId": ctx.command().trace_id,
                "command": { "name": ctx.command().handler_id, "handlerId": ctx.command().handler_id },
                "prompt": {
                    "steps": steps.iter().map(|step| json!({
                        "id": step.id,
                        "field": step.field,
                        "kind": format!("{:?}", step.kind).to_ascii_lowercase(),
                        "message": step.message
                    })).collect::<Vec<_>>()
                }
            }),
        )
        .await
        .unwrap_or_else(|_| json!({}));
    if let Some(answers) = prompt_before
        .get("prompt")
        .and_then(|prompt| prompt.get("answers"))
        .and_then(Value::as_object)
    {
        for (key, value) in answers {
            argv.options.insert(key.clone(), value.clone());
        }
        return Ok(());
    }

    // 允许通过环境变量注入“脚本化答案”，适配 CI / 非 TTY 场景。
    let scripted_answers = read_prompt_answers_from_env();

    let flow = steps
        .into_iter()
        .fold(PromptFlow::new(), |flow, step| flow.step(step));
    let fallback = if interaction_mode == "auto" && !interactive_tty {
        Some(if default_strategy == "use_defaults" {
            PromptFallbackStrategy::UseDefault
        } else {
            PromptFallbackStrategy::Error
        })
    } else if interaction_mode == "interactive" && !interactive_tty {
        // 用户显式要求 interactive，但环境却不是 TTY，这里宁可报错也不静默降级。
        Some(PromptFallbackStrategy::Error)
    } else {
        None
    };

    let mut initial_answers = argv.options.clone();
    initial_answers.extend(scripted_answers);

    let state = {
        let _progress_guard = ctx.progress().suspend_terminal_guard();
        // prompt 前先暂停终端进度条，避免“问题文本”和 spinner/progress bar 抢 stderr。
        ctx.prompt().run_cli_with_options(
            &flow,
            PromptRunOptions {
                context: prompt_context_from_argv(argv),
                answers: initial_answers,
                fallback,
                ..PromptRunOptions::default()
            },
        )?
    };

    for (key, value) in state.answers {
        argv.options.entry(key).or_insert(value);
    }
    // 注意这里要先做脱敏，再把答案发送给 hook。
    let redacted_answers = redact_secret_answer_map(&argv.options, &secret_fields);

    let prompt_after: Value = ctx
        .hooks()
        .call_waterfall(
            "host-runtime".to_string(),
            hook_keys::ON_INTERACTION_PROMPT.to_string(),
            json!({
                "cwd": ctx.command().cwd,
                "traceId": ctx.command().trace_id,
                "command": { "name": ctx.command().handler_id, "handlerId": ctx.command().handler_id },
                "prompt": {
                    "steps": [],
                    "answers": redacted_answers
                }
            }),
        )
        .await
        .unwrap_or_else(|_| json!({}));
    if let Some(answers) = prompt_after
        .get("prompt")
        .and_then(|prompt| prompt.get("answers"))
        .and_then(Value::as_object)
    {
        argv.options = answers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
    }

    Ok(())
}

pub(in crate::runtime) fn prompt_context_from_argv(argv: &ParsedArgv) -> BTreeMap<String, Value> {
    let mut context = argv.args.clone();
    context.extend(argv.options.clone());
    context
}

pub(in crate::runtime) fn redact_secret_answer_map(
    answers: &BTreeMap<String, Value>,
    secret_fields: &[String],
) -> BTreeMap<String, Value> {
    answers
        .iter()
        .map(|(key, value)| {
            if secret_fields.iter().any(|field| field == key) {
                (key.clone(), json!("***"))
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect()
}

fn read_prompt_answers_from_env() -> BTreeMap<String, Value> {
    let mut answers = BTreeMap::new();
    let Ok(raw) = std::env::var("LANIA_PROMPT_ANSWERS_JSON") else {
        return answers;
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return answers;
    };
    let Some(object) = value.as_object() else {
        return answers;
    };
    for (key, value) in object {
        answers.insert(key.clone(), value.clone());
    }
    answers
}

fn prompt_steps_from_target(target: &Value, argv: &ParsedArgv, locale: &str) -> Vec<PromptStep> {
    let mut steps = Vec::new();

    // 1) explicit prompt steps from target.prompt[]
    if let Some(items) = target.get("prompt").and_then(Value::as_array) {
        for item in items {
            if let Some(step) = prompt_step_from_wire(item, argv, locale) {
                steps.push(step);
            }
        }
    }

    // 2) auto steps for missing required inputs
    // 这一步体现了“动态命令 schema 驱动交互”的思路。
    if target.get("kind").and_then(Value::as_str) == Some("openapi_operation") {
        if let Some(parameters) = target.get("parameters").and_then(Value::as_array) {
            for parameter in parameters {
                let name = parameter.get("name").and_then(Value::as_str).unwrap_or("");
                let required = parameter
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !required || name.is_empty() {
                    continue;
                }
                if argv.options.contains_key(name) {
                    continue;
                }
                steps.push(PromptStep::new(
                    name,
                    if locale == "zh" {
                        format!("缺少必填参数 --{name}，请输入：")
                    } else {
                        format!("Missing required option --{name}. Please enter a value:")
                    },
                    name,
                ));
            }
        }
    }
    if target.get("kind").and_then(Value::as_str) == Some("manifest_command") {
        if let Some(required) = target.get("requiredOptions").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if name.is_empty() || argv.options.contains_key(name) {
                    continue;
                }
                steps.push(PromptStep::new(
                    name,
                    if locale == "zh" {
                        format!("缺少必填参数 --{name}，请输入：")
                    } else {
                        format!("Missing required option --{name}. Please enter a value:")
                    },
                    name,
                ));
            }
        }
    }

    steps
}

pub(in crate::runtime) fn prompt_step_from_wire(
    item: &Value,
    argv: &ParsedArgv,
    locale: &str,
) -> Option<PromptStep> {
    let field = item.get("field").and_then(Value::as_str)?;
    if argv.options.contains_key(field) {
        return None;
    }
    let message = localized_text(item.get("message")?, locale)?;
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or(field)
        .to_string();

    if let Some(when_missing) = item.get("whenMissing").and_then(Value::as_array) {
        let needs = when_missing
            .iter()
            .filter_map(Value::as_str)
            .any(|key| !argv.options.contains_key(key));
        if !needs {
            return None;
        }
    }

    let mut step = PromptStep::new(id, message, field.to_string());

    if let Some(detail) = item.get("detail") {
        if let Some(detail) = localized_text(detail, locale) {
            step = step.detail(detail);
        }
    }

    if let Some(when) = item.get("when").and_then(parse_when_condition) {
        step = step.when(when);
    }

    if let Some(goto) = item.get("goto").and_then(Value::as_str) {
        step = step.goto(goto.to_string());
    }

    if let Some(validate) = item.get("validate").map(parse_validation_rules) {
        for rule in validate {
            step = step.validate_rule(rule);
        }
    }

    if let Some(timeout_ms) = item
        .get("timeoutMs")
        .or_else(|| item.get("timeout_ms"))
        .and_then(Value::as_u64)
    {
        step = step.timeout_ms(timeout_ms);
    }

    if let Some(context_key) = item
        .get("contextKey")
        .or_else(|| item.get("context_key"))
        .and_then(Value::as_str)
    {
        step = step.context_key(context_key.to_string());
    }

    if let Some(accumulation) = item.get("accumulation").and_then(parse_accumulation_mode) {
        step = step.accumulation(accumulation);
    }

    if item
        .get("returnable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        step = step.returnable();
    }

    // prompt spec 里可能同时出现：
    // - 新字段 `kind`
    // - 历史遗留字段 `type`
    // 这里两者都兼容读取，避免旧配置/旧 bridge payload 失效。
    if let Some(kind) = item
        .get("kind")
        .or_else(|| item.get("type"))
        .and_then(Value::as_str)
    {
        step = step.kind(match kind {
            "select" => PromptStepKind::Select,
            "confirm" => PromptStepKind::Confirm,
            "multi_select" => PromptStepKind::MultiSelect,
            "password" => PromptStepKind::Password,
            "editor" => PromptStepKind::Editor,
            "number" => PromptStepKind::Number,
            "fuzzy_select" | "autocomplete" | "search" => PromptStepKind::FuzzySelect,
            "rawlist" => PromptStepKind::RawList,
            "expand" => PromptStepKind::Expand,
            _ => PromptStepKind::Input,
        });
    }

    if let Some(default_value) = item.get("defaultValue") {
        step = step.default_value(default_value.clone());
    }

    if let Some(choices) = item.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let label = choice.get("label").and_then(Value::as_str)?;
            let value = choice.get("value").cloned().unwrap_or(Value::Null);
            step = step.choice(label.to_string(), value);
        }
    }

    if let Some(map_functions) = item
        .get("mapFunctions")
        .or_else(|| item.get("map_functions"))
        .map(parse_map_functions)
    {
        for map_function in map_functions {
            step = step.map_function(map_function);
        }
    }

    if let Some(on_answered) = item
        .get("onAnswered")
        .or_else(|| item.get("on_answered"))
        .map(parse_on_answered_actions)
    {
        for action in on_answered {
            step = step.on_answered(action);
        }
    }

    Some(step)
}

fn parse_when_condition(value: &Value) -> Option<WhenCondition> {
    let item = value.as_object()?;
    let kind = item
        .get("type")
        .and_then(Value::as_str)?
        .to_ascii_lowercase();
    let key = item.get("key").and_then(Value::as_str)?.to_string();
    match kind.as_str() {
        "equals" => Some(WhenCondition::Equals {
            key,
            value: item.get("value").cloned().unwrap_or(Value::Null),
        }),
        "not_equals" => Some(WhenCondition::NotEquals {
            key,
            value: item.get("value").cloned().unwrap_or(Value::Null),
        }),
        "exists" => Some(WhenCondition::Exists { key }),
        "truthy" => Some(WhenCondition::Truthy { key }),
        _ => None,
    }
}

fn parse_validation_rules(value: &Value) -> Vec<ValidationRule> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            if item.as_str() == Some("required") {
                return Some(ValidationRule::Required);
            }
            let record = item.as_object()?;
            let kind = record
                .get("type")
                .and_then(Value::as_str)?
                .to_ascii_lowercase();
            match kind.as_str() {
                "required" => Some(ValidationRule::Required),
                "min_length" => record
                    .get("min")
                    .or_else(|| record.get("value"))
                    .and_then(Value::as_u64)
                    .map(|min| ValidationRule::MinLength(min as usize)),
                "one_of" => {
                    let values = record
                        .get("values")
                        .or_else(|| record.get("choices"))
                        .and_then(Value::as_array)?
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>();
                    (!values.is_empty()).then_some(ValidationRule::OneOf(values))
                }
                _ => None,
            }
        })
        .collect()
}

fn parse_accumulation_mode(value: &Value) -> Option<AccumulationMode> {
    match value.as_str()?.to_ascii_lowercase().as_str() {
        "replace" => Some(AccumulationMode::Replace),
        "append" => Some(AccumulationMode::Append),
        _ => None,
    }
}

fn parse_map_functions(value: &Value) -> Vec<PromptMapFunction> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            if let Some(kind) = item.as_str() {
                return parse_map_function_kind(kind, None);
            }
            let record = item.as_object()?;
            let kind = record.get("type").and_then(Value::as_str)?;
            parse_map_function_kind(kind, Some(record))
        })
        .collect()
}

fn parse_map_function_kind(
    kind: &str,
    record: Option<&serde_json::Map<String, Value>>,
) -> Option<PromptMapFunction> {
    match kind.to_ascii_lowercase().as_str() {
        "trim" => Some(PromptMapFunction::Trim),
        "lowercase" => Some(PromptMapFunction::Lowercase),
        "uppercase" => Some(PromptMapFunction::Uppercase),
        "to_number" => Some(PromptMapFunction::ToNumber),
        "json_parse" => Some(PromptMapFunction::JsonParse),
        "split" => record
            .and_then(|record| record.get("separator"))
            .and_then(Value::as_str)
            .map(|separator| PromptMapFunction::Split {
                separator: separator.to_string(),
            }),
        _ => None,
    }
}

fn parse_on_answered_actions(value: &Value) -> Vec<OnAnsweredAction> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let record = item.as_object()?;
            let kind = record
                .get("type")
                .and_then(Value::as_str)?
                .to_ascii_lowercase();
            match kind.as_str() {
                "set_context_value" => Some(OnAnsweredAction::SetContextValue {
                    key: record.get("key").and_then(Value::as_str)?.to_string(),
                    value: record.get("value").cloned().unwrap_or(Value::Null),
                }),
                "set_context_from_answer" => Some(OnAnsweredAction::SetContextFromAnswer {
                    key: record.get("key").and_then(Value::as_str)?.to_string(),
                    field: record
                        .get("field")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    map_functions: record
                        .get("mapFunctions")
                        .or_else(|| record.get("map_functions"))
                        .map(parse_map_functions)
                        .unwrap_or_default(),
                }),
                "goto" => Some(OnAnsweredAction::Goto {
                    target: record.get("target").and_then(Value::as_str)?.to_string(),
                }),
                "goto_if" => Some(OnAnsweredAction::GotoIf {
                    when: parse_when_condition(record.get("when")?)?,
                    target: record.get("target").and_then(Value::as_str)?.to_string(),
                }),
                _ => None,
            }
        })
        .collect()
}

fn localized_text(value: &Value, locale: &str) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    let map = value.as_object()?;
    let key = if locale == "zh" { "zh" } else { "en" };
    map.get(key)
        .and_then(Value::as_str)
        .or_else(|| map.get("en").and_then(Value::as_str))
        .or_else(|| map.get("zh").and_then(Value::as_str))
        .map(|text| text.to_string())
}
