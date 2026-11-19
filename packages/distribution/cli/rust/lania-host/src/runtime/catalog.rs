//! 运行时的发现与清单辅助方法。
//!
//! 这些方法更偏“告诉外部当前有哪些插件/能力”，而不是直接驱动生命周期，
//! 因此单独放在一个子模块里，避免和初始化、执行流程搅在一起。

use crate::{plugin::NodePluginMeta, registry::PluginRegistry};

use super::{config::node_plugin_metas_from_config, HostRuntime};

/// 内建 Node bridge 能力清单。
///
/// 这不是实时探测结果，而是一份静态声明：用于 handshake preview / 调试输出 / 能力发现，
/// 也作为没有项目级插件时的默认 node plugin 视图。
/// 方法名必须与各 bridge slice 的 `plugin.methods` 逐字对应——能力发现对外暴露的方法若
/// 在 TS 侧不存在，调用方只会拿到 `E_METHOD_NOT_FOUND`，因此这里不列任何未被实现的方法。
/// 包名采用拆分后的真实 slice 包名（`@lania-cli-v2/bridge-slice-*`），不再是拆分前的
/// `@lania-cli/node-bridge`。
fn builtin_node_plugin_metas() -> Vec<NodePluginMeta> {
    const PKG: &str = "@lania-cli-v2/bridge-slice-";
    let meta = |name: &str, methods: &[&str]| NodePluginMeta {
        name: name.into(),
        package: format!("{PKG}{name}"),
        methods: methods.iter().map(|method| (*method).into()).collect(),
    };
    vec![
        meta("config", &["config.loadLan", "config.loadTool"]),
        meta(
            "dynamic-commands",
            &["commands.resolveDynamic", "command.invokeDynamic"],
        ),
        // 与 lifecycle slice 的 plugin.methods 对齐：只有 hooks.invoke / hooks.invokeInline。
        // 旧清单里的 hooks.resolveLifecycle 从未在 TS 侧实现，已移除。
        meta("lifecycle", &["hooks.invoke", "hooks.invokeInline"]),
        meta(
            "compiler",
            &["compiler.dev", "compiler.build", "compiler.stop"],
        ),
        meta("lint", &["lint.run"]),
        meta("system", &["system.listCommands"]),
        meta(
            "template",
            &[
                "template.list",
                "template.getQuestions",
                "template.getDependencies",
                "template.getOutputTasks",
                "template.render",
                "addTemplate.render",
                "addGenerator.list",
            ],
        ),
        meta("toolchain", &["toolchain.describe"]),
        meta(
            "product",
            &[
                "product.inspect",
                "product.build",
                "product.pack",
                "product.publish",
            ],
        ),
        meta("commitizen", &["commitizen.run"]),
        meta("commitlint", &["commitlint.run"]),
    ]
}

impl HostRuntime {
    pub fn node_plugin_metas(&self) -> Vec<NodePluginMeta> {
        builtin_node_plugin_metas()
    }

    pub fn discover_builtin_plugins(&self) -> Vec<crate::PluginMeta> {
        self.registries.plugin_registry.metas()
    }

    pub fn discover_project_node_plugins(&self, project_plugins: &[String]) -> Vec<NodePluginMeta> {
        if project_plugins.is_empty() {
            // 没有项目级声明时，回退到内建 node bridge 能力清单。
            return self.node_plugin_metas();
        }

        project_plugins
            .iter()
            .map(|plugin| NodePluginMeta {
                name: plugin.clone(),
                package: plugin.clone(),
                // 这里只知道“项目声明了这个包”，并不知道它具体暴露哪些 method，
                // 所以 methods 先留空，后续由真正的 bridge/load 阶段再补全能力细节。
                methods: vec![],
            })
            .collect()
    }

    pub fn discover_project_node_plugins_from_cwd(
        &self,
        cwd: impl Into<String>,
    ) -> Vec<NodePluginMeta> {
        self.load_lan_config_snapshot_from_cwd(cwd)
            .map(|snapshot| node_plugin_metas_from_config(&snapshot.plugins))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 能力发现清单不得暴露 TS 侧从未实现的方法，否则调用方只会拿到 E_METHOD_NOT_FOUND。
    #[test]
    fn builtin_catalog_has_no_ghost_methods() {
        let metas = builtin_node_plugin_metas();
        let all_methods: Vec<&str> = metas
            .iter()
            .flat_map(|meta| meta.methods.iter().map(String::as_str))
            .collect();
        // hooks.resolveLifecycle 从未在 lifecycle slice 实现（真实方法为 hooks.invoke / invokeInline）。
        assert!(
            !all_methods.contains(&"hooks.resolveLifecycle"),
            "清单不应包含未实现的幽灵方法 hooks.resolveLifecycle"
        );
        assert!(all_methods.contains(&"hooks.invoke"));
        assert!(all_methods.contains(&"hooks.invokeInline"));
    }

    /// 包名必须是拆分后的真实 slice 包名，不再是历史遗留的 @lania-cli/node-bridge。
    #[test]
    fn builtin_catalog_uses_slice_package_names() {
        let metas = builtin_node_plugin_metas();
        for meta in &metas {
            assert!(
                meta.package.starts_with("@lania-cli-v2/bridge-slice-"),
                "{} 的包名 {} 不是拆分后的 slice 包名",
                meta.name,
                meta.package
            );
        }
    }

    /// 宿主自身会调用的能力（product.* / toolchain.describe / addGenerator.list）必须在清单中。
    #[test]
    fn builtin_catalog_covers_host_called_capabilities() {
        let metas = builtin_node_plugin_metas();
        let lifecycle = metas.iter().find(|meta| meta.name == "lifecycle").unwrap();
        assert_eq!(
            lifecycle.methods,
            vec!["hooks.invoke", "hooks.invokeInline"]
        );
        let template = metas.iter().find(|meta| meta.name == "template").unwrap();
        assert!(template.methods.contains(&"addTemplate.render".to_string()));
        assert!(template.methods.contains(&"addGenerator.list".to_string()));
        let product = metas.iter().find(|meta| meta.name == "product").unwrap();
        assert!(product.methods.contains(&"product.publish".to_string()));
        let toolchain = metas.iter().find(|meta| meta.name == "toolchain").unwrap();
        assert!(toolchain
            .methods
            .contains(&"toolchain.describe".to_string()));
    }
}
