use std::{collections::BTreeMap, path::Path};

use anyhow::Result;
use lania_format::{FormatMode, FormatOptions, FormatService};
use lania_fs::PlannedFile;
use serde_json::json;

use crate::generate_api_support::default_contract_proto;
use crate::generate_module_manifest::{
    default_module_config, render_generate_module_command_plan, resolve_module_config_path,
};
use crate::models::{
    GenerateModuleWorkflowInput, WorkflowExecution, WorkflowServices, WorkflowState,
};
use crate::workflow_hooks::{call_files_prepare, write_files_with_hooks};

// 初始化路径只负责“生成一个可继续编辑/执行的模块生成骨架”。
// 它不会解析已有 schema，也不会渲染业务模块文件；目标是让用户先拥有
// `lania.module.yaml`、contracts 目录和 generated 输出目录，再进入后续生成流程。
pub(crate) async fn initialize_generate_module(
    services: &WorkflowServices,
    input: &GenerateModuleWorkflowInput,
) -> Result<WorkflowExecution> {
    let config_path = resolve_module_config_path(input);
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input.cwd.clone());
    // 初始化时一次性声明所有需要落盘的默认文件。
    // 后续统一交给 formatter + workflow hooks + 写文件流程处理，
    // 这样 init 路径与正常生成路径共享相同的文件落盘约束。
    let mut files = vec![
        PlannedFile {
            path: config_path.clone(),
            content: default_module_config().to_string(),
        },
        PlannedFile {
            path: config_dir.join("schemas/proto/greeter.proto"),
            content: default_contract_proto().to_string(),
        },
        PlannedFile {
            path: config_dir.join("generated/lania/contracts/.gitkeep"),
            content: String::new(),
        },
        PlannedFile {
            path: config_dir.join("generated/lania/adapters/grpc/.gitkeep"),
            content: String::new(),
        },
        PlannedFile {
            path: config_dir.join("generated/lania/adapters/http/.gitkeep"),
            content: String::new(),
        },
        PlannedFile {
            path: config_dir.join("generated/lania/modules/.gitkeep"),
            content: String::new(),
        },
    ];
    let formatter = FormatService;
    // 即使是默认脚手架，也先走一次 best-effort formatting，
    // 保证初始化出来的配置文件和 proto 示例能直接进入仓库而不显得“半成品”。
    let _format_report = formatter.format_planned_files(
        &services.exec,
        &mut files,
        &FormatOptions {
            enabled: true,
            mode: FormatMode::BestEffort,
            root_dir: Some(config_dir.clone()),
        },
    )?;
    // workflow hooks 仍然可以参与初始化路径：
    // - `files_prepare` 允许在写入前改写默认文件
    // - `write_files_with_hooks` 统一处理覆盖策略与写入结果汇总
    call_files_prepare(services, "generate-module", &config_dir, &mut files).await?;
    let report = write_files_with_hooks(
        services,
        "generate-module",
        &config_dir,
        &files,
        input.force,
    )
    .await?;
    Ok(WorkflowExecution {
        // 初始化结果仍然返回标准 WorkflowExecution，
        // 这样 CLI / bridge 层不需要为 init 模式单独维护一套返回协议。
        workflow: "generate-module".into(),
        state: WorkflowState::Completed,
        target_dir: config_dir.display().to_string(),
        prompts: BTreeMap::from([
            ("config".into(), json!(config_path.display().to_string())),
            ("mode".into(), json!("init")),
            ("force".into(), json!(input.force)),
        ]),
        bridge_steps: Vec::new(),
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
        command_plans: vec![render_generate_module_command_plan(&config_path, input)],
        git_status: None,
        notes: vec![
            "initialized module generation scaffold".into(),
            "created lania.module.yaml, schemas/, generated/lania/, and module output directories."
                .into(),
        ],
        interactive_rendered: false,
    })
}
