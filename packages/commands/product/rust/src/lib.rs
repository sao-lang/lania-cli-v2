//! `command-product`：`lan product dev|build|pack|publish|inspect|doctor` 的独立承载子包。
//!
//! product 是与 workspace（项目开发流程）无关的产品化生命周期命令；
//! 原实现分散在 dev/build 包（随两包并入 workspace 后按用户决策拆出为独立子包），
//! `lan product <sub>` 的用法保持不变。

use anyhow::Result;
use lania_host::{
    capability::CapabilityName,
    plugin::{Plugin, PluginKind, PluginMeta, PluginSetupContext},
};

mod build;
mod dev;
mod spec;
mod watch;

use registry::register_product_commands;

pub const HANDLER_ID: &str = "command.product";
pub const COMMAND_NAME: &str = "product";
pub const PRODUCT_DEV_HANDLER_ID: &str = "command.dev.product";
pub const PRODUCT_BUILD_HANDLER_ID: &str = "command.build.product";
pub const PACK_PRODUCT_HANDLER_ID: &str = "command.pack.product";
pub const PUBLISH_PRODUCT_HANDLER_ID: &str = "command.publish.product";
pub const INSPECT_PRODUCT_HANDLER_ID: &str = "command.inspect.product";
pub const DOCTOR_PRODUCT_HANDLER_ID: &str = "command.doctor.product";

#[derive(Debug, Default)]
pub struct ProductCommandPlugin;

pub fn factory() -> Box<dyn lania_host::plugin::Plugin> {
    Box::new(ProductCommandPlugin)
}

/// 冻结导出面：bin 的 clap 依据（product root + 6 子命令完整树）。
pub fn spec() -> lania_command::CommandSpec {
    spec::product_full_spec()
}

impl Plugin for ProductCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-product".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Logger,
                CapabilityName::NodeBridge,
                CapabilityName::Exec,
                CapabilityName::Progress,
                CapabilityName::Task,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_product_commands(ctx)?;
        Ok(())
    }
}

mod registry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_full_spec_carries_six_subcommands() {
        let full = spec();
        assert_eq!(full.name, "product");
        let names: Vec<&str> = full.subcommands.iter().map(|s| s.name.as_str()).collect();
        for expected in ["dev", "build", "pack", "publish", "inspect", "doctor"] {
            assert!(names.contains(&expected), "missing product.{expected}");
        }
    }

    #[test]
    fn handler_ids_are_frozen() {
        assert_eq!(PRODUCT_DEV_HANDLER_ID, "command.dev.product");
        assert_eq!(PRODUCT_BUILD_HANDLER_ID, "command.build.product");
        assert_eq!(PACK_PRODUCT_HANDLER_ID, "command.pack.product");
        assert_eq!(PUBLISH_PRODUCT_HANDLER_ID, "command.publish.product");
        assert_eq!(INSPECT_PRODUCT_HANDLER_ID, "command.inspect.product");
        assert_eq!(DOCTOR_PRODUCT_HANDLER_ID, "command.doctor.product");
    }
}
