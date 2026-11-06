//! 基于 `dialoguer` 的交互 UI 实现（TTY 场景）。
//!
//! PromptService 在交互模式下会优先走这里：
//! - Select/FuzzySelect/MultiSelect/Confirm/Password/Editor 等题型由 dialoguer 提供更好的 UX
//! - 遇到 Ctrl-C 或用户取消选择时，会统一映射成 EXIT_SIGNAL，交由上层状态机处理
//!
//! 终端不支持或需要超时控制的场景，会退回 `ui_terminal` 的文本实现。

use std::io::{self, Write};

use anyhow::Result;
use dialoguer::{
    theme::ColorfulTheme, Confirm as DialoguerConfirm, Editor, FuzzySelect, MultiSelect, Password,
    Select,
};
use serde_json::Value;

use crate::{
    parsing::{
        default_choice_index, default_multi_choice_flags, parse_number_input,
        stringify_prompt_value,
    },
    PromptStep, ValidationRule, EXIT_SIGNAL,
};

pub(crate) fn prompt_dialoguer_select(step: &PromptStep) -> Result<Value> {
    let theme = ColorfulTheme::default();

    let labels = step
        .choices
        .iter()
        .map(|choice| choice.label.as_str())
        .collect::<Vec<_>>();
    let prompt = if let Some(detail) = &step.detail {
        format!("{}\n  {}", step.message, detail)
    } else {
        step.message.clone()
    };

    let select = Select::with_theme(&theme)
        .items(&labels)
        .with_prompt(prompt);
    let select = if let Some(default_idx) = default_choice_index(step) {
        select.default(default_idx)
    } else {
        select
    };

    let index = match select.interact_opt() {
        Ok(Some(index)) => index,
        Ok(None) => return Ok(Value::String(EXIT_SIGNAL.into())),
        Err(err) => {
            if matches!(&err, dialoguer::Error::IO(io_err) if io_err.kind() == std::io::ErrorKind::Interrupted)
            {
                return Ok(Value::String(EXIT_SIGNAL.into()));
            }
            return Err(err.into());
        }
    };

    Ok(step
        .choices
        .get(index)
        .map(|choice| choice.value.clone())
        .unwrap_or(Value::Null))
}

pub(crate) fn prompt_dialoguer_fuzzy_select(step: &PromptStep) -> Result<Value> {
    let theme = ColorfulTheme::default();

    let labels = step
        .choices
        .iter()
        .map(|choice| choice.label.as_str())
        .collect::<Vec<_>>();
    let prompt = if let Some(detail) = &step.detail {
        format!("{}\n  {}", step.message, detail)
    } else {
        step.message.clone()
    };

    let select = FuzzySelect::with_theme(&theme)
        .items(&labels)
        .with_prompt(prompt);
    let select = if let Some(default_idx) = default_choice_index(step) {
        select.default(default_idx)
    } else {
        select
    };

    let index = match select.interact_opt() {
        Ok(Some(index)) => index,
        Ok(None) => return Ok(Value::String(EXIT_SIGNAL.into())),
        Err(err) => {
            if matches!(&err, dialoguer::Error::IO(io_err) if io_err.kind() == std::io::ErrorKind::Interrupted)
            {
                return Ok(Value::String(EXIT_SIGNAL.into()));
            }
            return Err(err.into());
        }
    };

    Ok(step
        .choices
        .get(index)
        .map(|choice| choice.value.clone())
        .unwrap_or(Value::Null))
}

pub(crate) fn prompt_dialoguer_multiselect(step: &PromptStep) -> Result<Value> {
    let theme = ColorfulTheme::default();

    let labels = step
        .choices
        .iter()
        .map(|choice| choice.label.as_str())
        .collect::<Vec<_>>();
    let prompt = if let Some(detail) = &step.detail {
        format!("{}\n  {}", step.message, detail)
    } else {
        step.message.clone()
    };

    let select = MultiSelect::with_theme(&theme)
        .items(&labels)
        .with_prompt(prompt);
    let select = if let Some(defaults) = default_multi_choice_flags(step) {
        let defaults: Vec<bool> = defaults;
        select.defaults(&defaults)
    } else {
        select
    };

    let indexes = match select.interact_opt() {
        Ok(Some(indexes)) => indexes,
        Ok(None) => return Ok(Value::String(EXIT_SIGNAL.into())),
        Err(err) => {
            if matches!(&err, dialoguer::Error::IO(io_err) if io_err.kind() == std::io::ErrorKind::Interrupted)
            {
                return Ok(Value::String(EXIT_SIGNAL.into()));
            }
            return Err(err.into());
        }
    };

    Ok(Value::Array(
        indexes
            .into_iter()
            .filter_map(|idx| step.choices.get(idx).map(|choice| choice.value.clone()))
            .collect(),
    ))
}

pub(crate) fn prompt_dialoguer_confirm(step: &PromptStep) -> Result<Value> {
    let theme = ColorfulTheme::default();
    let prompt = if let Some(detail) = &step.detail {
        format!("{}\n  {}", step.message, detail)
    } else {
        step.message.clone()
    };
    let default = step
        .default_value
        .as_ref()
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match DialoguerConfirm::with_theme(&theme)
        .with_prompt(prompt)
        .default(default)
        .interact_opt()
    {
        Ok(Some(value)) => Ok(Value::Bool(value)),
        Ok(None) => Ok(Value::String(EXIT_SIGNAL.into())),
        Err(err) => {
            if matches!(&err, dialoguer::Error::IO(io) if io.kind() == io::ErrorKind::Interrupted) {
                return Ok(Value::String(EXIT_SIGNAL.into()));
            }
            Err(err.into())
        }
    }
}

pub(crate) fn prompt_dialoguer_password(step: &PromptStep) -> Result<Value> {
    let theme = ColorfulTheme::default();
    let prompt = if let Some(detail) = &step.detail {
        format!("{}\n  {}", step.message, detail)
    } else {
        step.message.clone()
    };

    // Note: Password does not support defaults and should not echo user input.
    // If the user cancels (Ctrl-C), return EXIT signal to stop the flow.
    let allow_empty = !step
        .validate
        .iter()
        .any(|rule| matches!(rule, ValidationRule::Required));
    let value = match Password::with_theme(&theme)
        .with_prompt(prompt)
        .allow_empty_password(allow_empty)
        .interact()
    {
        Ok(value) => value,
        Err(err) => {
            if matches!(&err, dialoguer::Error::IO(io) if io.kind() == io::ErrorKind::Interrupted) {
                return Ok(Value::String(EXIT_SIGNAL.into()));
            }
            return Err(err.into());
        }
    };

    Ok(Value::String(value))
}

pub(crate) fn prompt_dialoguer_editor(step: &PromptStep) -> Result<Value> {
    // dialoguer::Editor launches an external editor and returns the edited content.
    // It does not render a "prompt" in the terminal itself, so we print a message first.
    let mut stderr = io::stderr();
    writeln!(stderr, "{}", step.message)?;
    if let Some(detail) = &step.detail {
        writeln!(stderr, "  {}", detail)?;
    }
    stderr.flush()?;

    let initial = step
        .default_value
        .as_ref()
        .and_then(Value::as_str)
        .unwrap_or("");

    match Editor::new().edit(initial) {
        Ok(Some(text)) => Ok(Value::String(text)),
        Ok(None) => Ok(Value::String(EXIT_SIGNAL.into())),
        Err(err) => Err(err.into()),
    }
}

pub(crate) fn prompt_dialoguer_number(step: &PromptStep) -> Result<Value> {
    // Keep number interaction simple: read text and parse into a JSON number.
    // We still run validation afterwards via PromptEngine::validate().
    let mut stderr = io::stderr();
    loop {
        writeln!(stderr, "{}", step.message)?;
        if let Some(detail) = &step.detail {
            writeln!(stderr, "  {}", detail)?;
        }
        if let Some(default_value) = &step.default_value {
            writeln!(stderr, "Default: {}", stringify_prompt_value(default_value))?;
        }
        write!(stderr, "> ")?;
        stderr.flush()?;

        let mut buffer = String::new();
        io::stdin().read_line(&mut buffer)?;
        let input = buffer.trim();

        let value = if input.is_empty() {
            match &step.default_value {
                Some(default_value) => default_value.clone(),
                None => {
                    writeln!(stderr, "Input is required.")?;
                    continue;
                }
            }
        } else {
            parse_number_input(input)?
        };

        // Let the engine validate any additional rules (e.g. OneOf if configured).
        // Note: Most numeric constraints should be implemented as dedicated rules later.
        return Ok(value);
    }
}
