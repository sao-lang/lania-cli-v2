use serde_json::json;

use super::prompts::prompt_step_from_question;

#[test]
fn maps_extended_template_question_types() {
    let password = prompt_step_from_question(
        &json!({
            "name": "token",
            "type": "password"
        }),
        0,
    )
    .expect("password question");
    assert!(matches!(
        password.kind,
        lania_prompt::PromptStepKind::Password
    ));

    let number = prompt_step_from_question(
        &json!({
            "name": "count",
            "type": "number"
        }),
        1,
    )
    .expect("number question");
    assert!(matches!(number.kind, lania_prompt::PromptStepKind::Number));

    let fuzzy = prompt_step_from_question(
        &json!({
            "name": "template",
            "type": "search"
        }),
        2,
    )
    .expect("search question");
    assert!(matches!(
        fuzzy.kind,
        lania_prompt::PromptStepKind::FuzzySelect
    ));

    let rawlist = prompt_step_from_question(
        &json!({
            "name": "mode",
            "type": "rawlist"
        }),
        3,
    )
    .expect("rawlist question");
    assert!(matches!(
        rawlist.kind,
        lania_prompt::PromptStepKind::RawList
    ));

    let expand = prompt_step_from_question(
        &json!({
            "name": "action",
            "type": "expand"
        }),
        4,
    )
    .expect("expand question");
    assert!(matches!(expand.kind, lania_prompt::PromptStepKind::Expand));
}
