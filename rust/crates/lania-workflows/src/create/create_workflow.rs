//! `create` 主工作流：把“创建项目”从交互到写盘串成一条完整链路。
//!
//! `CreateWorkflow::run()` 大致分 3 段：
//! 1) 采集输入：模板选择 + 模板问题 + 推断 projectName/path/packageManager
//! 2) 生成计划：依赖/任务/模板渲染 -> `PlannedFile` 列表 +（可选）bridge steps
//! 3) 落地执行：格式化、写文件、安装依赖、初始化 Git、汇总 `WorkflowExecution`
//!
//! 与其把它当成“纯函数”，不如把它当成“编排器”：它自己尽量不做细节，
//! 而是调用 `prompts.rs`/`capability.rs`/`helpers.rs` 等模块完成具体工作。

use anyhow::{anyhow, Result};
use lania_exec::{ExecError, ExecErrorCode};
use lania_format::{FormatMode, FormatOptions, FormatService};
use lania_fs::PlannedFile;
use lania_logger::LogLevel;
use serde_json::{json, Value};

use crate::models::{
    redact_prompt_answers, CreateWorkflow, CreateWorkflowInput, TemplateCapability,
    WorkflowExecution, WorkflowServices, WorkflowState,
};
use crate::workflow_hooks::{
    call_dependencies_modify, call_files_prepare, call_template_parse, write_files_with_hooks,
};

use super::helpers::*;
use super::prompts::{build_template_question_options, run_create_prompt, run_template_prompt};

impl CreateWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: CreateWorkflowInput,
    ) -> Result<WorkflowExecution> {
        // `CreateWorkflow::run` 可以粗略理解成 3 个阶段：
        // 1. 采集输入：列模板、问问题、补齐 projectName/path/packageManager 等信息
        // 2. 生成计划：算依赖、拿 output tasks、渲染模板、构造 PlannedFile 列表
        // 3. 落地执行：格式化、写文件、安装依赖、初始化 Git、汇总结果
        let logger = services.logger.scoped("workflow.create");
        let capability = TemplateCapability::new(&services.bridge);
        let mut bridge_steps = Vec::new();
        let mut notes = Vec::new();

        let (templates, list_step) = capability.list(&input.cwd).await?;
        if let Some(step) = list_step {
            bridge_steps.push(step);
        }
        let dry_run_like = input.dry_run || input.preview;
        logger.log(
            LogLevel::Debug,
            format!(
                "create workflow: cwd={} dry_run={} preview={}",
                input.cwd.display(),
                input.dry_run,
                input.preview
            ),
        );

        // 兼容旧行为（legacy parity）：`create .` 要求当前目录必须为空。
        if input.path.as_deref() == Some(".") {
            ensure_directory_empty(&input.cwd)?;
        }

        // 首先运行 prompt 来获取模板和其他信息
        // prompt 交互期应尽量“独占 stderr”，否则进度条 spinner 会不断重绘，导致终端显示抖动。
        // 这里 suspend 终端进度条，是因为 prompt 和 spinner 都会占用终端输出，
        // 如果同时刷新，用户会看到“问题文本和进度条互相打架”的错乱效果。
        let _progress_guard = services.progress.suspend_terminal_guard();
        let mut prompt_state = run_create_prompt(
            &services.prompt,
            services.locale.as_str(),
            &input,
            &templates,
        )?;
        let template_name = prompt_state["template"]
            .as_str()
            .ok_or_else(|| anyhow!("create prompt did not resolve template"))?
            .to_string();
        notes.push(format!("template: {template_name}"));

        // 确定目标目录和项目名称
        let (target_dir, project_name) = if let Some(path) = &input.path {
            if path == "." {
                // 使用当前目录，从目录名推断项目名
                let cwd = &input.cwd;
                let dir_name = cwd
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("lania-app")
                    .to_string();

                // 如果用户没有明确指定项目名，使用目录名或从 prompt 中获取
                let final_project_name = input
                    .project_name
                    .clone()
                    .or_else(|| {
                        prompt_state
                            .get("projectName")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                    .unwrap_or(dir_name);
                (cwd.clone(), final_project_name)
            } else {
                // 使用指定路径
                let target_dir = input.cwd.join(path);
                let project_name = input
                    .project_name
                    .clone()
                    .or_else(|| {
                        prompt_state
                            .get("projectName")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                    .unwrap_or_else(|| {
                        target_dir
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("lania-app")
                            .to_string()
                    });
                (target_dir, project_name)
            }
        } else {
            // 没有 path 参数，走原来的流程
            let project_name = prompt_state
                .get("projectName")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("create prompt did not resolve project name"))?
                .to_string();
            (input.cwd.join(&project_name), project_name)
        };

        // 更新 prompt_state 中的 projectName
        prompt_state.insert("projectName".into(), json!(&project_name));

        logger.log(
            LogLevel::Debug,
            format!(
                "create workflow: template={} project={} target={}",
                template_name,
                project_name,
                target_dir.display()
            ),
        );

        if !templates.iter().any(|item| item == &template_name) {
            return Err(anyhow!("unknown template: {}", template_name));
        }
        let question_options =
            build_template_question_options(&input, &prompt_state, services.locale.as_str());
        let (questions, question_step) = capability
            .questions(&template_name, question_options)
            .await?;
        if let Some(step) = question_step {
            bridge_steps.push(step);
        }
        let template_prompt_state = {
            let _progress_guard = services.progress.suspend_terminal_guard();
            run_template_prompt(
                &services.prompt,
                &template_name,
                &questions,
                &prompt_state,
                &input,
            )?
        };
        prompt_state.extend(template_prompt_state);
        if let Some(package_manager) = &input.package_manager {
            prompt_state
                .entry("packageManager".into())
                .or_insert_with(|| json!(package_manager));
        }
        prompt_state
            .entry("skipInstall".into())
            .or_insert_with(|| json!(input.skip_install));
        prompt_state.insert("skipGit".into(), json!(!input.init_git));
        prompt_state.insert("dryRun".into(), json!(dry_run_like));
        prompt_state.insert("preview".into(), json!(input.preview));
        // Legacy parity: templates expect a computed port injected for vite/webpack dev config.
        // If user/template already provided one, do not override it.
        prompt_state
            .entry("port".into())
            .or_insert_with(|| json!(find_available_port("127.0.0.1", 8089)));
        if let Some(language) = &input.language {
            // Legacy compatibility: allow templates to branch on "language" (e.g. js/ts).
            prompt_state.insert("language".into(), json!(language));
        }
        let mut template_runtime_options =
            build_template_question_options(&input, &prompt_state, services.locale.as_str());

        // 模板依赖分两步：
        // 1. 模板先返回“逻辑依赖列表”（可能没有版本、也可能会被 hook 改写）
        // 2. Rust 侧再解析出最终版本，形成真正可安装的依赖集合
        let (mut dependencies, mut dev_dependencies, deps_step) = capability
            .dependencies(&template_name, template_runtime_options.clone())
            .await?;
        if let Some(step) = deps_step {
            bridge_steps.push(step);
        }
        let manager = resolve_package_manager(
            &services.package_manager,
            prompt_state
                .get("packageManager")
                .and_then(Value::as_str)
                .or(input.package_manager.as_deref()),
        );
        call_dependencies_modify(
            services,
            "create",
            manager.binary(),
            &mut dependencies,
            &mut dev_dependencies,
        )
        .await?;
        let (resolved_dependencies, resolved_dev_dependencies) = resolve_dependency_versions(
            services,
            manager,
            &target_dir,
            &dependencies,
            &dev_dependencies,
        )
        .await?;
        template_runtime_options
            .as_object_mut()
            .expect("template runtime options object")
            .insert("resolvedDependencies".into(), json!(resolved_dependencies));
        template_runtime_options
            .as_object_mut()
            .expect("template runtime options object")
            .insert(
                "resolvedDevDependencies".into(),
                json!(resolved_dev_dependencies),
            );
        let (output_tasks, tasks_step) = capability
            .output_tasks(&template_name, template_runtime_options.clone())
            .await?;
        if let Some(step) = tasks_step {
            bridge_steps.push(step);
        }
        notes.push(format!("output tasks: {}", output_tasks.join(", ")));

        let (rendered_files, render_step) = capability
            .render(
                &template_name,
                serde_json::to_value(prompt_state.clone())?,
                template_runtime_options,
            )
            .await?;
        if let Some(step) = render_step {
            bridge_steps.push(step);
        } else {
            notes.push(format!(
                "template {} rendered via rust declarative rules",
                template_name
            ));
        }
        let mut files = rendered_files
            .into_iter()
            .map(|file| PlannedFile {
                path: target_dir.join(file.path),
                content: file.content,
            })
            .collect::<Vec<_>>();
        // 到这里模板只负责“给出应该写哪些文件”，真正写磁盘仍由 Rust 宿主控制。
        // 这也是这个项目里一个很重要的边界设计：模板语义在 Node，事务控制在 Rust。
        call_template_parse(
            services,
            "create",
            &template_name,
            &serde_json::to_value(prompt_state.clone())?,
            &target_dir,
            &mut files,
        )
        .await?;
        let preview_files = files
            .iter()
            .map(|file| {
                file.path
                    .strip_prefix(&target_dir)
                    .unwrap_or(file.path.as_path())
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>();
        prompt_state.insert("previewFiles".into(), json!(preview_files));
        services.progress.advance("create", 1);

        if input.preview {
            notes.push(format!(
                "template preview: {} files would be created",
                files.len()
            ));
            for path in prompt_state["previewFiles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(12)
            {
                notes.push(format!("preview file: {path}"));
            }
            if files.len() > 12 {
                notes.push(format!(
                    "preview file list truncated: {} more",
                    files.len() - 12
                ));
            }
        }

        if !input.preview {
            logger.log(
                LogLevel::Debug,
                format!("create workflow: rendering {} files", files.len()),
            );
        }

        let report = if dry_run_like {
            // dry-run / preview 的核心是不碰磁盘：
            // 仍然完整走到“渲染出最终文件计划”这一步，
            // 只是把真正的写入动作替换成一份空报告。
            notes.push("dry-run: files were rendered but not written".into());
            lania_fs::WriteReport {
                written: Vec::new(),
                conflicts: Vec::new(),
            }
        } else {
            let formatter = FormatService;
            // `BestEffort` 表示“能格式化就格式化，失败也不要中断整个 create 流程”。
            // 对脚手架类命令来说，这通常比“一个 formatter 失败就整个创建失败”更友好。
            let format_report = formatter.format_planned_files(
                &services.exec,
                &mut files,
                &FormatOptions {
                    enabled: true,
                    mode: FormatMode::BestEffort,
                    // Treat the generated project directory as the formatting root.
                    root_dir: Some(target_dir.clone()),
                },
            )?;
            if format_report.formatted_count() > 0 {
                notes.push(format!(
                    "formatted {} generated files",
                    format_report.formatted_count()
                ));
            }
            if format_report.failed_count() > 0 {
                notes.push(format!(
                    "formatters failed for {} files (best-effort: kept original content)",
                    format_report.failed_count()
                ));
            }
            logger.log(
                LogLevel::Debug,
                format!("create workflow: writing {} files", files.len()),
            );
            call_files_prepare(services, "create", &target_dir, &mut files).await?;
            write_files_with_hooks(services, "create", &target_dir, &files, false).await?
        };
        if !dry_run_like {
            logger.log(
                LogLevel::Debug,
                format!(
                    "create workflow: wrote {} files ({} conflicts)",
                    report.written.len(),
                    report.conflicts.len()
                ),
            );
        }
        let mut command_plans = Vec::new();
        let effective_skip_install = input.skip_install
            || prompt_state
                .get("skipInstall")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if effective_skip_install {
            notes.push("skip-install: package manager commands were skipped".into());
        } else {
            let install = services.package_manager.install_all_command(manager);
            command_plans.push(command_to_vec(&install));
            if !dry_run_like {
                if let Err(error) =
                    run_package_command(services, "create", &target_dir, &install).await
                {
                    if error
                        .downcast_ref::<ExecError>()
                        .is_some_and(|exec_error| exec_error.code == ExecErrorCode::BinaryMissing)
                    {
                        notes.push(format!(
                            "skip-install: {} is unavailable; scaffold created without installing dependencies",
                            manager.binary()
                        ));
                    } else {
                        return Err(error);
                    }
                }
            }
            if dry_run_like {
                notes
                    .push("dry-run: package manager commands were planned but not executed".into());
            }
        }
        let git_status = if input.init_git && !dry_run_like {
            services.fs.ensure_dir(&target_dir)?;
            command_plans.push(
                std::iter::once("git".to_string())
                    .chain(services.git.plan_init())
                    .collect(),
            );
            services.git.init(&target_dir)?;
            Some(services.git.status(&target_dir)?)
        } else if input.init_git {
            command_plans.push(
                std::iter::once("git".to_string())
                    .chain(services.git.plan_init())
                    .collect(),
            );
            None
        } else {
            None
        };
        if input.init_git && dry_run_like {
            notes.push("dry-run: git init was skipped".into());
        }
        services
            .tasks
            .complete("create", "Create workflow completed");

        Ok(WorkflowExecution {
            workflow: "create".into(),
            state: if dry_run_like {
                WorkflowState::Planned
            } else {
                WorkflowState::Completed
            },
            target_dir: target_dir.display().to_string(),
            prompts: redact_prompt_answers(&prompt_state, &services.prompt.secret_fields()),
            bridge_steps,
            written_files: report
                .written
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
            conflicts: report
                .conflicts
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
            command_plans,
            git_status,
            notes,
            interactive_rendered: false,
        })
    }
}
