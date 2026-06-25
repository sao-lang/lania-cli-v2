# Phase 0 核心抽象冻结

## 目标

Phase 0 只做架构收口，不进入具体业务实现。Rust 成为唯一宿主，Node Bridge 成为 JS 生态能力运行时。

## 宿主抽象

- `Host`
  - 对外暴露 `PluginRegistry`、`HookBus`、`CommandRegistry`、`CapabilityResolver`。
- `HostRuntime`
  - 运行时唯一实例。
  - 负责插件注册、命令元信息装配、能力快照管理以及 Node Bridge 握手预览。

## 插件抽象

- `PluginMeta`
  - 描述插件身份、类型、能力依赖以及顺序约束。
- `Plugin`
  - 只提供 `meta()` 和 `setup()`。
  - `setup()` 只能通过 `PluginSetupContext` 注册命令、hook、capability。
- `PluginSetupContext`
  - 约束插件只做注册，不直接获取宿主内部可变状态。

## 命令抽象

- `CommandSpec`
  - 冻结字段：`name`、`about`、`alias`、`args`、`options`、`examples`、`subcommands`、`handler_id`。
- `CommandContext`
  - 冻结字段：`cwd`、`argv`、`handler_id`、`trace_id`。
  - 业务能力后续统一通过 capability 注入。
- 命令树
  - 支持组合。
  - 支持聚合命令挂载子命令。
  - 支持子命令来自不同插件。

## 能力边界

Phase 0 只冻结名字与边界，不实现完整行为：

- `logger`
- `config`
- `prompt`
- `exec`
- `fs`
- `git`
- `package_manager`
- `task`
- `progress`
- `compiler`
- `lint`
- `template`
- `node_bridge`

边界原则：

- Rust 负责 `Host + Workflow + Presentation + System Runtime`
- Node 负责 `JS Ecosystem Execution Runtime`
- 命令层只通过 capability 访问实现，不直接依赖底层工具

## Hook 模型

保留两类 Hook：

- `Waterfall`
- `Parallel`

冻结的 Hook 名称：

- `runtime_init`
- `plugin_loaded`
- `command_register`
- `command_pre_run`
- `command_post_run`
- `command_error`
- `config_load`
- `config_resolve`
- `template_render`
- `file_write`
- `exec_before_spawn`
- `exec_after_spawn`
- `workflow_start`
- `workflow_complete`

## 生命周期

冻结插件生命周期阶段：

1. `discover`
2. `resolve`
3. `load`
4. `setup`
5. `runtime_start`
6. `command_execute`
7. `shutdown`

## 验收口径

Phase 0 达成的最小状态：

- Rust CLI 可以启动并输出运行时摘要。
- 空插件和 bootstrap 插件可以注册。
- 命令元信息可以被装配并输出。
- Node Bridge 协议和握手数据结构已经落盘。
- 核心抽象文档已经冻结到仓库。
