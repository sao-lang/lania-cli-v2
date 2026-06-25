//! 动态命令与动态 hook 的运行时接入层。
//!
//! 这个入口文件只保留模块组织与对外重导出：
//! - `types` 定义动态命令/动态 hook 需要的结构体
//! - `command` 负责动态命令执行与 prompt 注入
//! - `prompt` 负责把 wire 格式 prompt 解析成宿主 prompt flow
//! - `hooks` 负责动态 hook invoker 与注册逻辑

mod command;
mod hooks;
mod prompt;
mod types;

pub(super) use self::hooks::register_dynamic_target_hooks;
pub(super) use self::prompt::prompt_step_from_wire;
#[cfg(test)]
pub(super) use self::prompt::{prompt_context_from_argv, redact_secret_answer_map};
pub(super) use self::types::{
    BridgeCommandHandler, BridgeHookInvoker, InlineHookInvoker, ResolvedDynamicCommands,
};

#[cfg(test)]
mod tests {
    use super::{prompt_context_from_argv, prompt_step_from_wire};
    use lania_command::ParsedArgv;
    use lania_prompt::{
        AccumulationMode, OnAnsweredAction, PromptMapFunction, ValidationRule, WhenCondition,
    };
    use serde_json::json;

    #[test]
    fn prompt_step_from_wire_supports_advanced_flow_fields() {
        let argv = ParsedArgv {
            args: Default::default(),
            options: [("project".to_string(), json!("demo"))]
                .into_iter()
                .collect(),
        };
        let item = json!({
            "id": "mode",
            "field": "mode",
            "message": { "zh": "模式？", "en": "Mode?" },
            "kind": "select",
            "choices": [
                { "label": "simple", "value": "simple" },
                { "label": "advanced", "value": "advanced" }
            ],
            "when": { "type": "truthy", "key": "project" },
            "goto": "region",
            "validate": [
                "required",
                { "type": "min_length", "min": 3 },
                { "type": "one_of", "values": ["simple", "advanced"] }
            ],
            "timeoutMs": 3000,
            "contextKey": "deployMode",
            "accumulation": "append",
            "returnable": true,
            "mapFunctions": ["trim", { "type": "lowercase" }],
            "onAnswered": [
                { "type": "set_context_value", "key": "advanced", "value": true },
                {
                    "type": "goto_if",
                    "when": { "type": "truthy", "key": "advanced" },
                    "target": "region"
                }
            ]
        });

        let step = prompt_step_from_wire(&item, &argv, "zh").expect("step should parse");

        assert_eq!(step.id, "mode");
        assert_eq!(step.message, "模式？");
        assert_eq!(step.goto.as_deref(), Some("region"));
        assert_eq!(
            step.when,
            Some(WhenCondition::Truthy {
                key: "project".into()
            })
        );
        assert_eq!(step.validate[0], ValidationRule::Required);
        assert_eq!(step.validate[1], ValidationRule::MinLength(3));
        assert_eq!(
            step.validate[2],
            ValidationRule::OneOf(vec!["simple".into(), "advanced".into()])
        );
        assert_eq!(step.timeout_ms, Some(3000));
        assert_eq!(step.context_key.as_deref(), Some("deployMode"));
        assert_eq!(step.accumulation, AccumulationMode::Append);
        assert!(step.returnable);
        assert_eq!(
            step.map_functions,
            vec![PromptMapFunction::Trim, PromptMapFunction::Lowercase]
        );
        assert_eq!(
            step.on_answered,
            vec![
                OnAnsweredAction::SetContextValue {
                    key: "advanced".into(),
                    value: json!(true)
                },
                OnAnsweredAction::GotoIf {
                    when: WhenCondition::Truthy {
                        key: "advanced".into()
                    },
                    target: "region".into()
                }
            ]
        );
    }

    #[test]
    fn prompt_context_uses_existing_argv_values() {
        let argv = ParsedArgv {
            args: [("name".to_string(), json!("service-a"))]
                .into_iter()
                .collect(),
            options: [("env".to_string(), json!("prod"))].into_iter().collect(),
        };

        let context = prompt_context_from_argv(&argv);

        assert_eq!(context.get("name"), Some(&json!("service-a")));
        assert_eq!(context.get("env"), Some(&json!("prod")));
    }
}
