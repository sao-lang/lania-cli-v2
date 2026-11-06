//! PromptFlow 的纯状态机引擎。
//!
//! 和 `PromptService` 的区别是：
//! - `PromptService` 负责“如何与用户交互”
//! - `PromptEngine` 只负责“当前应该问哪一步、提交答案后下一步去哪”
//!
//! 因此这个文件基本不碰终端 IO，它更像一个“可测试的问卷状态机核心”。

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::parsing::parse_number_input;
use crate::{
    AccumulationMode, OnAnsweredAction, PromptFlow, PromptMapFunction, PromptStep, PromptStepKind,
    ValidationRule,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptState {
    pub current_step_id: Option<String>,
    pub answers: BTreeMap<String, Value>,
    pub context: BTreeMap<String, Value>,
    pub completed_steps: BTreeSet<String>,
    pub timed_out_steps: BTreeSet<String>,
    pub interrupted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PromptEngine;

impl PromptEngine {
    pub fn start(&self, flow: &PromptFlow, context: BTreeMap<String, Value>) -> PromptState {
        PromptState {
            current_step_id: self.next_step_id(flow, None, &context, &BTreeSet::new()),
            answers: BTreeMap::new(),
            context,
            completed_steps: BTreeSet::new(),
            timed_out_steps: BTreeSet::new(),
            interrupted: false,
        }
    }

    pub fn current_step<'a>(
        &self,
        flow: &'a PromptFlow,
        state: &PromptState,
    ) -> Option<&'a PromptStep> {
        let current_id = state.current_step_id.as_ref()?;
        flow.steps.iter().find(|step| &step.id == current_id)
    }

    pub fn submit(
        &self,
        flow: &PromptFlow,
        state: &mut PromptState,
        value: Value,
    ) -> Result<Option<PromptStep>> {
        let step = self
            .current_step(flow, state)
            .ok_or_else(|| anyhow::anyhow!("prompt flow already completed"))?
            .clone();

        let value = apply_map_functions(value, &step.map_functions)?;
        self.validate(&step, &value)?;
        // answers 和 context 会同时更新，但语义不同：
        // - answers 更像“最终答卷”
        // - context 更像“后续条件判断/跳转可读取的运行时变量”
        accumulate_answer(
            &mut state.answers,
            &step.field,
            value.clone(),
            step.accumulation,
        );

        let context_key = step
            .context_key
            .as_deref()
            .unwrap_or(&step.field)
            .to_string();
        accumulate_answer(
            &mut state.context,
            &context_key,
            value.clone(),
            step.accumulation,
        );

        let mut next_target = step.goto.clone();
        for action in &step.on_answered {
            apply_on_answered_action(action, state, &value, &mut next_target)?;
        }

        state.completed_steps.insert(step.id.clone());
        state.current_step_id = if let Some(next_id) = next_target.as_ref() {
            self.jump_to(flow, next_id, &state.context, &state.completed_steps)
        } else {
            self.next_step_id(flow, Some(&step.id), &state.context, &state.completed_steps)
        };

        Ok(self.current_step(flow, state).cloned())
    }

    pub fn go_back(&self, flow: &PromptFlow, state: &mut PromptState) -> Option<PromptStep> {
        let current_id = state.current_step_id.clone()?;
        let current_index = flow.steps.iter().position(|step| step.id == current_id)?;
        let previous = flow.steps[..current_index]
            .iter()
            .rev()
            .find(|step| state.completed_steps.contains(&step.id))?;
        // go_back 只回退“流程位置”，不自动删除旧答案。
        // 这样 UI 可以先把旧值重新展示出来，再由用户决定是否覆盖。
        state.completed_steps.remove(&previous.id);
        state.current_step_id = Some(previous.id.clone());
        Some(previous.clone())
    }

    pub(crate) fn validate(&self, step: &PromptStep, value: &Value) -> Result<()> {
        for rule in &step.validate {
            match rule {
                ValidationRule::Required => {
                    if value.is_null() || value.as_str().is_some_and(|text| text.trim().is_empty())
                    {
                        return Err(anyhow::anyhow!("{} is required", step.field));
                    }
                }
                ValidationRule::MinLength(min) => {
                    let len = value.as_str().map(str::len).unwrap_or_default();
                    if len < *min {
                        return Err(anyhow::anyhow!(
                            "{} must be at least {} chars",
                            step.field,
                            min
                        ));
                    }
                }
                ValidationRule::OneOf(choices) => {
                    let current = value
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("{} must be a string", step.field))?;
                    if !choices.iter().any(|choice| choice == current) {
                        return Err(anyhow::anyhow!(
                            "{} must be one of {:?}",
                            step.field,
                            choices
                        ));
                    }
                }
            }
        }

        if !step.choices.is_empty() {
            let valid = match step.kind {
                PromptStepKind::MultiSelect => value
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .all(|item| step.choices.iter().any(|choice| choice.value == *item))
                    })
                    .unwrap_or(false),
                _ => step.choices.iter().any(|choice| choice.value == *value),
            };
            if !valid {
                return Err(anyhow::anyhow!(
                    "{} must match one of the configured choices",
                    step.field
                ));
            }
        }

        Ok(())
    }

    fn next_step_id(
        &self,
        flow: &PromptFlow,
        after_id: Option<&str>,
        context: &BTreeMap<String, Value>,
        visited: &BTreeSet<String>,
    ) -> Option<String> {
        let start = after_id
            .and_then(|id| flow.steps.iter().position(|step| step.id == id))
            .map(|idx| idx + 1)
            .unwrap_or(0);

        flow.steps
            .iter()
            .skip(start)
            .find(|step| should_present(step, context, visited))
            .map(|step| step.id.clone())
    }

    fn jump_to(
        &self,
        flow: &PromptFlow,
        target: &str,
        context: &BTreeMap<String, Value>,
        visited: &BTreeSet<String>,
    ) -> Option<String> {
        flow.steps
            .iter()
            .find(|step| step.id == target && should_present(step, context, visited))
            .map(|step| step.id.clone())
    }
}

fn should_present(
    step: &PromptStep,
    context: &BTreeMap<String, Value>,
    visited: &BTreeSet<String>,
) -> bool {
    if visited.contains(&step.id) {
        // 默认避免重复呈现同一步，除非流程通过 go_back 显式把它从 visited 中移除。
        return false;
    }

    step.when
        .as_ref()
        .map(|condition| condition.matches(context))
        .unwrap_or(true)
}

fn accumulate_answer(
    store: &mut BTreeMap<String, Value>,
    key: &str,
    value: Value,
    accumulation: AccumulationMode,
) {
    match accumulation {
        AccumulationMode::Replace => {
            store.insert(key.to_string(), value);
        }
        AccumulationMode::Append => {
            // Append 允许同一个字段多次收集值，例如多轮补充输入或重复问题。
            let entry = store
                .entry(key.to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            match entry {
                Value::Array(items) => items.push(value),
                existing => {
                    let previous = existing.take();
                    *existing = Value::Array(vec![previous, value]);
                }
            }
        }
    }
}

fn apply_map_functions(mut value: Value, map_functions: &[PromptMapFunction]) -> Result<Value> {
    for map_function in map_functions {
        value = apply_map_function(value, map_function)?;
    }
    Ok(value)
}

fn apply_map_function(value: Value, map_function: &PromptMapFunction) -> Result<Value> {
    match map_function {
        PromptMapFunction::Trim => Ok(Value::String(
            value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("trim map function requires string input"))?
                .trim()
                .to_string(),
        )),
        PromptMapFunction::Lowercase => Ok(Value::String(
            value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("lowercase map function requires string input"))?
                .to_ascii_lowercase(),
        )),
        PromptMapFunction::Uppercase => Ok(Value::String(
            value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("uppercase map function requires string input"))?
                .to_ascii_uppercase(),
        )),
        PromptMapFunction::ToNumber => parse_number_input(
            value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("to_number map function requires string input"))?,
        ),
        PromptMapFunction::JsonParse => {
            Ok(serde_json::from_str(value.as_str().ok_or_else(|| {
                anyhow::anyhow!("json_parse map function requires string input")
            })?)?)
        }
        PromptMapFunction::Split { separator } => Ok(Value::Array(
            value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("split map function requires string input"))?
                .split(separator)
                .map(str::trim)
                .filter(|segment| !segment.is_empty())
                .map(|segment| Value::String(segment.to_string()))
                .collect(),
        )),
    }
}

fn apply_on_answered_action(
    action: &OnAnsweredAction,
    state: &mut PromptState,
    current_value: &Value,
    next_target: &mut Option<String>,
) -> Result<()> {
    match action {
        OnAnsweredAction::SetContextValue { key, value } => {
            // 直接写死一个 context 值，常用于“选了某个选项后顺手种下后续条件变量”。
            state.context.insert(key.clone(), value.clone());
        }
        OnAnsweredAction::SetContextFromAnswer {
            key,
            field,
            map_functions,
        } => {
            // 允许 action 显式指定“从哪个字段抄答案”，
            // 没指定时才退回当前题目的 current_value。
            let source = field
                .as_ref()
                .and_then(|field| state.answers.get(field))
                .cloned()
                .unwrap_or_else(|| current_value.clone());
            let mapped = apply_map_functions(source, map_functions)?;
            state.context.insert(key.clone(), mapped);
        }
        OnAnsweredAction::Goto { target } => {
            // 显式跳转会覆盖 step 自带的默认 goto/顺序推进。
            *next_target = Some(target.clone());
        }
        OnAnsweredAction::GotoIf { when, target } => {
            if when.matches(&state.context) {
                // 条件跳转读取的是“已经更新后的 context”，
                // 因此同一轮 submit 里前面的 SetContext* 动作会影响这里的判断。
                *next_target = Some(target.clone());
            }
        }
    }
    Ok(())
}
