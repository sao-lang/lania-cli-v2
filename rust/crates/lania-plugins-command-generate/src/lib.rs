//! 将 `generate` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 主要导出：spec、api_spec、api_plan_spec、api_diff_spec、api_init_spec、module_spec。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含序列化/反序列化与 JSON 结构约定
use anyhow::{bail, Result};
use async_trait::async_trait;
use lania_command::CommandContext;
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
    plugin::{register_builtin_command, Plugin, PluginKind, PluginMeta, PluginSetupContext},
};
use lania_node_bridge::{BridgeRequest, NodeBridgeClient};
use lania_prompt::{PromptFallbackStrategy, PromptFlow, PromptRunOptions, PromptStep, PromptStepKind};
use lania_workflows::{GenerateApiWorkflow, GenerateModuleWorkflow};
use std::io::{stdin, IsTerminal};

pub const HANDLER_ID: &str = "command.generate";
pub const PRODUCT_HANDLER_ID: &str = "command.generate.product";
pub const API_HANDLER_ID: &str = "command.generate.api";
pub const API_PLAN_HANDLER_ID: &str = "command.generate.api.plan";
pub const API_DIFF_HANDLER_ID: &str = "command.generate.api.diff";
pub const API_INIT_HANDLER_ID: &str = "command.generate.api.init";
pub const MODULE_HANDLER_ID: &str = "command.generate.module";
pub const MODULE_PLAN_HANDLER_ID: &str = "command.generate.module.plan";
pub const MODULE_DIFF_HANDLER_ID: &str = "command.generate.module.diff";
pub const MODULE_INIT_HANDLER_ID: &str = "command.generate.module.init";
pub const MODULE_APPLY_HANDLER_ID: &str = "command.generate.module.apply";

#[derive(Debug, Default)]
pub struct GenerateCommandPlugin;

struct GenerateCommandHandler;

mod specs;

impl Plugin for GenerateCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-generate".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Fs,
                CapabilityName::Task,
                CapabilityName::Progress,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<GenerateCommandHandler, _>(
            ctx,
            "command-generate",
            "generate",
            Self::spec(),
            &[
                HANDLER_ID,
                PRODUCT_HANDLER_ID,
                API_HANDLER_ID,
                API_PLAN_HANDLER_ID,
                API_DIFF_HANDLER_ID,
                API_INIT_HANDLER_ID,
                MODULE_HANDLER_ID,
                MODULE_PLAN_HANDLER_ID,
                MODULE_DIFF_HANDLER_ID,
                MODULE_INIT_HANDLER_ID,
                MODULE_APPLY_HANDLER_ID,
            ],
            || GenerateCommandHandler,
        )?;
        ctx.commands
            .mount_subcommand("product", Self::product_generate_spec())?;
        ctx.commands
            .mount_subcommand("generate", Self::api_spec())?;
        ctx.commands
            .mount_subcommand("generate", Self::module_spec())?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl CommandHandler for GenerateCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::Fs)?;
        if ctx.command().handler_id == HANDLER_ID {
            bail!(
                "{}",
                if ctx.locale() == "zh" {
                    "缺少 generate 子命令，请使用 `lan product generate`、`lan generate api`、`lan generate module`"
                } else {
                    "missing generate subcommand, try `lan product generate`, `lan generate api`, `lan generate module`"
                }
            );
        }
        match ctx.command().handler_id.as_str() {
            PRODUCT_HANDLER_ID => {
                ctx.require_capability(CapabilityName::NodeBridge)?;
                let request = GenerateCommandPlugin::build_product_request_interactive(
                    ctx,
                    ctx.command(),
                    ctx.node_bridge(),
                )?;
                let run = ctx.call_bridge(request).await?;
                Ok(ctx.complete_bridge(run, lania_host::EXIT_SUCCESS))
            }
            MODULE_HANDLER_ID
            | MODULE_PLAN_HANDLER_ID
            | MODULE_DIFF_HANDLER_ID
            | MODULE_INIT_HANDLER_ID
            | MODULE_APPLY_HANDLER_ID => {
                ctx.emit_workflow_start("generate-module").await;
                let input = GenerateCommandPlugin::build_module_input(ctx.command());
                let input_for_task = input.clone();
                let execution = ctx
                    .run_workflow_with_tasks(
                        "generate-module",
                        "Generate module workflow",
                        move |services| async move {
                            let workflow = GenerateModuleWorkflow;
                            workflow.run(&services, input_for_task).await
                        },
                    )
                    .await?;
                ctx.emit_workflow_complete("generate-module").await;
                let exit_code = generate_exit_code(input.check, &execution);
                Ok(ctx.complete_workflow(execution, exit_code))
            }
            _ => {
                ctx.emit_workflow_start("generate-api").await;
                let input = GenerateCommandPlugin::build_api_input(ctx.command());
                let input_for_task = input.clone();
                let execution = ctx
                    .run_workflow_with_tasks(
                        "generate-api",
                        "Generate API workflow",
                        move |services| async move {
                            let workflow = GenerateApiWorkflow;
                            workflow.run(&services, input_for_task).await
                        },
                    )
                    .await?;
                ctx.emit_workflow_complete("generate-api").await;
                let exit_code = generate_exit_code(input.check, &execution);
                Ok(ctx.complete_workflow(execution, exit_code))
            }
        }
    }
}

fn generate_exit_code(check: bool, execution: &lania_workflows::WorkflowExecution) -> i32 {
    if check
        && (execution
            .notes
            .iter()
            .any(|note| note.contains("drift detected"))
            || !execution.conflicts.is_empty())
    {
        lania_host::EXIT_LINT_FAILED
    } else {
        lania_host::EXIT_SUCCESS
    }
}

fn split_csv(value: Option<&str>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

impl GenerateCommandPlugin {
    pub fn build_product_request(
        context: &CommandContext,
        bridge: &NodeBridgeClient,
    ) -> BridgeRequest {
        let mut params = serde_json::Map::new();
        params.insert("cwd".into(), serde_json::Value::String(context.cwd.clone()));
        for (option_name, param_name) in [
            ("preset", "preset"),
            ("name", "name"),
            ("binary-name", "binaryName"),
            ("package-name", "packageName"),
            ("output-dir", "outputDir"),
        ] {
            if let Some(value) = context
                .argv
                .options
                .get(option_name)
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
            {
                params.insert(param_name.into(), serde_json::Value::String(value));
            }
        }
        if context
            .argv
            .options
            .get("force")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            params.insert("force".into(), serde_json::Value::Bool(true));
        }
        bridge.request("product.generate", serde_json::Value::Object(params))
    }

    pub fn build_product_request_interactive(
        ctx: &CommandExecutionContext<'_>,
        context: &CommandContext,
        bridge: &NodeBridgeClient,
    ) -> Result<BridgeRequest> {
        let interactive = context
            .argv
            .options
            .get("interactive")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let mut preset = context
            .argv
            .options
            .get("preset")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "demo".into());
        let mut name = context
            .argv
            .options
            .get("name")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let mut binary_name = context
            .argv
            .options
            .get("binary-name")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let mut package_name = context
            .argv
            .options
            .get("package-name")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let mut output_dir = context
            .argv
            .options
            .get("output-dir")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let force = context
            .argv
            .options
            .get("force")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let should_prompt = interactive || name.is_none();
        if should_prompt && stdin().is_terminal() {
            let zh = ctx.locale() == "zh";
            let flow = PromptFlow::new()
                .step(
                    PromptStep::new(
                        "preset",
                        if zh { "生成预置" } else { "Preset" },
                        "preset",
                    )
                    .kind(PromptStepKind::Select)
                    .choice("demo", serde_json::json!("demo"))
                    .choice("minimal", serde_json::json!("minimal"))
                    .default_value(serde_json::json!(preset.clone())),
                )
                .step(
                    PromptStep::new(
                        "name",
                        if zh { "产品展示名" } else { "Product display name" },
                        "name",
                    )
                    .kind(PromptStepKind::Input)
                    .default_value(serde_json::json!(
                        name.clone().unwrap_or_else(|| "Acme CLI".into())
                    )),
                )
                .step(
                    PromptStep::new(
                        "binary-name",
                        if zh { "命令名（binaryName）" } else { "Binary name" },
                        "binaryName",
                    )
                    .kind(PromptStepKind::Input)
                    .default_value(serde_json::json!(
                        binary_name.clone().unwrap_or_else(|| "acme".into())
                    )),
                )
                .step(
                    PromptStep::new(
                        "output-dir",
                        if zh { "输出目录" } else { "Output directory" },
                        "outputDir",
                    )
                    .kind(PromptStepKind::Input)
                    .default_value(serde_json::json!(
                        output_dir
                            .clone()
                            .unwrap_or_else(|| "products/acme-cli".into())
                    )),
                )
                .step(
                    PromptStep::new(
                        "package-name",
                        if zh { "包名（可选）" } else { "Package name (optional)" },
                        "packageName",
                    )
                    .kind(PromptStepKind::Input)
                    .default_value(serde_json::json!(package_name.clone().unwrap_or_default())),
                );

            let state = ctx.prompt().run_cli_with_options(
                &flow,
                PromptRunOptions {
                    fallback: Some(PromptFallbackStrategy::Error),
                    ..PromptRunOptions::default()
                },
            )?;
            if state.interrupted {
                bail!("{}", if zh { "已取消" } else { "cancelled" });
            }

            if let Some(value) = state.answers.get("preset").and_then(|v| v.as_str()) {
                preset = value.to_string();
            }
            if let Some(value) = state.answers.get("name").and_then(|v| v.as_str()) {
                name = Some(value.to_string());
            }
            if let Some(value) = state.answers.get("binaryName").and_then(|v| v.as_str()) {
                binary_name = Some(value.to_string());
            }
            if let Some(value) = state.answers.get("outputDir").and_then(|v| v.as_str()) {
                output_dir = Some(value.to_string());
            }
            if let Some(value) = state.answers.get("packageName").and_then(|v| v.as_str()) {
                let trimmed = value.trim();
                package_name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
        }

        if name.is_none() && binary_name.is_none() && package_name.is_none() {
            bail!(
                "{}",
                if ctx.locale() == "zh" {
                    "缺少必要参数：请使用 `--interactive`，或至少提供 --name/--binary-name/--package-name 之一"
                } else {
                    "missing required args: use `--interactive` or pass --name/--binary-name/--package-name"
                }
            );
        }

        let mut params = serde_json::Map::new();
        params.insert("cwd".into(), serde_json::Value::String(context.cwd.clone()));
        params.insert("preset".into(), serde_json::Value::String(preset));
        if let Some(value) = name {
            params.insert("name".into(), serde_json::Value::String(value));
        }
        if let Some(value) = binary_name {
            params.insert("binaryName".into(), serde_json::Value::String(value));
        }
        if let Some(value) = package_name {
            params.insert("packageName".into(), serde_json::Value::String(value));
        }
        if let Some(value) = output_dir {
            params.insert("outputDir".into(), serde_json::Value::String(value));
        }
        if force {
            params.insert("force".into(), serde_json::Value::Bool(true));
        }

        Ok(bridge.request(
            "product.generate",
            serde_json::Value::Object(params),
        ))
    }
}

#[cfg(test)]
mod tests;
