use lania_host::{
    capability::CapabilityContainer,
    plugin::{Plugin, PluginSetupContext},
    registry::{CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl, HandlerRegistry},
    HookBusImpl,
};
use lania_command::CommandSpec;
use lania_workflows::{GenerateApiMode, GenerateModuleMode};
use serde_json::json;

use super::{
    GenerateCommandPlugin, API_DIFF_HANDLER_ID, API_HANDLER_ID, API_INIT_HANDLER_ID,
    API_PLAN_HANDLER_ID, HANDLER_ID, MODULE_APPLY_HANDLER_ID, MODULE_DIFF_HANDLER_ID,
    MODULE_HANDLER_ID, MODULE_INIT_HANDLER_ID, MODULE_PLAN_HANDLER_ID, PRODUCT_HANDLER_ID,
};

#[test]
fn registers_generate_spec() {
    let plugin = GenerateCommandPlugin;
    let mut commands = CommandRegistryImpl::new();
    let mut hooks = HookBusImpl::new();
    let mut capabilities = CapabilityContainer::new();
    let mut handlers = CommandHandlerRegistryImpl::new();

    commands
        .register(CommandSpec::new("product", "product root", "command.product"))
        .expect("product root registered");

    plugin
        .setup(&mut PluginSetupContext {
            commands: &mut commands,
            hooks: &mut hooks,
            capabilities: &mut capabilities,
            handlers: &mut handlers,
        })
        .expect("plugin setup succeeds");

    let spec = commands
        .commands()
        .iter()
        .find(|spec| spec.name == "generate")
        .expect("generate spec registered");
    assert_eq!(spec.name, "generate");
    assert_eq!(spec.handler_id, HANDLER_ID);
    assert_eq!(spec.subcommands.len(), 2);
    assert_eq!(spec.subcommands[0].name, "api");
    assert_eq!(spec.subcommands[0].subcommands.len(), 3);
    assert_eq!(spec.subcommands[1].name, "module");
    assert_eq!(spec.subcommands[1].subcommands.len(), 4);

    let product_root = commands
        .commands()
        .iter()
        .find(|spec| spec.name == "product")
        .expect("product root registered");
    assert!(product_root
        .subcommands
        .iter()
        .any(|spec| spec.name == "generate"));
    assert!(handlers.get(HANDLER_ID).is_some());
    assert!(handlers.get(PRODUCT_HANDLER_ID).is_some());
    assert!(handlers.get(API_HANDLER_ID).is_some());
    assert!(handlers.get(API_PLAN_HANDLER_ID).is_some());
    assert!(handlers.get(API_DIFF_HANDLER_ID).is_some());
    assert!(handlers.get(API_INIT_HANDLER_ID).is_some());
    assert!(handlers.get(MODULE_HANDLER_ID).is_some());
    assert!(handlers.get(MODULE_PLAN_HANDLER_ID).is_some());
    assert!(handlers.get(MODULE_DIFF_HANDLER_ID).is_some());
    assert!(handlers.get(MODULE_INIT_HANDLER_ID).is_some());
    assert!(handlers.get(MODULE_APPLY_HANDLER_ID).is_some());
}

#[test]
fn builds_generate_product_request_from_context() {
    let context = lania_command::CommandContext {
        cwd: "/repo".into(),
        argv: lania_command::ParsedArgv {
            args: Default::default(),
            options: [
                ("preset".into(), json!("demo")),
                ("name".into(), json!("Acme CLI")),
                ("binary-name".into(), json!("acme")),
                ("package-name".into(), json!("@demo/acme")),
                ("output-dir".into(), json!("products/acme-cli")),
                ("force".into(), json!(true)),
            ]
            .into_iter()
            .collect(),
        },
        handler_id: PRODUCT_HANDLER_ID.into(),
        trace_id: "trace-generate-product".into(),
    };

    let bridge = lania_node_bridge::NodeBridgeClient::new(
        lania_node_bridge::BridgeClientConfig::default(),
    );
    let request = GenerateCommandPlugin::build_product_request(&context, &bridge);
    assert_eq!(request.method, "product.generate");
    assert_eq!(request.params["cwd"], "/repo");
    assert_eq!(request.params["preset"], "demo");
    assert_eq!(request.params["name"], "Acme CLI");
    assert_eq!(request.params["binaryName"], "acme");
    assert_eq!(request.params["packageName"], "@demo/acme");
    assert_eq!(request.params["outputDir"], "products/acme-cli");
    assert_eq!(request.params["force"], true);
}

#[test]
fn builds_generate_api_input_from_context() {
    let context = lania_command::CommandContext {
        cwd: "/repo".into(),
        argv: lania_command::ParsedArgv {
            args: Default::default(),
            options: [
                ("config".into(), json!("configs/contracts.yaml")),
                ("source".into(), json!("proto,thrift")),
                ("target".into(), json!("grpc,http")),
                ("entry".into(), json!("user-service,order-service")),
                ("manifest".into(), json!(".lania/custom-lock.json")),
                ("dry-run".into(), json!(true)),
                ("check".into(), json!(true)),
                ("clean".into(), json!(true)),
                ("force".into(), json!(true)),
            ]
            .into_iter()
            .collect(),
        },
        handler_id: API_HANDLER_ID.into(),
        trace_id: "trace-generate".into(),
    };

    let input = GenerateCommandPlugin::build_api_input(&context);
    assert_eq!(input.config_path.as_deref(), Some("configs/contracts.yaml"));
    assert_eq!(
        input.manifest_path.as_deref(),
        Some(".lania/custom-lock.json")
    );
    assert_eq!(input.source_filter, vec!["proto", "thrift"]);
    assert_eq!(input.target_filter, vec!["grpc", "http"]);
    assert_eq!(input.entry_filter, vec!["user-service", "order-service"]);
    assert!(input.dry_run);
    assert!(input.check);
    assert!(input.clean);
    assert!(input.force);
    assert_eq!(input.mode, GenerateApiMode::Apply);
}

#[test]
fn builds_generate_module_input_from_context() {
    let context = lania_command::CommandContext {
        cwd: "/repo".into(),
        argv: lania_command::ParsedArgv {
            args: Default::default(),
            options: [
                ("config".into(), json!("configs/module.yaml")),
                ("input".into(), json!("schemas/proto")),
                ("source".into(), json!("proto")),
                ("target".into(), json!("grpc,http")),
                ("entry".into(), json!("user")),
                ("framework".into(), json!("lania-g")),
                ("main".into(), json!("cmd/app/main.go")),
                ("module-name".into(), json!("UserModule")),
                ("package".into(), json!("app/generated/lania/modules")),
                ("manifest".into(), json!(".lania/module-gen.lock.json")),
                ("dry-run".into(), json!(true)),
                ("check".into(), json!(true)),
                ("clean".into(), json!(true)),
                ("force".into(), json!(true)),
                ("no-inject".into(), json!(true)),
            ]
            .into_iter()
            .collect(),
        },
        handler_id: MODULE_HANDLER_ID.into(),
        trace_id: "trace-generate-module".into(),
    };

    let input = GenerateCommandPlugin::build_module_input(&context);
    assert_eq!(input.config_path.as_deref(), Some("configs/module.yaml"));
    assert_eq!(input.input_path.as_deref(), Some("schemas/proto"));
    assert_eq!(
        input.manifest_path.as_deref(),
        Some(".lania/module-gen.lock.json")
    );
    assert_eq!(input.framework.as_deref(), Some("lania-g"));
    assert_eq!(input.main_path.as_deref(), Some("cmd/app/main.go"));
    assert_eq!(input.module_name.as_deref(), Some("UserModule"));
    assert_eq!(
        input.package_name.as_deref(),
        Some("app/generated/lania/modules")
    );
    assert_eq!(input.source_filter, vec!["proto"]);
    assert_eq!(input.target_filter, vec!["grpc", "http"]);
    assert_eq!(input.entry_filter, vec!["user"]);
    assert!(input.dry_run);
    assert!(input.check);
    assert!(input.clean);
    assert!(input.force);
    assert!(input.no_inject);
    assert_eq!(input.mode, GenerateModuleMode::Apply);
}

#[test]
fn maps_generate_api_subcommand_modes() {
    for (handler_id, mode) in [
        (API_HANDLER_ID, GenerateApiMode::Apply),
        (API_PLAN_HANDLER_ID, GenerateApiMode::Plan),
        (API_DIFF_HANDLER_ID, GenerateApiMode::Diff),
        (API_INIT_HANDLER_ID, GenerateApiMode::Init),
    ] {
        let context = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv::default(),
            handler_id: (*handler_id).to_string(),
            trace_id: "trace".into(),
        };
        let input = GenerateCommandPlugin::build_api_input(&context);
        assert_eq!(input.mode, mode);
    }
}

#[test]
fn maps_generate_module_subcommand_modes() {
    for (handler_id, mode) in [
        (MODULE_HANDLER_ID, GenerateModuleMode::Apply),
        (MODULE_PLAN_HANDLER_ID, GenerateModuleMode::Plan),
        (MODULE_DIFF_HANDLER_ID, GenerateModuleMode::Diff),
        (MODULE_INIT_HANDLER_ID, GenerateModuleMode::Init),
        (MODULE_APPLY_HANDLER_ID, GenerateModuleMode::Apply),
    ] {
        let context = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv::default(),
            handler_id: (*handler_id).to_string(),
            trace_id: "trace".into(),
        };
        let input = GenerateCommandPlugin::build_module_input(&context);
        assert_eq!(input.mode, mode);
    }
}
