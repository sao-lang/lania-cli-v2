//! 将 `dev` 命令注册到宿主，并把 CLI 输入映射到对应 workflow 或 bridge 调用。
//!
//! 拆分说明：
//! - `spec.rs`：命令/子命令定义
//! - `request.rs`：标准 `lan dev` 的 bridge request 构建
//! - `watch.rs`：`lan product dev` 的 once/watch 运行时
//! - `handlers.rs`：命令 handler
//! - `registry.rs`：命令与 handler 注册

use anyhow::Result;
use lania_host::{
    capability::CapabilityName,
    plugin::{Plugin, PluginKind, PluginMeta, PluginSetupContext},
};

mod handlers;
mod registry;
mod request;
mod spec;
mod watch;

use registry::register_dev_commands;

pub const HANDLER_ID: &str = "command.dev";
pub const PRODUCT_HANDLER_ID: &str = "command.dev.product";
pub const PRODUCT_ROOT_HANDLER_ID: &str = "command.product";

#[derive(Debug, Default)]
pub struct DevCommandPlugin;

impl Plugin for DevCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-dev".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Logger,
                CapabilityName::NodeBridge,
                CapabilityName::Exec,
                CapabilityName::Progress,
            ],
            before: vec!["command-build".into(), "command-generate".into()],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_dev_commands(ctx)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{watch, DevCommandPlugin, HANDLER_ID, PRODUCT_HANDLER_ID, PRODUCT_ROOT_HANDLER_ID};
    use lania_command::CommandContext;
    use lania_host::registry::HandlerRegistry;
    use lania_host::Plugin;
    use lania_node_bridge::{BridgeClientConfig, NodeBridgeClient};
    use serde_json::json;

    #[test]
    fn registers_dev_command_spec() {
        let plugin = DevCommandPlugin;
        let mut commands = lania_host::registry::CommandRegistryImpl::new();
        let mut hooks = lania_host::HookBusImpl::new();
        let mut capabilities = lania_host::capability::CapabilityContainer::new();
        let mut handlers = lania_host::registry::CommandHandlerRegistryImpl::new();

        plugin
            .setup(&mut lania_host::PluginSetupContext {
                commands: &mut commands,
                hooks: &mut hooks,
                capabilities: &mut capabilities,
                handlers: &mut handlers,
            })
            .expect("plugin setup succeeds");

        let specs = lania_host::CommandRegistry::commands(&commands);
        let dev_spec = specs
            .iter()
            .find(|spec| spec.name == "dev")
            .expect("dev spec registered");
        assert_eq!(dev_spec.handler_id, "command.dev");
        assert_eq!(dev_spec.options.len(), 7);
        assert_eq!(dev_spec.subcommands.len(), 0);
        let product_spec = specs
            .iter()
            .find(|spec| spec.name == "product")
            .expect("product root spec registered");
        assert_eq!(product_spec.handler_id, PRODUCT_ROOT_HANDLER_ID);
        assert_eq!(product_spec.subcommands.len(), 1);
        assert_eq!(product_spec.subcommands[0].name, "dev");
        assert!(handlers.get(HANDLER_ID).is_some());
        assert!(handlers.get(PRODUCT_ROOT_HANDLER_ID).is_some());
        assert!(handlers.get(PRODUCT_HANDLER_ID).is_some());
    }

    #[test]
    fn builds_dev_bridge_request_from_context() {
        let context = CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: Default::default(),
                options: [
                    ("port".into(), json!(4000)),
                    ("path".into(), json!("apps/demo")),
                    ("hmr".into(), json!(false)),
                    ("mode".into(), json!("development")),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: "command.dev".into(),
            trace_id: "trace-1".into(),
        };
        let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
        let request = DevCommandPlugin::build_request(&context, &bridge);

        assert_eq!(request.method, "compiler.dev");
        assert_eq!(request.params["cwd"], "/repo/apps/demo");
        assert_eq!(request.params["port"], 4000);
        assert_eq!(request.params["hmr"], false);
        assert_eq!(request.params["mode"], "development");
    }

    #[test]
    fn resolves_product_dev_options_with_watch_settings() {
        let context = CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv {
                args: [("args".into(), json!(["ops", "hello"]))].into_iter().collect(),
                options: [
                    ("path".into(), json!("products/acme")),
                    ("watch".into(), json!(true)),
                    ("poll-interval-ms".into(), json!(120)),
                ]
                .into_iter()
                .collect(),
            },
            handler_id: PRODUCT_HANDLER_ID.into(),
            trace_id: "trace-product-dev".into(),
        };

        let options = watch::resolve_product_dev_options(&context, "en").expect("options resolve");
        assert_eq!(options.product_root, "/repo/products/acme");
        assert_eq!(options.forwarded_args, vec!["ops", "hello"]);
        assert!(options.watch);
        assert_eq!(options.poll_interval, std::time::Duration::from_millis(120));
    }

    #[test]
    fn skips_generated_directories_when_hashing_watch_paths() {
        let root = std::env::temp_dir().join(format!(
            "lania-dev-watch-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock works")
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("product")).expect("root created");
        std::fs::create_dir_all(root.join(".lania/tmp")).expect("generated dir created");
        std::fs::write(root.join("product/lania.schemas.ts"), "export default {};\n")
            .expect("schema written");
        let before = watch::compute_product_watch_fingerprint(root.to_str().expect("root str"));
        std::fs::write(root.join(".lania/tmp/report.json"), "{\"ok\":true}\n")
            .expect("generated file written");
        let after = watch::compute_product_watch_fingerprint(root.to_str().expect("root str"));
        assert_eq!(before, after);
        let _ = std::fs::remove_dir_all(root);
    }
}
