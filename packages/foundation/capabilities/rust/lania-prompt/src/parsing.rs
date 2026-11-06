//! Prompt 输入解析工具。
//!
//! 这个文件负责把“用户输入的原始文本”转换成 `serde_json::Value`：
//! - 处理默认值
//! - 处理 yes/no、数字、多选、快捷键
//! - 处理特殊控制信号（例如 back / exit）
//!
//! 它相当于 prompt 子系统里的“词法/语义解析层”。

use anyhow::Result;
use serde_json::Value;

use crate::{PromptStep, PromptStepKind};

pub(crate) fn resolve_prompt_value(step: &PromptStep, input: &str) -> Result<Value> {
    if input.is_empty() {
        return match &step.default_value {
            Some(default_value) => Ok(default_value.clone()),
            None if matches!(step.kind, PromptStepKind::Confirm) => Ok(Value::Bool(false)),
            None => Err(anyhow::anyhow!("Input is required.")),
        };
    }
    parse_prompt_input(step, input)
}

pub(crate) fn expand_short_keys(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| {
            let key = b'a'.saturating_add(index as u8) as char;
            key.to_string()
        })
        .collect()
}

pub(crate) fn default_choice_index(step: &PromptStep) -> Option<usize> {
    let default = step.default_value.as_ref()?;
    step.choices.iter().position(|choice| {
        choice.value == *default || Value::String(choice.label.clone()) == *default
    })
}

pub(crate) fn default_multi_choice_flags(step: &PromptStep) -> Option<Vec<bool>> {
    let default = step.default_value.as_ref()?;
    let defaults = default.as_array()?;
    Some(
        step.choices
            .iter()
            .map(|choice| defaults.contains(&choice.value))
            .collect(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptSignal {
    Back,
    Exit,
}

pub(crate) fn parse_signal(value: &Value) -> Option<PromptSignal> {
    match value.as_str() {
        Some(crate::BACK_SIGNAL) => Some(PromptSignal::Back),
        Some(crate::EXIT_SIGNAL) => Some(PromptSignal::Exit),
        _ => None,
    }
}

pub(crate) fn parse_prompt_input(step: &PromptStep, input: &str) -> Result<Value> {
    match step.kind {
        PromptStepKind::Input => Ok(Value::String(input.to_string())),
        PromptStepKind::Password | PromptStepKind::Editor => Ok(Value::String(input.to_string())),
        PromptStepKind::Number => parse_number_input(input),
        PromptStepKind::Confirm => match input.to_ascii_lowercase().as_str() {
            "y" | "yes" | "true" | "1" => Ok(Value::Bool(true)),
            "n" | "no" | "false" | "0" => Ok(Value::Bool(false)),
            _ => Err(anyhow::anyhow!("please answer yes or no")),
        },
        PromptStepKind::Select | PromptStepKind::RawList | PromptStepKind::FuzzySelect => {
            resolve_single_choice(step, input)
        }
        PromptStepKind::Expand => resolve_expand_choice(step, input),
        PromptStepKind::MultiSelect => resolve_multiple_choices(step, input),
    }
}

pub(crate) fn parse_number_input(input: &str) -> Result<Value> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Value::Null);
    }
    if let Ok(value) = input.parse::<i64>() {
        return Ok(Value::Number(value.into()));
    }
    if let Ok(value) = input.parse::<u64>() {
        return Ok(Value::Number(value.into()));
    }
    if let Ok(value) = input.parse::<f64>() {
        let number =
            serde_json::Number::from_f64(value).ok_or_else(|| anyhow::anyhow!("invalid number"))?;
        return Ok(Value::Number(number));
    }
    Err(anyhow::anyhow!("please enter a valid number"))
}

pub(crate) fn resolve_expand_choice(step: &PromptStep, input: &str) -> Result<Value> {
    let input = input.trim().to_ascii_lowercase();
    if input.is_empty() {
        return Err(anyhow::anyhow!(
            "please choose one of the available options"
        ));
    }
    // Inquirer-style expand: accept single-letter keys (a, b, c...) in addition to labels/indices.
    if input.len() == 1 {
        let keys = expand_short_keys(step.choices.len());
        if let Some(index) = keys.iter().position(|key| *key == input) {
            return step
                .choices
                .get(index)
                .map(|choice| choice.value.clone())
                .ok_or_else(|| anyhow::anyhow!("please choose one of the available options"));
        }
    }
    resolve_single_choice(step, &input)
}

pub(crate) fn resolve_single_choice(step: &PromptStep, input: &str) -> Result<Value> {
    if let Ok(index) = input.parse::<usize>() {
        if let Some(choice) = step.choices.get(index.saturating_sub(1)) {
            return Ok(choice.value.clone());
        }
    }

    step.choices
        .iter()
        .find(|choice| choice.label == input || stringify_prompt_value(&choice.value) == input)
        .map(|choice| choice.value.clone())
        .ok_or_else(|| anyhow::anyhow!("please choose one of the available options"))
}

pub(crate) fn resolve_multiple_choices(step: &PromptStep, input: &str) -> Result<Value> {
    let mut values = Vec::new();
    for segment in input
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
    {
        values.push(resolve_single_choice(step, segment)?);
    }
    Ok(Value::Array(values))
}

pub(crate) fn stringify_prompt_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(items) => items
            .iter()
            .map(stringify_prompt_value)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

pub(crate) fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(boolean) => *boolean,
        Value::Number(number) => number.as_i64().is_some_and(|item| item != 0),
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::Null => false,
    }
}
