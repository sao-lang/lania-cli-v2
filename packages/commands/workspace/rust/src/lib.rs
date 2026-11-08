//! 将 `workspace` 命令注册到宿主：dev/build/lint/create/add 五个原顶级命令
//! 降级为 `lan workspace <sub>` 子命令（product 子树已拆出为独立子包 packages/commands/product）。
//!
//! 拆分说明：
//! - `spec.rs`：workspace root 与各子命令的 spec 定义
//! - `registry.rs`：命令与 handler 注册（注册 + mount 的全部胶水）
//! - `handlers.rs`：workspace root（bare 提示）与 config 统一 handler
//! - `dev.rs`：原 dev 包的 handler 与 bridge request（product dev 运行时已迁至 product 子包）
//! - `build.rs`：原 build 包的 handler 与 bridge request
//! - `lint.rs`：原 lint 包的 handler 与语言 bridge 路由
//! - `create.rs` / `create_template.rs`：原 create 包与 template 子命令
//! - `add.rs`：原 add 包
//! - `config.rs`：workspace 包级偏好（共享 crate `lania-command-config`）
//!
//! 架构铁律：所有 handler_id 保持原值（"command.dev"、"command.build" 等），
//! Rust 侧 localization.rs 的 match 与 e2e 断言都依赖它们。

use anyhow::Result;
use lania_host::{
    capability::CapabilityName,
    plugin::{Plugin, PluginKind, PluginMeta, PluginSetupContext},
};

mod add;
mod build;
mod config;
mod create;
mod create_template;
mod dev;
mod handlers;
mod lint;
mod registry;
mod spec;

use registry::register_workspace_commands;

pub use create_template::{TemplateCommandHandler, TemplateCommandSpec, TEMPLATE_HANDLER_ID};

/// 命令名，必须等于 `spec().name`（§5.4 D.1 冻结导出面 / §10.3 CI 断言）。
pub const COMMAND_NAME: &str = "workspace";

/// workspace 根命令 handler id。
pub const HANDLER_ID: &str = "command.workspace";

// 以下 handler id 全部保持原包冻结值（localization.rs match 与 e2e 依赖）。
pub const DEV_HANDLER_ID: &str = "command.dev";
pub const BUILD_HANDLER_ID: &str = "command.build";
pub const LINT_HANDLER_ID: &str = "command.lint";
pub const LINT_CHECK_HANDLER_ID: &str = "command.lint.check";
pub const LINT_FIX_HANDLER_ID: &str = "command.lint.fix";
pub const CREATE_HANDLER_ID: &str = "command.create";
pub const ADD_HANDLER_ID: &str = "command.add";

#[derive(Debug, Default)]
pub struct WorkspaceCommandPlugin;

/// 命令双形态的唯一契约（§5.4 D.1）：总包 `lan` 与独立二进制 `lan-workspace`
/// 都用它装配同一个插件。目标态返回 `Box<dyn lania_command::CommandPlugin>`；
/// 当前沿用宿主 `Plugin` trait，待类型完全下沉后平移签名。
pub fn factory() -> Box<dyn lania_host::plugin::Plugin> {
    Box::new(WorkspaceCommandPlugin)
}

/// 冻结导出面：`spec()` 作为自由函数，供 `--describe`、独立 bin 与
/// known-commands.json 派生使用；内部委托 `spec::workspace_root_spec()`。
pub fn spec() -> lania_command::CommandSpec {
    // 独立二进制的完整命令树（bin 的 clap 依据）。宿主形态的挂载
    // 由 registry::register_workspace_commands 负责，两者互不影响。
    spec::workspace_full_spec()
}

impl Plugin for WorkspaceCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-workspace".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            // requires 取原 dev/build 包的并集（Logger/NodeBridge/Exec/Progress/Task）
            // 并补齐 create/add 需要的 Prompt/Fs/PackageManager/Git/Template。
            requires: vec![
                CapabilityName::Logger,
                CapabilityName::NodeBridge,
                CapabilityName::Exec,
                CapabilityName::Progress,
                CapabilityName::Task,
                CapabilityName::Prompt,
                CapabilityName::Fs,
                CapabilityName::PackageManager,
                CapabilityName::Git,
                CapabilityName::Template,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_workspace_commands(ctx)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::spec::workspace_root_spec;
    use super::{
        ADD_HANDLER_ID, BUILD_HANDLER_ID, CREATE_HANDLER_ID, DEV_HANDLER_ID, HANDLER_ID,
        LINT_CHECK_HANDLER_ID, LINT_FIX_HANDLER_ID, LINT_HANDLER_ID, TEMPLATE_HANDLER_ID,
        WorkspaceCommandPlugin,
    };
    use lania_host::{
        capability::CapabilityContainer,
        plugin::{Plugin, PluginSetupContext},
        registry::{
            CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl, HandlerRegistry,
        },
        HookBusImpl,
    };

    #[test]
    fn registers_workspace_command_spec() {
        let plugin = WorkspaceCommandPlugin;
        let mut commands = CommandRegistryImpl::new();
        let mut hooks = HookBusImpl::new();
        let mut capabilities = CapabilityContainer::new();
        let mut handlers = CommandHandlerRegistryImpl::new();

        plugin
            .setup(&mut PluginSetupContext {
                commands: &mut commands,
                hooks: &mut hooks,
                capabilities: &mut capabilities,
                handlers: &mut handlers,
            })
            .expect("plugin setup succeeds");

        let specs = CommandRegistry::commands(&commands);

        // workspace root：注册成功，5 个子命令 mount 成功。
        let root = specs
            .iter()
            .find(|spec| spec.name == "workspace")
            .expect("workspace root spec registered");
        assert_eq!(root.handler_id, HANDLER_ID);
        let sub_names: Vec<&str> = root
            .subcommands
            .iter()
            .map(|spec| spec.name.as_str())
            .collect();
        for name in ["dev", "build", "lint", "create", "add", "template", "config"] {
            assert!(sub_names.contains(&name), "missing subcommand {name}");
        }

        // dev 子命令：handler_id 冻结、options 数量不变、不再携带旧包 config。
        let dev = root
            .subcommands
            .iter()
            .find(|spec| spec.name == "dev")
            .expect("dev subcommand mounted");
        assert_eq!(dev.handler_id, DEV_HANDLER_ID);
        assert_eq!(dev.options.len(), 9);
        assert_eq!(dev.subcommands.len(), 0);

        // lint 子命令：check/fix 保留挂在 lint_spec 下。
        let lint = root
            .subcommands
            .iter()
            .find(|spec| spec.name == "lint")
            .expect("lint subcommand mounted");
        assert_eq!(lint.handler_id, LINT_HANDLER_ID);
        assert_eq!(lint.subcommands.len(), 2);
        assert_eq!(lint.subcommands[0].name, "check");
        assert_eq!(lint.subcommands[0].handler_id, LINT_CHECK_HANDLER_ID);
        assert_eq!(lint.subcommands[1].name, "fix");
        assert_eq!(lint.subcommands[1].handler_id, LINT_FIX_HANDLER_ID);

        // config 子树存在（共享 crate 生成的 workspace 偏好树）。
        let config = root
            .subcommands
            .iter()
            .find(|spec| spec.name == "config")
            .expect("config subtree mounted");
        assert_eq!(config.handler_id, "command.workspace.config");
        assert_eq!(config.subcommands.len(), 5);

        // product root + 6 个 product 子命令 mount 成功。
        let product = specs
            .iter()
            .find(|spec| spec.name == "product")
            .expect("product root spec registered");
        assert_eq!(product.subcommands.len(), 6);
        for name in ["dev", "build", "pack", "publish", "inspect", "doctor"] {
            assert!(
                product.subcommands.iter().any(|spec| spec.name == name),
                "missing product subcommand {name}"
            );
        }

        // 全部 handler（含 config 子树 6 个）注册成功。
        let mut expected_ids: Vec<&str> = vec![
            HANDLER_ID,
            DEV_HANDLER_ID,
                    BUILD_HANDLER_ID,
                                LINT_HANDLER_ID,
            LINT_CHECK_HANDLER_ID,
            LINT_FIX_HANDLER_ID,
            CREATE_HANDLER_ID,
            TEMPLATE_HANDLER_ID,
            ADD_HANDLER_ID,
        ];
        for id in lania_command_config::handler_ids(super::COMMAND_NAME) {
            expected_ids.push(Box::leak(id.into_boxed_str()));
        }
        for id in expected_ids {
            assert!(handlers.get(id).is_some(), "missing handler {id}");
        }
    }

    #[test]
    fn workspace_full_spec_carries_every_subcommand() {
        // bin 形态的完整树：dev/build/lint/create（含 template）/add/config。
        let full = crate::spec::workspace_full_spec();
        let names: Vec<&str> = full.subcommands.iter().map(|s| s.name.as_str()).collect();
        for expected in ["dev", "build", "lint", "create", "add", "config"] {
            assert!(names.contains(&expected), "missing subcommand {expected}");
        }
        assert!(!names.contains(&"product"), "product must live in its own subpackage");
        let create = full
            .subcommands
            .iter()
            .find(|s| s.name == "create")
            .expect("create present");
        let create_names: Vec<&str> = create.subcommands.iter().map(|s| s.name.as_str()).collect();
        assert!(create_names.contains(&"template"), "template must be under create");
    }

    #[test]
    fn workspace_root_spec_carries_config_and_examples() {
        let root = workspace_root_spec();
        assert_eq!(root.name, "workspace");
        assert_eq!(root.handler_id, HANDLER_ID);
        assert!(root
            .about
            .contains("Workspace development workflows (dev/build/lint/create/add)"));
        // 示例全部使用 `lan workspace ...` 新拼写。
        assert!(root
            .examples
            .iter()
            .all(|example| example.command.starts_with("lan workspace")));
        assert_eq!(root.subcommands.len(), 1);
        assert_eq!(root.subcommands[0].name, "config");
    }
}

#[cfg(test)]
mod frozen_export_surface {
    //! 冻结导出面守卫（§5.4 D.1 / §10.3 CI 断言）。
    use super::*;

    #[test]
    fn command_name_matches_spec() {
        assert_eq!(spec().name, COMMAND_NAME);
    }

    #[test]
    fn handler_id_is_frozen() {
        assert_eq!(spec().handler_id, "command.workspace");
    }

    #[test]
    fn factory_builds_the_plugin() {
        // 只验证可构造与元信息非空，不触发宿主装配。
        let _plugin = factory();
    }
}
