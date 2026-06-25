//! 纯文本终端交互实现（不依赖 dialoguer 的 fallback 路径）。
//!
//! 这个模块主要解决两类场景：
//! - 当前 stdin/stdout 不是完整 TTY，dialoguer 不适用
//! - 需要做“超时等待输入”（通过线程 + channel 组合实现），避免在某些环境里永久阻塞
//!
//! 读法建议：先看 `prompt_text_terminal`，再看带 timeout 的实现。

use std::{
    io::{self, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use serde_json::Value;

use crate::{
    parsing::{
        expand_short_keys, parse_prompt_input, resolve_prompt_value, resolve_single_choice,
        stringify_prompt_value,
    },
    service::InteractivePromptOutcome,
    PromptEngine, PromptStep, PromptStepKind,
};

pub(crate) fn prompt_text_terminal(step: &PromptStep, engine: &PromptEngine) -> Result<Value> {
    let mut stderr = io::stderr();
    loop {
        writeln!(stderr, "{}", step.message)?;
        if let Some(detail) = &step.detail {
            writeln!(stderr, "  {}", detail)?;
        }
        // For text-mode fallback we keep the original numbered list rendering.
        // This path should be rare for Select/MultiSelect now.
        if !step.choices.is_empty() {
            for (index, choice) in step.choices.iter().enumerate() {
                writeln!(stderr, "  {}. {}", index + 1, choice.label)?;
            }
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
                None if matches!(step.kind, PromptStepKind::Confirm) => Value::Bool(false),
                None => {
                    writeln!(stderr, "Input is required.")?;
                    continue;
                }
            }
        } else {
            parse_prompt_input(step, input)?
        };

        match engine.validate(step, &value) {
            Ok(()) => return Ok(value),
            Err(error) => {
                writeln!(stderr, "{error}")?;
            }
        }
    }
}

pub(crate) fn prompt_rawlist_terminal(step: &PromptStep, engine: &PromptEngine) -> Result<Value> {
    let mut stderr = io::stderr();
    loop {
        writeln!(stderr, "{}", step.message)?;
        if let Some(detail) = &step.detail {
            writeln!(stderr, "  {}", detail)?;
        }
        for (index, choice) in step.choices.iter().enumerate() {
            writeln!(stderr, "  {}. {}", index + 1, choice.label)?;
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
            resolve_single_choice(step, input)?
        };

        match engine.validate(step, &value) {
            Ok(()) => return Ok(value),
            Err(error) => {
                writeln!(stderr, "{error}")?;
            }
        }
    }
}

pub(crate) fn prompt_expand_terminal(step: &PromptStep, engine: &PromptEngine) -> Result<Value> {
    let mut stderr = io::stderr();
    let short_keys = expand_short_keys(step.choices.len());

    loop {
        writeln!(stderr, "{}", step.message)?;
        if let Some(detail) = &step.detail {
            writeln!(stderr, "  {}", detail)?;
        }
        let summary = step
            .choices
            .iter()
            .zip(short_keys.iter())
            .map(|(choice, key)| format!("{key}) {}", choice.label))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(stderr, "  {summary}")?;
        writeln!(stderr, "  h) help")?;
        if let Some(default_value) = &step.default_value {
            writeln!(stderr, "Default: {}", stringify_prompt_value(default_value))?;
        }
        write!(stderr, "> ")?;
        stderr.flush()?;

        let mut buffer = String::new();
        io::stdin().read_line(&mut buffer)?;
        let input = buffer.trim().to_ascii_lowercase();

        if input == "h" || input == "help" {
            for (choice, key) in step.choices.iter().zip(short_keys.iter()) {
                writeln!(stderr, "  {key}) {}", choice.label)?;
            }
            continue;
        }

        let value = if input.is_empty() {
            match &step.default_value {
                Some(default_value) => default_value.clone(),
                None => {
                    writeln!(stderr, "Input is required.")?;
                    continue;
                }
            }
        } else if let Some(position) = short_keys.iter().position(|key| *key == input) {
            step.choices
                .get(position)
                .map(|choice| choice.value.clone())
                .ok_or_else(|| anyhow::anyhow!("please choose one of the available options"))?
        } else {
            resolve_single_choice(step, &input)?
        };

        match engine.validate(step, &value) {
            Ok(()) => return Ok(value),
            Err(error) => {
                writeln!(stderr, "{error}")?;
            }
        }
    }
}

pub(crate) fn timeout_supported(step: &PromptStep) -> bool {
    !matches!(step.kind, PromptStepKind::Password | PromptStepKind::Editor)
}

pub(crate) fn prompt_text_terminal_with_timeout(
    step: &PromptStep,
    engine: &PromptEngine,
    timeout: Duration,
) -> Result<InteractivePromptOutcome> {
    prompt_text_terminal_with_timeout_using(step, engine, timeout, &mut read_line_with_timeout)
}

pub(crate) fn prompt_text_terminal_with_timeout_using<F>(
    step: &PromptStep,
    engine: &PromptEngine,
    timeout: Duration,
    read_line: &mut F,
) -> Result<InteractivePromptOutcome>
where
    F: FnMut(Duration) -> io::Result<Option<String>>,
{
    let deadline = Instant::now() + timeout;
    let mut stderr = io::stderr();

    loop {
        write_prompt(stderr.by_ref(), step)?;

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(InteractivePromptOutcome::TimedOut);
        }

        let Some(buffer) = read_line(remaining)? else {
            return Ok(InteractivePromptOutcome::TimedOut);
        };
        let input = buffer.trim();

        if matches!(step.kind, PromptStepKind::Expand) && matches!(input, "h" | "help") {
            write_expand_help(stderr.by_ref(), step)?;
            continue;
        }

        let value = match resolve_prompt_value(step, input) {
            Ok(value) => value,
            Err(error) => {
                writeln!(stderr, "{error}")?;
                continue;
            }
        };

        match engine.validate(step, &value) {
            Ok(()) => return Ok(InteractivePromptOutcome::Value(value)),
            Err(error) => {
                writeln!(stderr, "{error}")?;
            }
        }
    }
}

fn read_line_with_timeout(timeout: Duration) -> io::Result<Option<String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = String::new();
        let result = io::stdin().read_line(&mut buffer).map(|_| buffer);
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(result) => result.map(Some),
        // 返回 `Ok(None)` 而不是 Err，表示“这不是读 stdin 失败，而是超时这个业务结果”。
        // 上层随后会把它翻译成 `InteractivePromptOutcome::TimedOut`。
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "timed prompt input channel disconnected",
        )),
    }
}

fn write_prompt(mut stderr: impl Write, step: &PromptStep) -> io::Result<()> {
    // 统一把 prompt 写到 stderr，而不是 stdout：
    // - stdout 更适合留给机器可读输出（JSON / JSONL）
    // - 即使命令在交互，stdout 仍尽量保持干净，减少和脚本消费方互相污染
    writeln!(stderr, "{}", step.message)?;
    if let Some(detail) = &step.detail {
        writeln!(stderr, "  {}", detail)?;
    }

    match step.kind {
        PromptStepKind::Expand => {
            let short_keys = expand_short_keys(step.choices.len());
            let summary = step
                .choices
                .iter()
                .zip(short_keys.iter())
                .map(|(choice, key)| format!("{key}) {}", choice.label))
                .collect::<Vec<_>>()
                .join(", ");
            if !summary.is_empty() {
                writeln!(stderr, "  {summary}")?;
                writeln!(stderr, "  h) help")?;
            }
        }
        PromptStepKind::MultiSelect => {
            for (index, choice) in step.choices.iter().enumerate() {
                writeln!(stderr, "  {}. {}", index + 1, choice.label)?;
            }
            if !step.choices.is_empty() {
                writeln!(
                    stderr,
                    "  Choose multiple values with comma-separated indexes or labels."
                )?;
            }
        }
        _ if !step.choices.is_empty() => {
            for (index, choice) in step.choices.iter().enumerate() {
                writeln!(stderr, "  {}. {}", index + 1, choice.label)?;
            }
        }
        _ => {}
    }

    if let Some(default_value) = &step.default_value {
        writeln!(stderr, "Default: {}", stringify_prompt_value(default_value))?;
    }
    write!(stderr, "> ")?;
    stderr.flush()
}

fn write_expand_help(mut stderr: impl Write, step: &PromptStep) -> io::Result<()> {
    let short_keys = expand_short_keys(step.choices.len());
    for (choice, key) in step.choices.iter().zip(short_keys.iter()) {
        writeln!(stderr, "  {key}) {}", choice.label)?;
    }
    Ok(())
}
