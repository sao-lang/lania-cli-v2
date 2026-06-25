//! 命令、处理器与插件注册表的内存实现。
//!
//! 主要导出：new、CommandRegistryImpl、CommandHandlerRegistryImpl、PluginRegistryImpl、CommandRegistry、PluginRegistry。
//! 关键点：
//! - 包含异步/超时/取消等控制流
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};

use lania_command::CommandSpec;

use crate::{CommandHandler, Plugin, PluginMeta};

pub trait CommandRegistry {
    fn register(&mut self, spec: CommandSpec) -> Result<()>;
    fn mount_subcommand(&mut self, parent_name: &str, subcommand: CommandSpec) -> Result<()>;
    fn commands(&self) -> &[CommandSpec];
}

#[derive(Debug, Default)]
pub struct CommandRegistryImpl {
    commands: Vec<CommandSpec>,
}

impl CommandRegistryImpl {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CommandRegistry for CommandRegistryImpl {
    fn register(&mut self, spec: CommandSpec) -> Result<()> {
        if self.commands.iter().any(|item| item.name == spec.name) {
            return Err(anyhow!("duplicate command: {}", spec.name));
        }
        self.commands.push(spec);
        Ok(())
    }

    fn mount_subcommand(&mut self, parent_name: &str, subcommand: CommandSpec) -> Result<()> {
        let parent = self
            .commands
            .iter_mut()
            .find(|item| item.name == parent_name)
            .ok_or_else(|| anyhow!("parent command not found: {}", parent_name))?;

        if parent
            .subcommands
            .iter()
            .any(|item| item.name == subcommand.name)
        {
            return Err(anyhow!(
                "duplicate subcommand {} for parent {}",
                subcommand.name,
                parent_name
            ));
        }

        parent.subcommands.push(subcommand);
        Ok(())
    }

    fn commands(&self) -> &[CommandSpec] {
        &self.commands
    }
}

pub trait PluginRegistry {
    fn register(&mut self, plugin: Box<dyn Plugin>) -> Result<()>;
    fn metas(&self) -> Vec<PluginMeta>;
    fn setup_all(
        &self,
        commands: &mut dyn CommandRegistry,
        hooks: &mut dyn crate::HookRuntime,
        capabilities: &mut dyn crate::CapabilityRegistrar,
        handlers: &mut dyn HandlerRegistry,
    ) -> Result<()>;
}

pub trait HandlerRegistry {
    fn register(&mut self, handler_id: &str, handler: Box<dyn CommandHandler>) -> Result<()>;
    fn get(&self, handler_id: &str) -> Option<&dyn CommandHandler>;
}

#[derive(Default)]
pub struct CommandHandlerRegistryImpl {
    handlers: BTreeMap<String, Box<dyn CommandHandler>>,
}

impl CommandHandlerRegistryImpl {
    pub fn new() -> Self {
        Self::default()
    }
}

impl HandlerRegistry for CommandHandlerRegistryImpl {
    fn register(&mut self, handler_id: &str, handler: Box<dyn CommandHandler>) -> Result<()> {
        if self.handlers.contains_key(handler_id) {
            return Err(anyhow!("duplicate command handler: {handler_id}"));
        }
        self.handlers.insert(handler_id.to_string(), handler);
        Ok(())
    }

    fn get(&self, handler_id: &str) -> Option<&dyn CommandHandler> {
        self.handlers
            .get(handler_id)
            .map(|handler| handler.as_ref())
    }
}

#[derive(Default)]
pub struct PluginRegistryImpl {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistryImpl {
    pub fn new() -> Self {
        Self::default()
    }

    fn ordered_plugin_indices(&self) -> Result<Vec<usize>> {
        // 通过 `before/after` 约束计算插件 setup 顺序。
        //
        // 规则：
        // - `after: [A]` 表示“当前插件必须在 A 之后 setup”，即 A -> current。
        // - `before: [B]` 表示“当前插件必须在 B 之前 setup”，即 current -> B。
        //
        // 实现：构建有向图后执行 Kahn 拓扑排序；如果剩余节点未被消费，则存在环。
        let metas = self.metas();
        let mut name_to_index = BTreeMap::new();
        for (index, meta) in metas.iter().enumerate() {
            name_to_index.insert(meta.name.clone(), index);
        }

        let mut edges = vec![BTreeSet::new(); metas.len()];
        let mut indegree = vec![0usize; metas.len()];

        for (index, meta) in metas.iter().enumerate() {
            for dependency in &meta.after {
                let dependency_index = *name_to_index.get(dependency).ok_or_else(|| {
                    anyhow!(
                        "plugin {} declares unknown dependency in after: {}",
                        meta.name,
                        dependency
                    )
                })?;
                if edges[dependency_index].insert(index) {
                    indegree[index] += 1;
                }
            }

            for dependency in &meta.before {
                let dependency_index = *name_to_index.get(dependency).ok_or_else(|| {
                    anyhow!(
                        "plugin {} declares unknown dependency in before: {}",
                        meta.name,
                        dependency
                    )
                })?;
                if edges[index].insert(dependency_index) {
                    indegree[dependency_index] += 1;
                }
            }
        }

        let mut ready = indegree
            .iter()
            .enumerate()
            .filter_map(|(index, degree)| (*degree == 0).then_some(index))
            .collect::<Vec<_>>();
        // `ready` 可以理解成“当前已经没有前置依赖、随时可以 setup 的插件集合”。
        let mut ordered = Vec::with_capacity(metas.len());

        while !ready.is_empty() {
            // 为了让排序结果在无依赖或多候选时尽量稳定，这里按 index 排序。
            // index 对应注册顺序，能保证输出对“默认注册顺序”是可预测的。
            ready.sort_unstable();
            let next = ready.remove(0);
            ordered.push(next);

            for dependency in edges[next].clone() {
                // “消费掉 next” 的意思是：把 next 指向的边都从图里删掉，
                // 因此这些后继节点的 indegree 要减一。
                indegree[dependency] -= 1;
                if indegree[dependency] == 0 {
                    ready.push(dependency);
                }
            }
        }

        if ordered.len() != metas.len() {
            let remaining = metas
                .iter()
                .enumerate()
                .filter_map(|(index, meta)| (indegree[index] > 0).then_some(meta.name.clone()))
                .collect::<Vec<_>>();
            return Err(anyhow!(
                "plugin dependency cycle detected across: {}",
                remaining.join(", ")
            ));
        }

        Ok(ordered)
    }
}

impl PluginRegistry for PluginRegistryImpl {
    fn register(&mut self, plugin: Box<dyn Plugin>) -> Result<()> {
        let meta = plugin.meta();
        if self
            .plugins
            .iter()
            .any(|registered| registered.meta().name == meta.name)
        {
            return Err(anyhow!("duplicate plugin: {}", meta.name));
        }
        self.plugins.push(plugin);
        Ok(())
    }

    fn metas(&self) -> Vec<PluginMeta> {
        self.plugins.iter().map(|plugin| plugin.meta()).collect()
    }

    fn setup_all(
        &self,
        commands: &mut dyn CommandRegistry,
        hooks: &mut dyn crate::HookRuntime,
        capabilities: &mut dyn crate::CapabilityRegistrar,
        handlers: &mut dyn HandlerRegistry,
    ) -> Result<()> {
        for index in self.ordered_plugin_indices()? {
            let plugin = &self.plugins[index];
            let meta = plugin.meta();
            // 每个插件共享同一组 registry 引用，因此它们实际上是在共同修改
            // 一棵宿主运行时的“全局注册树”。
            let mut ctx = crate::PluginSetupContext {
                commands,
                hooks,
                capabilities,
                handlers,
            };
            plugin.setup(&mut ctx).map_err(|error| {
                anyhow!("plugin {} failed during setup phase: {error}", meta.name)
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use async_trait::async_trait;

    use crate::{
        capability::CapabilityContainer,
        execution::{CommandExecution, CommandExecutionContext, CommandHandler},
        plugin::{Plugin, PluginKind, PluginMeta, PluginSetupContext},
        registry::{
            CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl, HandlerRegistry,
            PluginRegistry, PluginRegistryImpl,
        },
    };
    use lania_hooks::HookBusImpl;

    struct TestPlugin {
        name: &'static str,
    }

    impl Plugin for TestPlugin {
        fn meta(&self) -> PluginMeta {
            PluginMeta {
                name: self.name.into(),
                version: "0.1.0".into(),
                kind: PluginKind::Rust,
                requires: vec![],
                before: vec![],
                after: vec![],
            }
        }

        fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
            ctx.commands.register(lania_command::CommandSpec::new(
                self.name,
                "test command",
                self.name,
            ))?;
            ctx.handlers
                .register(self.name, Box::new(TestHandler(self.name)))?;
            Ok(())
        }
    }

    struct TestHandler(&'static str);

    #[async_trait(?Send)]
    impl CommandHandler for TestHandler {
        async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
            Ok(ctx.complete_workflow(
                lania_workflows::WorkflowExecution {
                    workflow: self.0.into(),
                    state: lania_workflows::WorkflowState::Completed,
                    target_dir: ctx.command().cwd.clone(),
                    prompts: Default::default(),
                    bridge_steps: vec![],
                    written_files: vec![],
                    conflicts: vec![],
                    command_plans: vec![],
                    git_status: None,
                    interactive_rendered: false,
                    notes: vec![],
                },
                0,
            ))
        }
    }

    #[test]
    fn preserves_plugin_registration_order() {
        let mut registry = PluginRegistryImpl::new();
        registry
            .register(Box::new(TestPlugin { name: "first" }))
            .expect("first plugin registers");
        registry
            .register(Box::new(TestPlugin { name: "second" }))
            .expect("second plugin registers");

        let metas = registry.metas();
        assert_eq!(metas[0].name, "first");
        assert_eq!(metas[1].name, "second");
    }

    #[test]
    fn setup_all_runs_plugins_in_registration_order() {
        let mut registry = PluginRegistryImpl::new();
        registry
            .register(Box::new(TestPlugin { name: "first" }))
            .expect("first plugin registers");
        registry
            .register(Box::new(TestPlugin { name: "second" }))
            .expect("second plugin registers");
        let mut commands = CommandRegistryImpl::new();
        let mut hooks = HookBusImpl::new();
        let mut capabilities = CapabilityContainer::new();
        let mut handlers = CommandHandlerRegistryImpl::new();

        registry
            .setup_all(&mut commands, &mut hooks, &mut capabilities, &mut handlers)
            .expect("setup succeeds");

        assert_eq!(commands.commands()[0].name, "first");
        assert_eq!(commands.commands()[1].name, "second");
        assert!(handlers.get("first").is_some());
        assert!(handlers.get("second").is_some());
    }

    #[test]
    fn honors_plugin_dependency_order() {
        struct OrderedPlugin {
            name: &'static str,
            after: Vec<String>,
        }

        impl Plugin for OrderedPlugin {
            fn meta(&self) -> PluginMeta {
                PluginMeta {
                    name: self.name.into(),
                    version: "0.1.0".into(),
                    kind: PluginKind::Rust,
                    requires: vec![],
                    before: vec![],
                    after: self.after.clone(),
                }
            }

            fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
                ctx.commands.register(lania_command::CommandSpec::new(
                    self.name,
                    "ordered command",
                    self.name,
                ))?;
                Ok(())
            }
        }

        let mut registry = PluginRegistryImpl::new();
        registry
            .register(Box::new(OrderedPlugin {
                name: "second",
                after: vec!["first".into()],
            }))
            .expect("second plugin registers");
        registry
            .register(Box::new(OrderedPlugin {
                name: "first",
                after: vec![],
            }))
            .expect("first plugin registers");

        let mut commands = CommandRegistryImpl::new();
        let mut hooks = HookBusImpl::new();
        let mut capabilities = CapabilityContainer::new();
        let mut handlers = CommandHandlerRegistryImpl::new();

        registry
            .setup_all(&mut commands, &mut hooks, &mut capabilities, &mut handlers)
            .expect("dependency order resolves");

        assert_eq!(commands.commands()[0].name, "first");
        assert_eq!(commands.commands()[1].name, "second");
    }
}
