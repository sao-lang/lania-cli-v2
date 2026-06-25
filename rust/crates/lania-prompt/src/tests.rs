use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::json;

use crate::{
    parsing::{expand_short_keys, parse_prompt_input, resolve_single_choice},
    service::InteractivePromptOutcome,
    ui_terminal::prompt_text_terminal_with_timeout_using,
    AccumulationMode, OnAnsweredAction, PromptFallbackStrategy, PromptFlow, PromptMapFunction,
    PromptService, PromptStep, PromptStepKind, ValidationRule, WhenCondition,
};

#[test]
fn supports_when_and_goto_transitions() {
    let service = PromptService::default();
    let flow = PromptFlow::new()
        .step(
            PromptStep::new("type", "Pick type", "type")
                .goto("details")
                .context_key("kind"),
        )
        .step(
            PromptStep::new("details", "Project name", "name").when(WhenCondition::Equals {
                key: "kind".into(),
                value: json!("app"),
            }),
        );

    let mut answers = BTreeMap::new();
    answers.insert("type".into(), json!("app"));
    answers.insert("details".into(), json!("demo"));
    let state = service
        .run_scripted(&flow, BTreeMap::new(), answers)
        .expect("submit succeeds");

    assert_eq!(state.answers["name"], json!("demo"));
}

#[test]
fn applies_map_functions_before_validation_and_storage() {
    let service = PromptService::default();
    let flow = PromptFlow::new().step(
        PromptStep::new("name", "Name", "name")
            .map_function(PromptMapFunction::Trim)
            .map_function(PromptMapFunction::Lowercase)
            .validate_rule(ValidationRule::OneOf(vec!["lania".into()])),
    );

    let state = service
        .run_scripted(
            &flow,
            BTreeMap::new(),
            BTreeMap::from([("name".into(), json!("  LANIA  "))]),
        )
        .expect("mapped value should validate");

    assert_eq!(state.answers["name"], json!("lania"));
    assert_eq!(state.context["name"], json!("lania"));
}

#[test]
fn on_answered_can_set_context_and_override_next_step() {
    let service = PromptService::default();
    let flow = PromptFlow::new()
        .step(
            PromptStep::new("mode", "Mode", "mode")
                .on_answered(OnAnsweredAction::SetContextFromAnswer {
                    key: "modeKey".into(),
                    field: None,
                    map_functions: vec![PromptMapFunction::Lowercase],
                })
                .on_answered(OnAnsweredAction::GotoIf {
                    when: WhenCondition::Equals {
                        key: "modeKey".into(),
                        value: json!("advanced"),
                    },
                    target: "advanced-details".into(),
                }),
        )
        .step(PromptStep::new("basic-details", "Basic", "basic"))
        .step(PromptStep::new("advanced-details", "Advanced", "advanced"));

    let state = service
        .run_scripted(
            &flow,
            BTreeMap::new(),
            BTreeMap::from([
                ("mode".into(), json!("ADVANCED")),
                ("advanced-details".into(), json!("enabled")),
            ]),
        )
        .expect("on_answered goto should select advanced branch");

    assert_eq!(state.context["modeKey"], json!("advanced"));
    assert_eq!(state.answers["advanced"], json!("enabled"));
    assert!(!state.answers.contains_key("basic"));
}

#[test]
fn aligns_with_cli_interaction_style_operations() {
    let service = PromptService::default();
    let mut flow = PromptFlow::new().step(
        PromptStep::new("name", "Name", "name")
            .validate_rule(ValidationRule::Required)
            .validate_rule(ValidationRule::MinLength(3)),
    );
    flow.insert_after(
        "name",
        PromptStep::new("package-manager", "Package manager", "packageManager")
            .kind(PromptStepKind::Select)
            .choice("npm", json!("npm"))
            .choice("pnpm", json!("pnpm")),
    )
    .expect("insert after works");

    let mut answers = BTreeMap::new();
    answers.insert("name".into(), json!("lania"));
    answers.insert("package-manager".into(), json!("pnpm"));
    let state = service
        .run_scripted(
            &flow,
            BTreeMap::from([("project".into(), json!("demo"))]),
            answers,
        )
        .expect("flow succeeds");

    assert_eq!(state.answers["name"], json!("lania"));
    assert_eq!(state.answers["packageManager"], json!("pnpm"));
    assert_eq!(state.context["project"], json!("demo"));
}

#[test]
fn validates_and_accumulates_answers() {
    let flow = PromptFlow::new()
        .step(
            PromptStep::new("name", "Name", "name")
                .validate_rule(ValidationRule::Required)
                .validate_rule(ValidationRule::MinLength(3)),
        )
        .step(
            PromptStep::new("feature-a", "Feature", "features")
                .accumulation(AccumulationMode::Append),
        )
        .step(
            PromptStep::new("feature-b", "Feature", "features")
                .accumulation(AccumulationMode::Append),
        );

    let service = PromptService::default();
    let mut answers = BTreeMap::new();
    answers.insert("name".into(), json!("lania"));
    answers.insert("feature-a".into(), json!("eslint"));
    answers.insert("feature-b".into(), json!("vitest"));

    let state = service
        .run_scripted_with_options(
            &flow,
            super::PromptRunOptions {
                answers,
                accumulate: true,
                ..super::PromptRunOptions::default()
            },
        )
        .expect("prompt flow succeeds");

    assert_eq!(state.answers["name"], json!("lania"));
    assert_eq!(state.answers["features"], json!(["eslint", "vitest"]));
}

#[test]
fn supports_resume_and_timeout_fallback() {
    let service = PromptService::default();
    let flow = PromptFlow::new()
        .step(
            PromptStep::new("remote", "Remote", "remote")
                .default_value(json!("origin"))
                .timeout_ms(3000),
        )
        .step(PromptStep::new("branch", "Branch", "branch").returnable());

    let state = service
        .run_scripted_with_options(
            &flow,
            super::PromptRunOptions {
                answers: BTreeMap::from([("branch".into(), json!("__EXIT__"))]),
                fallback: Some(PromptFallbackStrategy::UseDefault),
                ..super::PromptRunOptions::default()
            },
        )
        .expect("initial run succeeds");
    assert!(state.interrupted);
    assert_eq!(state.answers["remote"], json!("origin"));
    assert!(state.timed_out_steps.contains("remote"));

    let resumed = service
        .resume_scripted(
            &flow,
            state,
            BTreeMap::from([("branch".into(), json!("main"))]),
        )
        .expect("resume succeeds");
    assert_eq!(resumed.answers["branch"], json!("main"));
}

#[test]
fn timed_prompt_returns_timeout_when_reader_expires() {
    let step = PromptStep::new("remote", "Remote", "remote")
        .default_value(json!("origin"))
        .timeout_ms(50);
    let mut reader = |_remaining: Duration| Ok(None);

    let outcome = prompt_text_terminal_with_timeout_using(
        &step,
        &super::PromptEngine,
        Duration::from_millis(50),
        &mut reader,
    )
    .expect("timed prompt should not error");

    assert_eq!(outcome, InteractivePromptOutcome::TimedOut);
}

#[test]
fn aligns_with_simple_prompt_interaction_wrapper() {
    let service = PromptService::default();
    let answers = service
        .simple_prompt_scripted(
            [PromptStep::new("project", "Project", "project")],
            BTreeMap::from([("project".into(), json!("demo"))]),
        )
        .expect("simple prompt succeeds");

    assert_eq!(answers["project"], json!("demo"));
}

#[test]
fn tracks_password_fields_for_redaction() {
    let service = PromptService::default();
    let state = service
        .run_scripted(
            &PromptFlow::new().step(
                PromptStep::new("password", "Password", "password").kind(PromptStepKind::Password),
            ),
            BTreeMap::new(),
            BTreeMap::from([("password".into(), json!("super-secret"))]),
        )
        .expect("password prompt should support scripted answers");

    assert_eq!(state.answers["password"], json!("super-secret"));
    assert_eq!(service.secret_fields(), vec!["password".to_string()]);
}

#[test]
fn parses_number_prompt_input() {
    let integer_step = PromptStep::new("count", "Count", "count").kind(PromptStepKind::Number);
    let float_step = PromptStep::new("ratio", "Ratio", "ratio").kind(PromptStepKind::Number);

    assert_eq!(
        parse_prompt_input(&integer_step, "42").expect("integer should parse"),
        json!(42)
    );
    assert_eq!(
        parse_prompt_input(&float_step, "3.14").expect("float should parse"),
        json!(3.14)
    );
    assert!(
        parse_prompt_input(&float_step, "abc").is_err(),
        "invalid numeric input should be rejected"
    );
}

#[test]
fn fuzzy_select_resolves_choices_like_single_select() {
    let step = PromptStep::new("search", "Search", "search")
        .kind(PromptStepKind::FuzzySelect)
        .choice("alpha", json!("alpha"))
        .choice("beta", json!("beta"));

    assert_eq!(
        parse_prompt_input(&step, "beta").expect("choice label should resolve"),
        json!("beta")
    );
    assert_eq!(
        parse_prompt_input(&step, "2").expect("numeric choice index should resolve"),
        json!("beta")
    );
    assert!(
        parse_prompt_input(&step, "gamma").is_err(),
        "unknown fuzzy choice should be rejected"
    );
}

#[test]
fn rawlist_resolves_by_numeric_index_and_label() {
    let step = PromptStep::new("pick", "Pick", "pick")
        .kind(PromptStepKind::RawList)
        .choice("alpha", json!("alpha"))
        .choice("beta", json!("beta"));

    assert_eq!(
        parse_prompt_input(&step, "2").expect("numeric choice index should resolve"),
        json!("beta")
    );
    assert_eq!(
        parse_prompt_input(&step, "alpha").expect("choice label should resolve"),
        json!("alpha")
    );
}

#[test]
fn expand_resolves_by_short_key_and_label() {
    let step = PromptStep::new("pick", "Pick", "pick")
        .kind(PromptStepKind::Expand)
        .choice("alpha", json!("alpha"))
        .choice("beta", json!("beta"));

    assert_eq!(
        parse_prompt_input(&step, "a").expect("expand short key should resolve"),
        json!("alpha")
    );
    assert_eq!(
        resolve_single_choice(&step, "2").expect("index should still resolve"),
        json!("beta")
    );
    assert_eq!(expand_short_keys(2), vec!["a".to_string(), "b".to_string()]);
}
