//! Prompt 子系统的数据模型定义。
//!
//! 这里描述的是“问卷长什么样”，而不是“问卷怎么执行”：
//! - `PromptStepKind`：题型
//! - `PromptChoice`：选项
//! - `PromptStep`：一步完整的问题定义
//! - `ValidationRule` / `WhenCondition` / `OnAnsweredAction`：控制题目行为的规则
//!
//! 新手可以把这部分看成 prompt DSL。

use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::parsing::is_truthy;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptStepKind {
    Input,
    Select,
    Confirm,
    MultiSelect,
    Password,
    Editor,
    Number,
    FuzzySelect,
    RawList,
    Expand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptChoice {
    pub label: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptStep {
    pub id: String,
    pub message: String,
    pub field: String,
    pub kind: PromptStepKind,
    pub choices: Vec<PromptChoice>,
    pub default_value: Option<Value>,
    pub when: Option<WhenCondition>,
    pub goto: Option<String>,
    pub validate: Vec<ValidationRule>,
    pub timeout_ms: Option<u64>,
    pub context_key: Option<String>,
    pub accumulation: AccumulationMode,
    pub returnable: bool,
    pub detail: Option<String>,
    pub map_functions: Vec<PromptMapFunction>,
    pub on_answered: Vec<OnAnsweredAction>,
}

impl PromptStep {
    pub fn new(
        id: impl Into<String>,
        message: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            message: message.into(),
            field: field.into(),
            kind: PromptStepKind::Input,
            choices: Vec::new(),
            default_value: None,
            when: None,
            goto: None,
            validate: Vec::new(),
            timeout_ms: None,
            context_key: None,
            accumulation: AccumulationMode::Replace,
            returnable: false,
            detail: None,
            map_functions: Vec::new(),
            on_answered: Vec::new(),
        }
    }

    pub fn kind(mut self, kind: PromptStepKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn choice(mut self, label: impl Into<String>, value: Value) -> Self {
        self.choices.push(PromptChoice {
            label: label.into(),
            value,
        });
        self
    }

    pub fn default_value(mut self, value: Value) -> Self {
        self.default_value = Some(value);
        self
    }

    pub fn when(mut self, when: WhenCondition) -> Self {
        self.when = Some(when);
        self
    }

    pub fn goto(mut self, target: impl Into<String>) -> Self {
        self.goto = Some(target.into());
        self
    }

    pub fn validate_rule(mut self, rule: ValidationRule) -> Self {
        self.validate.push(rule);
        self
    }

    pub fn timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    pub fn context_key(mut self, key: impl Into<String>) -> Self {
        self.context_key = Some(key.into());
        self
    }

    pub fn accumulation(mut self, accumulation: AccumulationMode) -> Self {
        self.accumulation = accumulation;
        self
    }

    pub fn returnable(mut self) -> Self {
        self.returnable = true;
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn map_function(mut self, map_function: PromptMapFunction) -> Self {
        self.map_functions.push(map_function);
        self
    }

    pub fn on_answered(mut self, action: OnAnsweredAction) -> Self {
        self.on_answered.push(action);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PromptFlow {
    pub steps: Vec<PromptStep>,
}

impl PromptFlow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn step(mut self, step: PromptStep) -> Self {
        self.steps.push(step);
        self
    }

    pub fn insert_after(&mut self, after_step_id: &str, step: PromptStep) -> Result<()> {
        let index = self
            .steps
            .iter()
            .position(|item| item.id == after_step_id)
            .ok_or_else(|| anyhow::anyhow!("prompt step not found: {after_step_id}"))?;
        self.steps.insert(index + 1, step);
        Ok(())
    }

    pub fn insert_before(&mut self, before_step_id: &str, step: PromptStep) -> Result<()> {
        let index = self
            .steps
            .iter()
            .position(|item| item.id == before_step_id)
            .ok_or_else(|| anyhow::anyhow!("prompt step not found: {before_step_id}"))?;
        self.steps.insert(index, step);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WhenCondition {
    Equals { key: String, value: Value },
    NotEquals { key: String, value: Value },
    Exists { key: String },
    Truthy { key: String },
}

impl WhenCondition {
    pub(crate) fn matches(&self, context: &BTreeMap<String, Value>) -> bool {
        match self {
            Self::Equals { key, value } => context.get(key) == Some(value),
            Self::NotEquals { key, value } => context.get(key) != Some(value),
            Self::Exists { key } => context.contains_key(key),
            Self::Truthy { key } => context.get(key).is_some_and(is_truthy),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationRule {
    Required,
    MinLength(usize),
    OneOf(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptMapFunction {
    Trim,
    Lowercase,
    Uppercase,
    ToNumber,
    JsonParse,
    Split { separator: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OnAnsweredAction {
    SetContextValue {
        key: String,
        value: Value,
    },
    SetContextFromAnswer {
        key: String,
        field: Option<String>,
        map_functions: Vec<PromptMapFunction>,
    },
    Goto {
        target: String,
    },
    GotoIf {
        when: WhenCondition,
        target: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccumulationMode {
    Replace,
    Append,
}
