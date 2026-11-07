//! 运行时的发现与清单辅助方法。
//!
//! 这些方法更偏“告诉外部当前有哪些插件/能力”，而不是直接驱动生命周期，
//! 因此单独放在一个子模块里，避免和初始化、执行流程搅在一起。

use crate::{plugin::NodePluginMeta, registry::PluginRegistry};

use super::{config::node_plugin_metas_from_config, HostRuntime};

impl HostRuntime {
    pub fn node_plugin_metas(&self) -> Vec<NodePluginMeta> {
        // 这份列表描述的是“宿主默认知道 Node bridge 能提供哪些能力”。
        // 它不是实时探测结果，而是一份内建能力声明：
        // - 用于 handshake preview / 调试输出 / 能力发现
        // - 也作为没有项目级插件时的默认 node plugin 视图
        vec![
            NodePluginMeta {
                name: "config".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec!["config.loadLan".into(), "config.loadTool".into()],
            },
            NodePluginMeta {
                name: "dynamic-commands".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec![
                    "commands.resolveDynamic".into(),
                    "command.invokeDynamic".into(),
                ],
            },
            NodePluginMeta {
                name: "lifecycle".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec!["hooks.resolveLifecycle".into(), "hooks.invoke".into()],
            },
            NodePluginMeta {
                name: "compiler".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec![
                    "compiler.dev".into(),
                    "compiler.build".into(),
                    "compiler.stop".into(),
                ],
            },
            NodePluginMeta {
                name: "lint".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec!["lint.run".into()],
            },
            NodePluginMeta {
                name: "system".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec!["system.listCommands".into()],
            },
            NodePluginMeta {
                name: "template".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec![
                    "template.list".into(),
                    "template.getQuestions".into(),
                    "template.getDependencies".into(),
                    "template.getOutputTasks".into(),
                    "template.render".into(),
                ],
            },
            NodePluginMeta {
                name: "commitizen".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec!["commitizen.run".into()],
            },
            NodePluginMeta {
                name: "commitlint".into(),
                package: "@lania-cli/node-bridge".into(),
                methods: vec!["commitlint.run".into()],
            },
        ]
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
