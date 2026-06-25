//! 将 `build/pack/publish/inspect/doctor` 命令注册到宿主，并把 CLI 输入映射到 bridge 调用。
//!
//! 拆分说明：
//! - `spec.rs`：命令/子命令/选项定义
//! - `request.rs`：CLI -> bridge request 映射
//! - `registry.rs`：命令与 handler 注册
//! - `handlers.rs`：真正执行命令的 handler

use anyhow::Result;
use lania_host::{
    capability::CapabilityName,
    plugin::{Plugin, PluginKind, PluginMeta, PluginSetupContext},
};

mod handlers;
mod registry;
mod request;
mod spec;

use registry::{
    register_build_commands,
};

pub const HANDLER_ID: &str = "command.build";
pub const PRODUCT_HANDLER_ID: &str = "command.build.product";
pub const PACK_PRODUCT_HANDLER_ID: &str = "command.pack.product";
pub const PUBLISH_PRODUCT_HANDLER_ID: &str = "command.publish.product";
pub const INSPECT_PRODUCT_HANDLER_ID: &str = "command.inspect.product";
pub const DOCTOR_PRODUCT_HANDLER_ID: &str = "command.doctor.product";

#[derive(Debug, Default)]
pub struct BuildCommandPlugin;

impl Plugin for BuildCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-build".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Logger,
                CapabilityName::NodeBridge,
                CapabilityName::Task,
                CapabilityName::Progress,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_build_commands(ctx)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lania_host::{
        capability::CapabilityContainer,
        plugin::{Plugin, PluginSetupContext},
        registry::{
            CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl, HandlerRegistry,
        },
        HookBusImpl,
    };

    use super::{
        BuildCommandPlugin, DOCTOR_PRODUCT_HANDLER_ID, HANDLER_ID, INSPECT_PRODUCT_HANDLER_ID,
        PACK_PRODUCT_HANDLER_ID, PRODUCT_HANDLER_ID, PUBLISH_PRODUCT_HANDLER_ID,
    };
    use lania_command::{CommandContext, CommandSpec};
    use lania_node_bridge::{BridgeClientConfig, NodeBridgeClient};
    use serde_json::json;

    #[test]
    fn registers_build_pack_publish_and_inspect_command_specs() {
        let plugin = BuildCommandPlugin;
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

        let build_spec = commands
            .commands()
            .iter()
            .find(|spec| spec.name == "build")
            .expect("build spec registered");
        assert_eq!(build_spec.name, "build");
        assert_eq!(build_spec.handler_id, "command.build");
        assert_eq!(build_spec.options.len(), 5);
        assert_eq!(build_spec.subcommands.len(), 0);
        let product_root_spec = commands
            .commands()
            .iter()
            .find(|spec| spec.name == "product")
            .expect("product root spec present");
        assert!(product_root_spec
            .subcommands
            .iter()
            .any(|spec| spec.name == "build"));
        assert!(product_root_spec
            .subcommands
            .iter()
            .any(|spec| spec.name == "pack"));
        assert!(product_root_spec
            .subcommands
            .iter()
            .any(|spec| spec.name == "publish"));
        assert!(product_root_spec
            .subcommands
            .iter()
            .any(|spec| spec.name == "inspect"));
        assert!(product_root_spec
            .subcommands
            .iter()
            .any(|spec| spec.name == "doctor"));
        assert!(handlers.get(HANDLER_ID).is_some());
        assert!(handlers.get(PRODUCT_HANDLER_ID).is_some());
        assert!(handlers.get(PACK_PRODUCT_HANDLER_ID).is_some());
        assert!(handlers.get(PUBLISH_PRODUCT_HANDLER_ID).is_some());
        assert!(handlers.get(INSPECT_PRODUCT_HANDLER_ID).is_some());
        assert!(handlers.get(DOCTOR_PRODUCT_HANDLER_ID).is_some());
    }

    #[test]
    fn builds_build_bridge_request_from_context() {
        let context = CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("watch".into(), json!(true)),
                    ("path".into(), json!("apps/demo")),
                    ("mode".into(), json!("development")),
                    ("output-dir".into(), json!("dist-custom")),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.build".into(),
            trace_id: "trace-2".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = BuildCommandPlugin::build_request(&context, &bridge);

        assert_eq!(request.method, "compiler.build");
        assert_eq!(request.params["cwd"], "/repo/apps/demo");
        assert_eq!(request.params["watch"], true);
        assert_eq!(request.params["mode"], "development");
        assert_eq!(request.params["outputDir"], "dist-custom");
    }

    #[test]
    fn builds_product_build_bridge_request_from_context() {
        let context = CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("path".into(), json!("products/acme")),
                    ("output-dir".into(), json!(".lania/build/product")),
                    ("clean".into(), json!(false)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: PRODUCT_HANDLER_ID.into(),
            trace_id: "trace-3".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = BuildCommandPlugin::build_product_request(&context, &bridge);

        assert_eq!(request.method, "product.build");
        assert_eq!(request.params["cwd"], "/repo/products/acme");
        assert_eq!(request.params["outputDir"], ".lania/build/product");
        assert_eq!(request.params["clean"], false);
    }

    #[test]
    fn builds_product_pack_bridge_request_from_context() {
        let context = CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("path".into(), json!("products/acme")),
                    ("build-dir".into(), json!(".lania/build/product")),
                    (
                        "output-dir".into(),
                        json!(".lania/pack/product/install-root"),
                    ),
                    ("clean".into(), json!(false)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: PACK_PRODUCT_HANDLER_ID.into(),
            trace_id: "trace-4".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = BuildCommandPlugin::pack_product_request(&context, &bridge);

        assert_eq!(request.method, "product.pack");
        assert_eq!(request.params["cwd"], "/repo/products/acme");
        assert_eq!(request.params["buildDir"], ".lania/build/product");
        assert_eq!(
            request.params["outputDir"],
            ".lania/pack/product/install-root"
        );
        assert_eq!(request.params["clean"], false);
    }

    #[test]
    fn builds_product_publish_bridge_request_from_context() {
        let context = CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("path".into(), json!("products/acme")),
                    ("pack-dir".into(), json!(".lania/pack/product/install-root")),
                    (
                        "output-dir".into(),
                        json!(".lania/publish/product/npm-package"),
                    ),
                    ("dist-tag".into(), json!("next")),
                    ("channel".into(), json!("beta")),
                    ("registry".into(), json!("http://localhost:4873")),
                    (
                        "platform-binaries-dir".into(),
                        json!("/tmp/lania-cli-platforms"),
                    ),
                    (
                        "platform-binary-paths".into(),
                        json!(r#"{"linux-x64":"/tmp/lania-cli-linux-x64"}"#),
                    ),
                    ("execute".into(), json!(true)),
                    ("dry-run".into(), json!(true)),
                    ("yes".into(), json!(true)),
                    ("resume".into(), json!(true)),
                    ("otp".into(), json!("123456")),
                    ("npm-bin".into(), json!("/tmp/fake-npm")),
                    ("max-retries".into(), json!("3")),
                    ("retry-delay-ms".into(), json!("1500")),
                    ("rollback-on-failure".into(), json!(true)),
                    ("clean".into(), json!(false)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: PUBLISH_PRODUCT_HANDLER_ID.into(),
            trace_id: "trace-5".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = BuildCommandPlugin::publish_product_request(&context, &bridge);

        assert_eq!(request.method, "product.publish");
        assert_eq!(request.params["cwd"], "/repo/products/acme");
        assert_eq!(
            request.params["packDir"],
            ".lania/pack/product/install-root"
        );
        assert_eq!(
            request.params["outputDir"],
            ".lania/publish/product/npm-package"
        );
        assert_eq!(request.params["distTag"], "next");
        assert_eq!(request.params["channel"], "beta");
        assert_eq!(request.params["registry"], "http://localhost:4873");
        assert_eq!(
            request.params["platformBinariesDir"],
            "/tmp/lania-cli-platforms"
        );
        assert_eq!(
            request.params["platformBinaryPaths"],
            r#"{"linux-x64":"/tmp/lania-cli-linux-x64"}"#
        );
        assert_eq!(request.params["execute"], true);
        assert_eq!(request.params["dryRun"], true);
        assert_eq!(request.params["yes"], true);
        assert_eq!(request.params["resume"], true);
        assert_eq!(request.params["otp"], "123456");
        assert_eq!(request.params["npmBin"], "/tmp/fake-npm");
        assert_eq!(request.params["maxRetries"], "3");
        assert_eq!(request.params["retryDelayMs"], "1500");
        assert_eq!(request.params["rollbackOnFailure"], true);
        assert_eq!(request.params["clean"], false);
    }

    #[test]
    fn builds_product_inspect_bridge_request_from_context() {
        let context = CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("path".into(), json!("products/acme")), ("compat".into(), json!(true))]
                    .into_iter()
                    .collect(),
            },
            handler_id: INSPECT_PRODUCT_HANDLER_ID.into(),
            trace_id: "trace-6".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = BuildCommandPlugin::inspect_product_request(&context, &bridge);

        assert_eq!(request.method, "product.inspect");
        assert_eq!(request.params["cwd"], "/repo/products/acme");
        assert_eq!(request.params["clean"], true);
        assert_eq!(request.params["compat"], true);
        assert_eq!(request.params["hostVersion"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn builds_product_doctor_bridge_request_from_context() {
        let context = CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [("path".into(), json!("products/acme"))].into_iter().collect(),
            },
            handler_id: DOCTOR_PRODUCT_HANDLER_ID.into(),
            trace_id: "trace-7".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = BuildCommandPlugin::doctor_product_request(&context, &bridge);

        assert_eq!(request.method, "product.inspect");
        assert_eq!(request.params["cwd"], "/repo/products/acme");
        assert_eq!(request.params["clean"], true);
        assert_eq!(request.params["doctor"], true);
        assert_eq!(request.params["compat"], true);
        assert_eq!(request.params["hostVersion"], env!("CARGO_PKG_VERSION"));
    }
}
