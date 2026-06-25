use std::{collections::BTreeMap, path::Path};

use anyhow::Result;
use lania_format::{FormatMode, FormatOptions, FormatService};
use lania_fs::PlannedFile;
use serde_json::json;

use crate::generate_api_support::render_generate_command_plan;
use crate::models::{GenerateApiWorkflowInput, WorkflowExecution, WorkflowServices, WorkflowState};
use crate::workflow_hooks::{call_files_prepare, write_files_with_hooks};

pub(crate) async fn initialize_generate_api(
    services: &WorkflowServices,
    input: &GenerateApiWorkflowInput,
) -> Result<WorkflowExecution> {
    // `init` 模式只负责“初始化目录结构 + 放一份示例配置/示例 schema”，
    // 它不会去读取现有 manifest，也不会生成真实业务代码。
    let config_path = resolve_contract_config_path(input);
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input.cwd.clone());
    let config_body = default_contract_config();
    let sample_proto = default_contract_proto();
    let mut files = vec![
        PlannedFile {
            path: config_path.clone(),
            content: config_body.to_string(),
        },
        PlannedFile {
            path: config_dir.join("schemas/proto/greeter.proto"),
            content: sample_proto.to_string(),
        },
        PlannedFile {
            path: config_dir.join("contracts/generated/.gitkeep"),
            content: String::new(),
        },
        PlannedFile {
            path: config_dir.join("transport/generated/.gitkeep"),
            content: String::new(),
        },
        PlannedFile {
            path: config_dir.join("modules/.gitkeep"),
            content: String::new(),
        },
    ];
    let formatter = FormatService;
    // 初始化阶段也会格式化输出，原因是：
    // - 生成器产物应尽量“开箱即用、无噪音 diff”
    // - 用户第一次打开文件就能看到干净的格式
    let _format_report = formatter.format_planned_files(
        &services.exec,
        &mut files,
        &FormatOptions {
            enabled: true,
            mode: FormatMode::BestEffort,
            root_dir: Some(config_dir.clone()),
        },
    )?;
    call_files_prepare(services, "generate-api", &config_dir, &mut files).await?;
    // 所有写文件动作都走 hooks 管线，保持和其它 workflow 一致：
    // - allow rewrite planned files
    // - emit onFileWrite events
    let report =
        write_files_with_hooks(services, "generate-api", &config_dir, &files, input.force).await?;
    Ok(WorkflowExecution {
        workflow: "generate-api".into(),
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
        command_plans: vec![render_generate_command_plan(&config_path, input)],
        git_status: None,
        notes: vec![
            "initialized contract generation scaffold".into(),
            "created lania.contract.yaml, schemas/, contracts/generated/, transport/generated/, and modules/".into(),
        ],
        interactive_rendered: false,
    })
}

pub(crate) fn default_contract_config() -> &'static str {
    "version: 1

defaults:
  language: go
  output:
    contractDir: contracts/generated
    transportDir: transport/generated
    moduleFile: modules/generated_module.gen.go
    manifest: .lania/contracts.lock.json

entries:
  - name: greeter-service
    source:
      kind: proto
      inputs:
        - schemas/proto/greeter.proto
    targets:
      - grpc
      - http
"
}

pub(crate) fn default_contract_proto() -> &'static str {
    "message HelloRequest {
  string name = 1;
}

message HelloReply {
  string message = 1;
}

service GreeterService {
  rpc SayHello (HelloRequest) returns (HelloReply);
}
"
}

pub(crate) fn resolve_contract_config_path(input: &GenerateApiWorkflowInput) -> std::path::PathBuf {
    // 配置路径解析规则与其它 workflow 一样：
    // - CLI 显式传绝对路径：直接使用
    // - 传相对路径或省略：相对 `input.cwd` 解析
    let raw = input
        .config_path
        .as_deref()
        .unwrap_or("lania.contract.yaml");
    let path = std::path::PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        input.cwd.join(path)
    }
}
