//! `command-workspace` 的 add 部分（原 `packages/commands/add` 迁入，去 Plugin 壳）。
//!
//! 内容：
//! - `AddCommandHandler`：`lan workspace add`（AddWorkflow / `--list` 生成器清单）
//! - `build_input`：CLI argv/options -> `AddWorkflowInput` 映射
//! - `list_catalog`：按框架分组输出生成器清单
//!
//! spec 构造在 `crate::spec::add_spec`。

use anyhow::Result;
use async_trait::async_trait;
use lania_command::CommandContext;
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
};
use lania_workflows::{generator_catalog, AddWorkflow, AddWorkflowInput};

pub(crate) struct AddCommandHandler;

// CLI argv/options -> AddWorkflowInput 映射：
// - `--name` 与位置参数 `generator-name` 二选一（clap 要求 option/位置参数名唯一）
// - `--filepath` 是旧版 target 路径参数名，保留兼容
pub(crate) fn build_input(context: &CommandContext) -> AddWorkflowInput {
    let legacy_filepath = context
        .argv
        .options
        .get("filepath")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let positional_generator = context
        .argv
        .args
        .get("generator")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let positional_name = context
        .argv
        .args
        .get("generator-name")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    AddWorkflowInput {
        cwd: context.cwd.clone().into(),
        name: context
            .argv
            .options
            .get("name")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .or(positional_name),
        template: context
            .argv
            .options
            .get("template")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .or(positional_generator),
        target: context
            .argv
            .options
            .get("target")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .or(legacy_filepath),
        force: context
            .argv
            .options
            .get("force")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    }
}

/// `lan workspace add --list`：按框架分组输出生成器清单。
///
/// 框架探测失败时**只列通用生成器**（§4.5.3），不猜框架。
///
/// 分组依据是探测结果：探测到框架时返回「该框架 + 通用」两组；
/// 未探测到时只返回 `generic` 一组。`generators` 保持平铺，兼容旧消费者。
pub(crate) fn list_catalog(cwd: &std::path::Path, locale: &str) -> serde_json::Value {
    let catalog = generator_catalog(cwd, None);
    let framework = catalog["framework"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let generic = catalog["generic"].as_bool().unwrap_or(true);
    let in_scope = |generator: &serde_json::Value| {
        let entry_framework = generator["framework"].as_str().unwrap_or_default();
        entry_framework.is_empty() || (!generic && entry_framework == framework)
    };
    let generators = catalog["generators"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(in_scope)
        .collect::<Vec<_>>();
    let mut groups = Vec::new();
    if !generic {
        groups.push(serde_json::json!({
            "framework": framework,
            "generators": generators
                .iter()
                .filter(|generator| generator["framework"].as_str() == Some(framework.as_str()))
                .cloned()
                .collect::<Vec<_>>(),
        }));
    }
    groups.push(serde_json::json!({
        "framework": serde_json::Value::Null,
        "generators": generators
            .iter()
            .filter(|generator| generator["framework"].as_str() == Some(""))
            .cloned()
            .collect::<Vec<_>>(),
    }));
    serde_json::json!({
        "kind": "template_info",
        "framework": if generic { serde_json::Value::Null } else { serde_json::json!(framework) },
        "frameworkReason": catalog["frameworkReason"],
        "locale": locale,
        "generators": generators,
        "groups": groups,
    })
}

#[async_trait(?Send)]
impl CommandHandler for AddCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::Template)?;
        let input = build_input(ctx.command());
        if ctx.bool_option("list") {
            let catalog = list_catalog(&input.cwd, ctx.locale());
            return Ok(ctx.complete_template_info(catalog, lania_host::EXIT_SUCCESS));
        }
        ctx.emit_workflow_start("add").await;
        let workflow = AddWorkflow;
        let execution = workflow.run(&ctx.workflow_services(), input).await?;
        ctx.emit_workflow_complete("add").await;
        Ok(ctx.complete_workflow(execution, lania_host::EXIT_SUCCESS))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{build_input, list_catalog};
    use crate::ADD_HANDLER_ID;

    #[test]
    fn builds_add_input_from_positional_args() {
        let context = lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: [
                    ("generator".into(), json!("resource")),
                    ("generator-name".into(), json!("users")),
                ]
                .into_iter()
                .collect(),
                options: Default::default(),
            },
            handler_id: ADD_HANDLER_ID.into(),
            trace_id: "trace-add-positional".into(),
        };

        let input = build_input(&context);
        assert_eq!(input.template.as_deref(), Some("resource"));
        assert_eq!(input.name.as_deref(), Some("users"));
    }

    #[test]
    fn list_catalog_only_shows_generic_generators_without_a_framework() {
        let root = std::env::temp_dir().join(format!("lania-add-list-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temp dir");
        let catalog = list_catalog(&root, "en");
        let generators = catalog["generators"].as_array().expect("generators");
        let names: Vec<&str> = generators
            .iter()
            .filter_map(|generator| generator["name"].as_str())
            .collect();
        assert_eq!(names, vec!["component", "config"]);
        assert!(catalog["framework"].is_null());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn list_catalog_includes_framework_generators_when_detected() {
        let root = std::env::temp_dir().join(format!("lania-add-list-nest-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temp dir");
        std::fs::write(root.join("nest-cli.json"), "{}").expect("marker");
        let catalog = list_catalog(&root, "en");
        assert_eq!(catalog["framework"], "nestjs");
        let names: Vec<&str> = catalog["generators"]
            .as_array()
            .expect("generators")
            .iter()
            .filter_map(|generator| generator["name"].as_str())
            .collect();
        assert!(names.contains(&"resource"));
        assert!(names.contains(&"component"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn list_catalog_groups_generators_by_framework() {
        let root =
            std::env::temp_dir().join(format!("lania-add-list-groups-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temp dir");
        std::fs::write(
            root.join("go.mod"),
            "module example.com/project\n\nrequire github.com/gin-gonic/gin v1.10.0\n",
        )
        .expect("marker");
        let catalog = list_catalog(&root, "en");
        assert_eq!(catalog["framework"], "gin");
        let groups = catalog["groups"].as_array().expect("groups");
        assert_eq!(groups.len(), 2, "framework + generic: {catalog}");
        assert_eq!(groups[0]["framework"], "gin");
        let gin_names: Vec<&str> = groups[0]["generators"]
            .as_array()
            .expect("gin generators")
            .iter()
            .filter_map(|generator| generator["name"].as_str())
            .collect();
        assert!(gin_names.contains(&"resource"), "{gin_names:?}");
        assert!(gin_names.contains(&"handler"), "{gin_names:?}");
        let generic_names: Vec<&str> = groups[1]["generators"]
            .as_array()
            .expect("generic generators")
            .iter()
            .filter_map(|generator| generator["name"].as_str())
            .collect();
        assert_eq!(generic_names, vec!["component", "config"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn builds_add_input_from_context() {
        let context = lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("name".into(), json!("button")),
                    ("template".into(), json!("spa-vue")),
                    ("target".into(), json!("packages")),
                    ("force".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: ADD_HANDLER_ID.into(),
            trace_id: "trace-add".into(),
        };

        let input = build_input(&context);
        assert_eq!(input.name.as_deref(), Some("button"));
        assert_eq!(input.template.as_deref(), Some("spa-vue"));
        assert_eq!(input.target.as_deref(), Some("packages"));
        assert!(input.force);
    }

    #[test]
    fn prefers_target_and_accepts_legacy_filepath() {
        let filepath_only = lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("filepath".into(), json!("/src/components"))]
                    .into_iter()
                    .collect(),
            },
            handler_id: ADD_HANDLER_ID.into(),
            trace_id: "trace-add-filepath".into(),
        };
        let filepath_input = build_input(&filepath_only);
        assert_eq!(filepath_input.target.as_deref(), Some("/src/components"));

        let target_wins = lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("filepath".into(), json!("/src/legacy")),
                    ("target".into(), json!("src/current")),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: ADD_HANDLER_ID.into(),
            trace_id: "trace-add-target".into(),
        };
        let target_input = build_input(&target_wins);
        assert_eq!(target_input.target.as_deref(), Some("src/current"));
    }
}
