//! `command-workspace` 的 create 部分（原 `packages/commands/create` 迁入，去 Plugin 壳）。
//!
//! 内容：
//! - `CreateCommandHandler`：`lan workspace create`（CreateWorkflow + 交互 banner）
//! - `build_input`：CLI argv/options -> `CreateWorkflowInput` 映射
//!
//! spec 构造在 `crate::spec::create_spec`；template 作为 create 的二级子命令
//! （`lan workspace create template`，见 `crate::create_template`）。

use anyhow::Result;
use async_trait::async_trait;
use lania_command::CommandContext;
use lania_host::{
    capability::CapabilityName,
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
};
use lania_logger::{render_ascii_banner, LogLevel};
use lania_workflows::{CreateWorkflow, CreateWorkflowInput};
use std::io::IsTerminal;

pub(crate) struct CreateCommandHandler;

// CLI argv/options -> CreateWorkflowInput 映射。
// `--directory` / `--skip-git` / `--skip-install` 等兼容选项保持旧 create 语义。

pub(crate) fn build_input(context: &CommandContext) -> CreateWorkflowInput {
    let directory = context
        .argv
        .options
        .get("directory")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let path = directory.or_else(|| {
        context
            .argv
            .args
            .get("path")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
    });
    let skip_git = context
        .argv
        .options
        .get("skip-git")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let skip_install = context
        .argv
        .options
        .get("skip-install")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let lint_tools = context
        .argv
        .options
        .get("lint-tools")
        .and_then(|value| value.as_str())
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty());
    let init_git = if skip_git {
        false
    } else {
        // Legacy default: initialize git unless explicitly disabled.
        context
            .argv
            .options
            .get("git")
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    };

    CreateWorkflowInput {
        cwd: context.cwd.clone().into(),
        path,
        project_name: context
            .argv
            .options
            .get("name")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        template: context
            .argv
            .options
            .get("template")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        package_manager: context
            .argv
            .options
            .get("package-manager")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        lint_tools,
        language: context
            .argv
            .options
            .get("language")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        init_git,
        skip_install,
        skip_install_specified: skip_install,
        dry_run: context
            .argv
            .options
            .get("dry-run")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        preview: context
            .argv
            .options
            .get("preview")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    }
}

#[async_trait(?Send)]
impl CommandHandler for CreateCommandHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        ctx.require_capability(CapabilityName::Template)?;
        ctx.emit_workflow_start("create").await;
        let input = build_input(ctx.command());
        let mut execution = ctx
            .run_workflow_with_tasks("create", "Create workflow", move |services| async move {
                let workflow = CreateWorkflow;
                workflow.run(&services, input).await
            })
            .await?;

        let logger = ctx.workflow_services().logger.scoped("command.create");
        let package_manager = execution
            .prompts
            .get("packageManager")
            .and_then(|value| value.as_str())
            .unwrap_or("npm");
        match execution.state {
            lania_workflows::WorkflowState::Completed => {
                let interactive_rendered =
                    std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
                execution.interactive_rendered = interactive_rendered;
                let banner_text = execution
                    .prompts
                    .get("projectName")
                    .and_then(|value| value.as_str())
                    .unwrap_or("lania");
                if interactive_rendered {
                    let banner = render_ascii_banner(banner_text).join("\n");
                    logger.log(LogLevel::Info, format!("\n\x1b[35;1m{banner}\x1b[0m\n"));
                }
                logger.log(
                    LogLevel::Info,
                    format!("project created in {}", execution.target_dir),
                );
                logger.log(LogLevel::Info, "next steps:".to_string());
                logger.log(LogLevel::Info, format!("  cd {}", execution.target_dir));
                logger.log(LogLevel::Info, format!("  {package_manager} run dev"));
            }
            lania_workflows::WorkflowState::Planned => {
                logger.log(
                    LogLevel::Info,
                    format!("create plan prepared for {}", execution.target_dir),
                );
            }
            _ => {}
        }
        ctx.emit_workflow_complete("create").await;
        Ok(ctx.complete_workflow(execution, lania_host::EXIT_SUCCESS))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::build_input;
    use crate::CREATE_HANDLER_ID;

    #[test]
    fn builds_create_input_from_context() {
        let context = lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("name".into(), json!("demo-app")),
                    ("template".into(), json!("toolkit")),
                    ("package-manager".into(), json!("pnpm")),
                    ("git".into(), json!(true)),
                    ("language".into(), json!("ts")),
                    ("skip-install".into(), json!(true)),
                    ("dry-run".into(), json!(true)),
                    ("preview".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: CREATE_HANDLER_ID.into(),
            trace_id: "trace-create".into(),
        };

        let input = build_input(&context);
        assert_eq!(input.project_name.as_deref(), Some("demo-app"));
        assert_eq!(input.template.as_deref(), Some("toolkit"));
        assert_eq!(input.package_manager.as_deref(), Some("pnpm"));
        assert_eq!(input.language.as_deref(), Some("ts"));
        assert!(input.init_git);
        assert!(input.skip_install);
        assert!(input.skip_install_specified);
        assert!(input.dry_run);
        assert!(input.preview);
    }

    #[test]
    fn prefers_directory_option_over_dot_path_and_skip_git() {
        let context = lania_command::CommandContext {
            name: String::new(),
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: [("path".into(), json!("."))].into_iter().collect(),
                options: [
                    ("directory".into(), json!("foo")),
                    ("skip-git".into(), json!(true)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: CREATE_HANDLER_ID.into(),
            trace_id: "trace-create-directory".into(),
        };

        let input = build_input(&context);
        assert_eq!(input.path.as_deref(), Some("foo"));
        assert!(!input.init_git);
        assert!(!input.skip_install);
        assert!(!input.skip_install_specified);
    }
}
