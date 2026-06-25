//! `create` 工作流的交互式输入采集（prompt）。
//!
//! 这里做的是“把 create 所需的信息问出来”，而不是生成文件：
//! - `run_create_prompt()`：先问 project name / template 等基础信息
//! - `run_template_prompt()`：模板自定义问题（通常来自 Node bridge 的模板系统）
//!
//! PromptFlow/PromptStep 只描述“问卷长什么样”，真正的交互执行由 `PromptService` 统一负责。

use std::{collections::BTreeMap, io::IsTerminal};

use anyhow::{anyhow, Result};
use lania_prompt::{PromptFlow, PromptService, PromptStep};
use serde_json::{json, Value};

use super::templates::ADD_TEMPLATE_SPECS;
use crate::models::{AddWorkflowInput, CreateWorkflowInput};

pub(super) fn run_create_prompt(
    prompt: &PromptService,
    locale: &str,
    input: &CreateWorkflowInput,
    templates: &[String],
) -> Result<BTreeMap<String, Value>> {
    let zh = locale == "zh";
    let inferred_project_name = if input.path.as_deref() == Some(".") {
        Some(
            input
                .cwd
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("lania-app")
                .to_string(),
        )
    } else {
        None
    };
    let mut template_step =
        PromptStep::new("template", if zh { "模板" } else { "Template" }, "template")
            .kind(lania_prompt::PromptStepKind::Select);
    if std::io::stdin().is_terminal() {
        template_step = template_step.default_value(json!("spa-react"));
    }
    for template in templates {
        template_step = template_step.choice(template, json!(template));
    }
    // PromptFlow 可以理解成“问卷状态机定义”：
    // 这里只负责把 step 顺序描述出来，真正怎么跑由 PromptService 统一处理。
    let mut flow = PromptFlow::new();
    if input.path.as_deref() != Some(".") {
        flow = flow.step(
            PromptStep::new(
                "project-name",
                if zh { "项目名称" } else { "Project name" },
                "projectName",
            )
            .default_value(json!("lania-app")),
        );
    }
    flow = flow.step(template_step);
    let mut answers = BTreeMap::new();
    if let Some(project_name) = input
        .project_name
        .as_ref()
        .or(inferred_project_name.as_ref())
    {
        answers.insert("project-name".into(), json!(project_name));
    }
    if let Some(template) = &input.template {
        answers.insert("template".into(), json!(template));
    }
    let state = prompt.run_cli_with_options(
        &flow,
        lania_prompt::PromptRunOptions {
            answers,
            fallback: Some(lania_prompt::PromptFallbackStrategy::Error),
            ..lania_prompt::PromptRunOptions::default()
        },
    )?;
    Ok(state.answers)
}

pub(super) fn run_template_prompt(
    prompt: &PromptService,
    _template: &str,
    questions: &[Value],
    existing: &BTreeMap<String, Value>,
    input: &CreateWorkflowInput,
) -> Result<BTreeMap<String, Value>> {
    // 模板问题来自 Node/模板定义，Rust 这里做的是“问题格式适配层”：
    // 把外部 question JSON 转成本项目统一的 PromptStep。
    let flow =
        questions
            .iter()
            .enumerate()
            .try_fold(PromptFlow::new(), |flow, (index, question)| {
                Ok::<_, anyhow::Error>(flow.step(prompt_step_from_question(question, index)?))
            })?;
    if flow.steps.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut answers = BTreeMap::new();
    for step in &flow.steps {
        if let Some(value) = existing.get(&step.field) {
            answers.insert(step.id.clone(), value.clone());
        } else if step.field == "packageManager" {
            let Some(package_manager) = input.package_manager.as_deref() else {
                continue;
            };
            answers.insert(step.id.clone(), json!(package_manager));
        }
    }
    let state = prompt.run_cli_with_options(
        &flow,
        lania_prompt::PromptRunOptions {
            answers,
            fallback: Some(lania_prompt::PromptFallbackStrategy::UseDefault),
            ..lania_prompt::PromptRunOptions::default()
        },
    )?;
    Ok(state.answers)
}

pub(super) fn build_template_question_options(
    input: &CreateWorkflowInput,
    prompt_state: &BTreeMap<String, Value>,
    locale: &str,
) -> serde_json::Value {
    // 这里把零散信息整理成一个 options 对象发给模板运行时：
    // 这样模板侧只需要读一份上下文，而不用分别关心 CLI 参数、交互答案和 locale 来源。
    let mut options = prompt_state
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<serde_json::Map<String, Value>>();
    options.insert("locale".into(), json!(locale));
    if let Some(language) = &input.language {
        options.entry("language").or_insert_with(|| json!(language));
    }
    if let Some(package_manager) = &input.package_manager {
        options
            .entry("packageManager")
            .or_insert_with(|| json!(package_manager));
    }
    if input.skip_install_specified {
        options.insert("skipInstall".into(), json!(input.skip_install));
        options.insert("skipInstallSpecified".into(), json!(true));
    }
    options.insert("skipGit".into(), json!(!input.init_git));
    if let Some(project_name) = prompt_state.get("projectName").cloned() {
        options.insert("projectName".into(), project_name.clone());
        options.insert("name".into(), project_name);
    }
    serde_json::Value::Object(options)
}

pub(super) fn run_add_prompt(
    prompt: &PromptService,
    locale: &str,
    input: &AddWorkflowInput,
) -> Result<BTreeMap<String, Value>> {
    let zh = locale == "zh";
    let mut template_step = PromptStep::new(
        "template",
        if zh { "添加模板" } else { "Add template" },
        "template",
    )
    .kind(lania_prompt::PromptStepKind::Select);
    if std::io::stdin().is_terminal() {
        template_step = template_step.default_value(json!("rfc"));
    }
    for template in ADD_TEMPLATE_SPECS {
        template_step = template_step.choice(template.label, json!(template.name));
    }
    let flow = PromptFlow {
        steps: vec![
            template_step,
            PromptStep::new(
                "target",
                if zh { "目标路径" } else { "Target path" },
                "target",
            )
            .detail(if zh {
                "示例：src/components 或 src/components/Button.tsx"
            } else {
                "Examples: src/components or src/components/Button.tsx"
            })
            .default_value(json!("src")),
            PromptStep::new("name", if zh { "文件名" } else { "File name" }, "name")
                .detail(if zh {
                    "当目标路径指向目录时使用"
                } else {
                    "Used when the target path points to a directory"
                })
                .default_value(json!("index")),
        ],
    };
    let mut answers = BTreeMap::new();
    if let Some(template) = &input.template {
        answers.insert("template".into(), json!(template));
    }
    if let Some(target) = &input.target {
        answers.insert("target".into(), json!(target));
    }
    if let Some(name) = &input.name {
        answers.insert("name".into(), json!(name));
    }
    let state = prompt.run_cli_with_options(
        &flow,
        lania_prompt::PromptRunOptions {
            answers,
            fallback: Some(lania_prompt::PromptFallbackStrategy::Error),
            ..lania_prompt::PromptRunOptions::default()
        },
    )?;
    Ok(state.answers)
}

pub(super) fn prompt_step_from_question(question: &Value, index: usize) -> Result<PromptStep> {
    let field = question["name"]
        .as_str()
        .ok_or_else(|| anyhow!("template question missing name"))?;
    let message = question["message"].as_str().unwrap_or(field);
    let prompt_type = question["type"].as_str().unwrap_or("input");
    // `question-{index}-{field}` 用作稳定 id，避免不同题目 field 相同时互相覆盖。
    let mut step = PromptStep::new(format!("question-{index}-{field}"), message, field);
    step = match prompt_type {
        "checkbox" => step.kind(lania_prompt::PromptStepKind::MultiSelect),
        "list" | "select" => step.kind(lania_prompt::PromptStepKind::Select),
        "rawlist" => step.kind(lania_prompt::PromptStepKind::RawList),
        "expand" => step.kind(lania_prompt::PromptStepKind::Expand),
        "autocomplete" | "search" | "autocomplete_search" | "fuzzy_select" => {
            step.kind(lania_prompt::PromptStepKind::FuzzySelect)
        }
        "confirm" => step.kind(lania_prompt::PromptStepKind::Confirm),
        "password" => step.kind(lania_prompt::PromptStepKind::Password),
        "editor" => step.kind(lania_prompt::PromptStepKind::Editor),
        "number" => step.kind(lania_prompt::PromptStepKind::Number),
        _ => step.kind(lania_prompt::PromptStepKind::Input),
    };
    if let Some(choices) = question["choices"].as_array() {
        for choice in choices {
            if let Some(label) = choice.as_str() {
                step = step.choice(label, json!(label));
            } else if let Some(record) = choice.as_object() {
                let label = record
                    .get("name")
                    .or_else(|| record.get("label"))
                    .and_then(Value::as_str)
                    .or_else(|| record.get("value").and_then(Value::as_str))
                    .ok_or_else(|| anyhow!("template question choice missing label"))?;
                let value = record.get("value").cloned().unwrap_or_else(|| json!(label));
                step = step.choice(label, value);
            }
        }
    }
    if let Some(default_value) = question.get("default") {
        step = step.default_value(default_value.clone());
    } else {
        // 没有显式 default 时，根据题目类型给一个“最不容易卡死”的默认值：
        // - select 默认第一项
        // - confirm 默认 false
        // - input/editor 默认空字符串
        step = match step.kind {
            lania_prompt::PromptStepKind::Select
            | lania_prompt::PromptStepKind::FuzzySelect
            | lania_prompt::PromptStepKind::RawList
            | lania_prompt::PromptStepKind::Expand => {
                let default_choice = step.choices.first().map(|choice| choice.value.clone());
                if let Some(default_choice) = default_choice {
                    step.default_value(default_choice)
                } else {
                    step
                }
            }
            lania_prompt::PromptStepKind::MultiSelect => {
                let defaults = step
                    .choices
                    .iter()
                    .map(|choice| choice.value.clone())
                    .collect::<Vec<_>>();
                if defaults.is_empty() {
                    step
                } else {
                    step.default_value(json!(defaults))
                }
            }
            lania_prompt::PromptStepKind::Confirm => step.default_value(json!(false)),
            lania_prompt::PromptStepKind::Number => step.default_value(json!(0)),
            lania_prompt::PromptStepKind::Password => step,
            lania_prompt::PromptStepKind::Editor => step.default_value(json!("")),
            lania_prompt::PromptStepKind::Input => step.default_value(json!("")),
        };
    }
    Ok(step)
}
