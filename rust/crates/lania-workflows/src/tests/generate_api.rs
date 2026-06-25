//! API 生成功能的回归测试。
//!
//! 关键点：
//! - 包含异步/超时/取消等控制流
use super::*;

#[tokio::test]
async fn generate_api_workflow_writes_proto_outputs_and_manifest() {
    let (services, root) = services(ExecService::dry_run());
    write_contract_config(
        &root,
        r#"version: 1
defaults:
  language: go
  output:
    contractDir: contracts/generated
    transportDir: transport/generated
    moduleFile: modules/generated_module.gen.go
    manifest: .lania/contracts.lock.json
entries:
  - name: user-service
    source:
      kind: proto
      inputs:
        - schemas/proto/user.proto
    targets:
      - grpc
      - http
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"message User {
  string id = 1;

service UserService {
  rpc GetUser (User) returns (User);
"#,
    );

    let workflow = GenerateApiWorkflow;
    let result = workflow
        .run(&services, generate_input(&root))
        .await
        .expect("generate api workflow succeeds");

    assert_eq!(result.workflow, "generate-api");
    assert!(root
        .join("contracts/generated/user_service.contract.gen.go")
        .exists());
    assert!(root
        .join("transport/generated/grpc/user_service_grpc.gen.go")
        .exists());
    assert!(root
        .join("transport/generated/http/user_service_http.gen.go")
        .exists());
    assert!(root.join("modules/generated_module.gen.go").exists());
    assert!(root.join(".lania/contracts.lock.json").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_api_workflow_supports_thrift_dry_run_and_incremental_skip() {
    let (services, root) = services(ExecService::dry_run());
    write_contract_config(
        &root,
        r#"version: 1
defaults:
  output:
    manifest: .lania/contracts.lock.json
entries:
  - name: order-service
    source:
      kind: thrift
      inputs:
        - schemas/thrift/order.thrift
    targets:
      - http
"#,
    );
    write_proto_schema(
        &root,
        "schemas/thrift/order.thrift",
        r#"struct Order {
  1: string id
}

service OrderService {
  Order getOrder(1: string id)
}
"#,
    );

    let workflow = GenerateApiWorkflow;
    let preview = workflow
        .run(
            &services,
            GenerateApiWorkflowInput {
                dry_run: true,
                ..generate_input(&root)
            },
        )
        .await
        .expect("dry run succeeds");
    assert_eq!(preview.state, WorkflowState::Planned);
    assert!(!root
        .join("contracts/generated/order_service.contract.gen.go")
        .exists());

    workflow
        .run(&services, generate_input(&root))
        .await
        .expect("first write succeeds");
    let second = workflow
        .run(&services, generate_input(&root))
        .await
        .expect("second write succeeds");

    assert!(second
        .notes
        .iter()
        .any(|note| note.contains("incremental skip")));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_api_workflow_detects_unmanaged_conflicts() {
    let (services, root) = services(ExecService::dry_run());
    write_contract_config(
        &root,
        r#"version: 1
entries:
  - name: user-service
    source:
      kind: proto
      inputs:
        - schemas/proto/user.proto
    targets:
      - grpc
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"message User {
  string id = 1;
}
"#,
    );
    let conflict_path = root.join("contracts/generated/user_service.contract.gen.go");
    std::fs::create_dir_all(conflict_path.parent().expect("parent exists"))
        .expect("generated dir created");
    std::fs::write(&conflict_path, "manual edit\n").expect("conflict file written");

    let workflow = GenerateApiWorkflow;
    let result = workflow
        .run(&services, generate_input(&root))
        .await
        .expect("workflow completes with conflicts");

    assert!(result
        .conflicts
        .iter()
        .any(|path| path.ends_with("user_service.contract.gen.go")));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_api_workflow_init_bootstraps_contract_scaffold() {
    let (services, root) = services(ExecService::dry_run());
    let workflow = GenerateApiWorkflow;
    let result = workflow
        .run(
            &services,
            GenerateApiWorkflowInput {
                mode: GenerateApiMode::Init,
                ..generate_input(&root)
            },
        )
        .await
        .expect("init succeeds");

    assert!(root.join("lania.contract.yaml").exists());
    assert!(root.join("schemas/proto/greeter.proto").exists());
    assert_eq!(result.prompts["mode"], "init");
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_api_workflow_check_detects_drift() {
    let (services, root) = services(ExecService::dry_run());
    write_contract_config(
        &root,
        r#"version: 1
entries:
  - name: user-service
    source:
      kind: proto
      inputs:
        - schemas/proto/user.proto
    targets:
      - grpc
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"message User {
  string id = 1;
}
"#,
    );

    let workflow = GenerateApiWorkflow;
    let result = workflow
        .run(
            &services,
            GenerateApiWorkflowInput {
                check: true,
                ..generate_input(&root)
            },
        )
        .await
        .expect("check workflow succeeds");

    assert_eq!(result.state, WorkflowState::Planned);
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("drift detected")));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_api_workflow_clean_removes_stale_outputs() {
    let (services, root) = services(ExecService::dry_run());
    write_contract_config(
        &root,
        r#"version: 1
entries:
  - name: user-service
    source:
      kind: proto
      inputs:
        - schemas/proto/user.proto
    targets:
      - grpc
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"message User {
  string id = 1;
}
"#,
    );

    let workflow = GenerateApiWorkflow;
    workflow
        .run(&services, generate_input(&root))
        .await
        .expect("initial generation succeeds");
    write_contract_config(
        &root,
        r#"version: 1
entries:
  - name: user-service
    source:
      kind: proto
      inputs:
        - schemas/proto/user.proto
    targets:
      - http
"#,
    );

    let result = workflow
        .run(
            &services,
            GenerateApiWorkflowInput {
                clean: true,
                ..generate_input(&root)
            },
        )
        .await
        .expect("clean generation succeeds");

    assert!(!root
        .join("transport/generated/grpc/user_service_grpc.gen.go")
        .exists());
    assert!(result.notes.iter().any(|note| note.contains("clean mode")));
    let _ = std::fs::remove_dir_all(root);
}
