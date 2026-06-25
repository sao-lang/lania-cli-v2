//! PromptService：统一驱动交互式提问、脚本化回答、恢复执行与敏感字段登记。
//!
//! 这个文件在整个 CLI 里非常重要，因为它把多种输入来源统一到了同一套状态机里：
//! - 真实终端交互
//! - 预先传入的 scripted answers
//! - fallback 默认值
//! - 从上一次中断状态继续 `resume`
//!
//! 新手可以先把 `PromptFlow` 看成“问卷定义”，再把 `PromptService` 看成“问卷执行器”。

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, IsTerminal},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    parsing::{parse_signal, PromptSignal},
    ui_dialoguer::{
        prompt_dialoguer_confirm, prompt_dialoguer_editor, prompt_dialoguer_fuzzy_select,
        prompt_dialoguer_multiselect, prompt_dialoguer_number, prompt_dialoguer_password,
        prompt_dialoguer_select,
    },
    ui_terminal::{
        prompt_expand_terminal, prompt_rawlist_terminal, prompt_text_terminal,
        prompt_text_terminal_with_timeout, timeout_supported,
    },
    PromptEngine, PromptFlow, PromptState, PromptStep, PromptStepKind,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PromptFallbackStrategy {
    UseDefault,
    UseValue(Value),
    Skip,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct PromptRunOptions {
    pub context: BTreeMap<String, Value>,
    pub answers: BTreeMap<String, Value>,
    pub i18n: BTreeMap<String, String>,
    pub accumulate: bool,
    pub reset_accumulated: bool,
    pub fallback: Option<PromptFallbackStrategy>,
    pub resume_from: Option<PromptState>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptService {
    engine: PromptEngine,
    // `accumulated` 用来保存“前一次提问流程得到的答案”，方便后续步骤复用。
    // 之所以放在 `Arc<Mutex<...>>` 里，是因为：
    // - `PromptService` 会被 workflow/context clone 到不同位置；
    // - 这些 clone 需要共享同一份累积答案；
    // - 答案集合又会不断被更新，所以需要互斥保护。
    accumulated: Arc<Mutex<BTreeMap<String, Value>>>,
    // `secret_fields` 单独维护，是为了后续输出/日志时知道哪些字段需要脱敏。
    secret_fields: Arc<Mutex<BTreeSet<String>>>,
}

impl PromptService {
    pub fn reset_secrets(&self) {
        self.secret_fields
            .lock()
            .expect("prompt secret store poisoned")
            .clear();
    }

    pub fn secret_fields(&self) -> Vec<String> {
        self.secret_fields
            .lock()
            .expect("prompt secret store poisoned")
            .iter()
            .cloned()
            .collect()
    }

    fn note_secret_field(&self, field: &str) {
        self.secret_fields
            .lock()
            .expect("prompt secret store poisoned")
            .insert(field.to_string());
    }

    pub fn run_cli_with_options(
        &self,
        flow: &PromptFlow,
        options: PromptRunOptions,
    ) -> Result<PromptState> {
        if options.reset_accumulated {
            self.reset_accumulated();
        }

        let mut state = if let Some(state) = options.resume_from {
            state
        } else {
            self.engine.start(flow, options.context.clone())
        };

        if options.accumulate {
            let accumulated = self
                .accumulated
                .lock()
                .expect("prompt accumulation poisoned")
                .clone();
            // 这里相当于“拿历史答案作为本轮的初始值”。
            // 这样多段 prompt 流程就能共享上下文，例如前面问过的 projectName
            // 后面还可以继续复用，而不用重复问用户。
            state.answers.extend(accumulated);
        }
        state.context.extend(options.context.clone());

        // 只有“当前 stdin 真的是终端”时，才允许进入交互模式。
        // 这能避免在 CI、管道、脚本环境里意外卡死等待输入。
        let interactive = io::stdin().is_terminal()
            && !matches!(options.fallback, Some(PromptFallbackStrategy::Skip));

        while let Some(step) = self.engine.current_step(flow, &state).cloned() {
            if matches!(step.kind, PromptStepKind::Password) {
                self.note_secret_field(&step.field);
            }
            let value = match options.answers.get(&step.id).cloned() {
                Some(value) => value,
                // interactive 分支只在“真 TTY 且没有明确 skip fallback”时走。
                // 一旦环境不适合交互，就强制走 missing-answer/fallback 逻辑，
                // 避免命令在 CI 或管道里卡死。
                None if interactive => {
                    self.resolve_interactive_answer(&step, options.fallback.as_ref(), &mut state)?
                }
                None => {
                    self.resolve_missing_answer(&step, options.fallback.as_ref(), &mut state)?
                }
            };

            match parse_signal(&value) {
                // 这里把“用户输入某个特殊文本”解释成控制信号，而不是普通答案。
                // 这种做法的好处是：脚本模式和交互模式都能复用同一套状态机。
                Some(PromptSignal::Exit) => {
                    state.interrupted = true;
                    break;
                }
                Some(PromptSignal::Back) if step.returnable => {
                    self.engine.go_back(flow, &mut state);
                }
                Some(PromptSignal::Back) => {
                    return Err(anyhow::anyhow!(
                        "step {} does not support back navigation",
                        step.id
                    ));
                }
                None => {
                    self.engine.submit(flow, &mut state, value)?;
                }
            }
        }

        if options.accumulate {
            // accumulate 相当于“把本轮答案写回 PromptService 的共享缓存”。
            *self
                .accumulated
                .lock()
                .expect("prompt accumulation poisoned") = state.answers.clone();
        }

        Ok(state)
    }

    pub fn run_scripted(
        &self,
        flow: &PromptFlow,
        context: BTreeMap<String, Value>,
        answers: BTreeMap<String, Value>,
    ) -> Result<PromptState> {
        self.run_scripted_with_options(
            flow,
            PromptRunOptions {
                context,
                answers,
                ..PromptRunOptions::default()
            },
        )
    }

    pub fn run_scripted_with_options(
        &self,
        flow: &PromptFlow,
        options: PromptRunOptions,
    ) -> Result<PromptState> {
        if options.reset_accumulated {
            self.reset_accumulated();
        }

        let mut state = if let Some(state) = options.resume_from {
            state
        } else {
            self.engine.start(flow, options.context.clone())
        };

        if options.accumulate {
            let accumulated = self
                .accumulated
                .lock()
                .expect("prompt accumulation poisoned")
                .clone();
            state.answers.extend(accumulated);
        }
        state.context.extend(options.context.clone());

        while let Some(step) = self.engine.current_step(flow, &state).cloned() {
            if matches!(step.kind, PromptStepKind::Password) {
                self.note_secret_field(&step.field);
            }
            let translated_message = options
                .i18n
                .get(&step.message)
                .cloned()
                .unwrap_or(step.message.clone());
            let _ = translated_message;
            let value = match options.answers.get(&step.id).cloned() {
                Some(value) => value,
                None => {
                    self.resolve_missing_answer(&step, options.fallback.as_ref(), &mut state)?
                }
            };

            match parse_signal(&value) {
                Some(PromptSignal::Exit) => {
                    state.interrupted = true;
                    break;
                }
                Some(PromptSignal::Back) if step.returnable => {
                    self.engine.go_back(flow, &mut state);
                }
                Some(PromptSignal::Back) => {
                    return Err(anyhow::anyhow!(
                        "step {} does not support back navigation",
                        step.id
                    ));
                }
                None => {
                    self.engine.submit(flow, &mut state, value)?;
                }
            }
        }

        if options.accumulate {
            *self
                .accumulated
                .lock()
                .expect("prompt accumulation poisoned") = state.answers.clone();
        }

        Ok(state)
    }

    pub fn resume_scripted(
        &self,
        flow: &PromptFlow,
        state: PromptState,
        answers: BTreeMap<String, Value>,
    ) -> Result<PromptState> {
        self.run_scripted_with_options(
            flow,
            PromptRunOptions {
                answers,
                resume_from: Some(state),
                ..PromptRunOptions::default()
            },
        )
    }

    pub fn simple_prompt_scripted(
        &self,
        steps: impl IntoIterator<Item = PromptStep>,
        answers: BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>> {
        let flow = steps
            .into_iter()
            .fold(PromptFlow::new(), |flow, step| flow.step(step));
        let state = self.run_scripted(&flow, BTreeMap::new(), answers)?;
        Ok(state.answers)
    }

    pub fn update_context(
        &self,
        state: &mut PromptState,
        context: impl IntoIterator<Item = (String, Value)>,
    ) {
        state.context.extend(context);
    }

    pub fn reset_accumulated(&self) {
        self.accumulated
            .lock()
            .expect("prompt accumulation poisoned")
            .clear();
    }

    pub fn engine(&self) -> &PromptEngine {
        &self.engine
    }

    fn resolve_interactive_answer(
        &self,
        step: &PromptStep,
        fallback: Option<&PromptFallbackStrategy>,
        state: &mut PromptState,
    ) -> Result<Value> {
        match self.prompt_terminal(step)? {
            InteractivePromptOutcome::Value(value) => Ok(value),
            // timeout 不直接报错，而是退回到统一的 missing/fallback 逻辑。
            // 好处是“交互超时”和“脚本里没给值”最终都复用同一套策略，
            // 不会出现两套行为各自分叉、越来越难维护。
            InteractivePromptOutcome::TimedOut => {
                self.resolve_missing_answer(step, fallback, state)
            }
        }
    }

    fn resolve_missing_answer(
        &self,
        step: &PromptStep,
        fallback: Option<&PromptFallbackStrategy>,
        state: &mut PromptState,
    ) -> Result<Value> {
        if let Some(default_value) = step.default_value.clone() {
            if step.timeout_ms.is_some() {
                // 这里单独记录 timed_out_steps，而不是只返回 default，
                // 是为了后续还能知道“这个值是用户真的输入的，还是超时后自动落的默认值”。
                state.timed_out_steps.insert(step.id.clone());
            }
            return Ok(default_value);
        }

        match fallback.cloned().unwrap_or(PromptFallbackStrategy::Error) {
            PromptFallbackStrategy::UseDefault => {
                if step.timeout_ms.is_some() {
                    state.timed_out_steps.insert(step.id.clone());
                    // `UseDefault` 但当前 step 本身没有默认值时，只能退成 `Null`。
                    // 这里的语义更接近“允许跳过”，由后续 step/handler 自己决定如何解释空值。
                    Ok(Value::Null)
                } else {
                    Err(anyhow::anyhow!("missing scripted answer for {}", step.id))
                }
            }
            PromptFallbackStrategy::UseValue(value) => {
                state.timed_out_steps.insert(step.id.clone());
                Ok(value)
            }
            PromptFallbackStrategy::Skip => {
                state.timed_out_steps.insert(step.id.clone());
                Ok(Value::Null)
            }
            PromptFallbackStrategy::Error => {
                Err(anyhow::anyhow!("missing scripted answer for {}", step.id))
            }
        }
    }

    fn prompt_terminal(&self, step: &PromptStep) -> Result<InteractivePromptOutcome> {
        if let Some(timeout_ms) = step.timeout_ms {
            if timeout_supported(step) {
                // timeout 目前只在纯文本终端路径支持：
                // - 因为它是靠“线程读 stdin + channel 等待”实现的
                // - dialoguer/editor/password 这类 richer UI 没法用同样方式稳定中断
                return prompt_text_terminal_with_timeout(
                    step,
                    &self.engine,
                    Duration::from_millis(timeout_ms),
                );
            }
        }

        // Select/MultiSelect are the main "high pitfall" area for terminal interaction.
        // Prefer a mature TUI implementation (dialoguer) and keep the legacy text fallback
        // for edge cases (empty choices, non-tty/scripted handled elsewhere).
        let value = match step.kind {
            PromptStepKind::Select if !step.choices.is_empty() => prompt_dialoguer_select(step),
            PromptStepKind::RawList if !step.choices.is_empty() => {
                prompt_rawlist_terminal(step, &self.engine)
            }
            PromptStepKind::Expand if !step.choices.is_empty() => {
                prompt_expand_terminal(step, &self.engine)
            }
            PromptStepKind::FuzzySelect if !step.choices.is_empty() => {
                prompt_dialoguer_fuzzy_select(step)
            }
            PromptStepKind::MultiSelect if !step.choices.is_empty() => {
                prompt_dialoguer_multiselect(step)
            }
            PromptStepKind::Confirm => prompt_dialoguer_confirm(step),
            PromptStepKind::Password => prompt_dialoguer_password(step),
            PromptStepKind::Editor => prompt_dialoguer_editor(step),
            PromptStepKind::Number => prompt_dialoguer_number(step),
            _ => prompt_text_terminal(step, &self.engine),
        }?;
        Ok(InteractivePromptOutcome::Value(value))
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum InteractivePromptOutcome {
    Value(Value),
    TimedOut,
}
