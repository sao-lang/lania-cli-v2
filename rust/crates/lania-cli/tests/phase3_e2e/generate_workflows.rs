//! generate api/module 工作流 e2e。
use super::common::*;
use super::*;

#[test]
fn lan_generate_api_init_e2e() {
    let root = temp_dir("generate-init");

    let output = run_cli(&root, &["generate", "api", "init"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert_eq!(json["execution"]["prompts"]["mode"], "init");
    assert!(root.join("lania.contract.yaml").exists());
    assert!(root.join("schemas/proto/greeter.proto").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_api_init_alias_e2e() {
    let root = temp_dir("generate-init-alias");

    let output = run_cli(&root, &["g", "contract", "init"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert_eq!(json["execution"]["prompts"]["mode"], "init");
    assert!(root.join("lania.contract.yaml").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_api_e2e() {
    let root = temp_dir("generate-apply");
    write_contract_fixture(&root, &["grpc", "http"]);

    let output = run_cli(&root, &["generate", "api"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert_eq!(json["execution"]["workflow"], "generate-api");
    assert!(root
        .join("contracts/generated/user_service.contract.gen.go")
        .exists());
    assert!(root
        .join("transport/generated/grpc/user_service_grpc.gen.go")
        .exists());
    assert!(root.join(".lania/contracts.lock.json").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_api_check_and_clean_e2e() {
    let root = temp_dir("generate-check-clean");
    write_contract_fixture(&root, &["grpc"]);

    let check_output = run_cli(&root, &["generate", "api", "--check"], &[]);
    let check_json = parse_stdout_json(&check_output);
    assert_eq!(check_output.status.code(), Some(1));
    assert!(check_json["execution"]["notes"]
        .as_array()
        .expect("notes array")
        .iter()
        .any(|note| note.as_str().unwrap_or_default().contains("drift detected")));

    let apply_output = run_cli(&root, &["generate", "api"], &[]);
    assert_eq!(apply_output.status.code(), Some(0));
    assert!(root
        .join("transport/generated/grpc/user_service_grpc.gen.go")
        .exists());

    write_contract_fixture(&root, &["http"]);
    let clean_output = run_cli(&root, &["generate", "api", "--clean"], &[]);
    let clean_json = parse_stdout_json(&clean_output);
    assert_eq!(clean_output.status.code(), Some(0));
    assert!(!root
        .join("transport/generated/grpc/user_service_grpc.gen.go")
        .exists());
    assert!(clean_json["execution"]["notes"]
        .as_array()
        .expect("notes array")
        .iter()
        .any(|note| note.as_str().unwrap_or_default().contains("clean mode")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_init_e2e() {
    let root = temp_dir("generate-module-init");

    let output = run_cli(&root, &["generate", "module", "init"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert_eq!(json["execution"]["workflow"], "generate-module");
    assert_eq!(json["execution"]["prompts"]["mode"], "init");
    assert!(root.join("lania.module.yaml").exists());
    assert!(root.join("schemas/proto/greeter.proto").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_e2e() {
    let root = temp_dir("generate-module-apply");
    write_module_fixture(&root, &["grpc", "http"], true);

    let output = run_cli(&root, &["generate", "module"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["kind"], "workflow");
    assert_eq!(json["execution"]["workflow"], "generate-module");
    assert!(root
        .join("generated/lania/contracts/user.contract.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/adapters/grpc/user_grpc.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/modules/user_module.gen.go")
        .exists());
    assert!(root.join(".lania/module-gen.lock.json").exists());
    assert!(root.join("zz_lania_module_inject.gen.go").exists());
    assert!(fs::read_to_string(root.join("main.go"))
        .expect("main.go readable")
        .contains("RegisterLaniaGeneratedModules"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_check_and_clean_e2e() {
    let root = temp_dir("generate-module-check-clean");
    write_module_fixture(&root, &["grpc"], false);

    let check_output = run_cli(&root, &["generate", "module", "--check"], &[]);
    let check_json = parse_stdout_json(&check_output);
    assert_eq!(check_output.status.code(), Some(1));
    assert!(check_json["execution"]["notes"]
        .as_array()
        .expect("notes array")
        .iter()
        .any(|note| note.as_str().unwrap_or_default().contains("drift detected")));

    let apply_output = run_cli(&root, &["generate", "module"], &[]);
    assert_eq!(apply_output.status.code(), Some(0));
    assert!(root
        .join("generated/lania/adapters/grpc/user_grpc.gen.go")
        .exists());

    write_module_fixture(&root, &["http"], false);
    let clean_output = run_cli(&root, &["generate", "module", "--clean"], &[]);
    let clean_json = parse_stdout_json(&clean_output);
    assert_eq!(clean_output.status.code(), Some(0));
    assert!(!root
        .join("generated/lania/adapters/grpc/user_grpc.gen.go")
        .exists());
    assert!(clean_json["execution"]["notes"]
        .as_array()
        .expect("notes array")
        .iter()
        .any(|note| note.as_str().unwrap_or_default().contains("clean mode")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_graphql_ws_e2e() {
    let root = temp_dir("generate-module-graphql-ws");
    write_graphql_module_fixture(&root, &["graphql", "ws"]);

    let output = run_cli(&root, &["generate", "module"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["execution"]["workflow"], "generate-module");
    assert!(root
        .join("generated/lania/adapters/graphql/gateway_graphql.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/adapters/ws/gateway_ws.gen.go")
        .exists());
    assert!(fs::read_to_string(
        root.join("generated/lania/adapters/graphql/gateway_graphql.gen.go")
    )
    .expect("graphql adapter readable")
    .contains("query user"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_module_json_grpc_http_e2e() {
    let root = temp_dir("generate-module-json-http-grpc");
    write_json_module_fixture(&root, &["http", "grpc"]);

    let output = run_cli(&root, &["generate", "module"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["execution"]["workflow"], "generate-module");
    assert!(root
        .join("generated/lania/contracts/account.contract.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/adapters/http/account_http.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/adapters/grpc/account_grpc.gen.go")
        .exists());
    assert!(
        fs::read_to_string(root.join("generated/lania/adapters/http/account_http.gen.go"))
            .expect("http adapter readable")
            .contains("GET /accounts/:id")
    );

    let _ = fs::remove_dir_all(root);
}
