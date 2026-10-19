# 命令模型与加载原理

<div class="lead">
Lania CLI 的命令不是写死在 clap 里的，而是由 Rust 插件、Node Bridge 插件和项目 manifest 共同装配出来的。理解命令如何被注册、解析和分发，是排查“命令为什么不存在/为什么走错链路”的关键。
</div>

## 命令的六种来源

| 来源           | 注册方                                   | 执行方                              | 典型命令                                                 |
| -------------- | ---------------------------------------- | ----------------------------------- | -------------------------------------------------------- |
| 内置 Rust 命令 | Rust 插件 `setup()`                      | Rust handler                        | `create`、`add`、`generate`、`release`、`sync`、`config` |
| Bridge 命令    | Rust 插件注册 spec                       | Node Bridge 插件                    | `dev`、`build`、`lint`                                   |
| 混合命令       | Rust 插件                                | 本地 + Bridge                       | `tools`（`list` 走 Bridge，`run/view` 走本地）           |
| 查询命令       | Rust 插件                                | Node Bridge 查询                    | `template`                                               |
| 运行时命令     | Node Bridge 解析 manifest 后由 Rust 注册 | Node Bridge `command.invokeDynamic` | 项目自定义命令                                           |
| 产品命令       | Rust 插件 / Node Bridge                  | Bridge                              | `product generate/dev/build/pack/publish/inspect/doctor` |

## 启动与注册顺序

`rust/crates/lania-cli/src/main.rs` 是总装配入口：

```text
HostRuntime::new()
  → register_plugin(EmptyPlugin)
  → register_plugin(DevCommandPlugin)
  → register_plugin(BuildCommandPlugin)
  → register_plugin(LintCommandPlugin)
  → register_plugin(CreateCommandPlugin)
  → register_plugin(AddCommandPlugin)
  → register_plugin(GenerateCommandPlugin)
  → register_plugin(ReleaseCommandPlugin)
  → register_plugin(SyncCommandPlugin)
  → register_plugin(TemplateCommandPlugin)
  → register_plugin(ToolsCommandPlugin)
  → register_plugin(ConfigCommandPlugin)
  → host.initialize()
  → 读取 lan.config.* 快照
  → 读取全局偏好
  → 计算 locale / output profile
  → bootstrap_project_extensions()
      → 加载项目插件
      → commands.resolveDynamic
      → 注册动态命令 spec 与 handler
  → 构建 clap 命令树
  → 解析 argv
  → host.execute_command(context)
```

注册顺序会影响：

- `--help` 中命令的展示顺序
- 插件 Hook / Capability 的装配顺序
- 动态命令覆盖或冲突时的可见性

## CommandSpec：命令的声明

Rust 侧用 `CommandSpec` 描述命令，不直接绑定执行代码：

```rust
CommandSpec {
    name: "build",
    about: "Run the project build workflow",
    alias: Some("b"),
    aliases: vec![],
    args: vec![],
    options: vec![...],
    examples: vec![...],
    subcommands: vec![],
    handler_id: "command.build",
}
```

字段含义：

| 字段          | 作用                               |
| ------------- | ---------------------------------- |
| `name`        | 命令名                             |
| `about`       | 帮助描述                           |
| `alias`       | 单个可见别名                       |
| `aliases`     | 多个可见别名                       |
| `args`        | 位置参数                           |
| `options`     | 命名选项                           |
| `examples`    | 帮助中的示例                       |
| `subcommands` | 子命令树                           |
| `handler_id`  | 执行时去 handler registry 查找的键 |

`CommandSpec` 会被 `lania-command` 转换成 clap 命令树。

## 参数模型

### ArgSpec

```rust
ArgSpec {
    name: "path",
    required: false,
    multiple: false,
    help: "Project path",
}
```

### OptionSpec

```rust
OptionSpec {
    long: "output-dir",
    short: Some('o'),
    help: "Output directory",
    value_kind: ValueKind::String,
    default_value: None,
    choices: vec![],
    negatable: false,
}
```

`ValueKind` 只有四种：

- `Bool`
- `String`
- `Number`
- `OptionalString`

更复杂的结构由 workflow / handler 自己解释。

## 从 argv 到 CommandContext

```text
clap matches
  → command_context_from_matches()
  → CommandContext {
      cwd,
      argv: { args: BTreeMap, options: BTreeMap },
      handler_id,
      trace_id,
    }
```

`CommandContext` 是 handler 唯一依赖的输入快照。动态命令、hooks、bridge request 都从这个结构继续派生。

## handler 分发

```text
HostRuntime::execute_command(context)
  → onCommandPreInit（waterfall）
  → onArgsParsed（waterfall）
  → 将 hook 返回的新 argv 写回 context
  → 按 context.handler_id 查找 CommandHandler
  → handler.execute(execution_context)
  → onSuccess / onError（parallel）
  → 附加 host_state
```

### 为什么 `onCommandPreInit` / `onArgsParsed` 是 waterfall

它们允许插件改写 argv，例如：

- 补默认值
- 展开旧参数别名
- 兼容旧版本参数名

返回值会**整体覆盖**当前 `args/options`，不是增量 merge。

## 别名

内置命令的别名来自两层：

1. `CommandSpec.alias` / `aliases`
2. `apply_legacy_aliases()` 中的历史别名表

当前历史别名：

| 命令           | 别名       |
| -------------- | ---------- |
| `dev`          | `d`        |
| `build`        | `b`        |
| `lint`         | `l`        |
| `create`       | `c`        |
| `add`          | `a`        |
| `generate`     | `g`        |
| `generate api` | `contract` |
| `release`      | `r`        |
| `sync`         | `s`        |
| `template`     | `t`        |

> `lan.config.commands.aliases` 与 `commands.shortcuts` 目前会被解析并校验，但没有接入实际命令解析流程。不要依赖它们改变 CLI 行为。

## 每类命令的真实执行路径

### Workflow 型

```text
Rust handler
  → lania-workflows
  → capability / service
  → Bridge 或本地实现
```

例如 `create`：

```text
CreateCommandPlugin
  → CreateWorkflow::run()
  → TemplateCapability
  → template.getQuestions / getDependencies / render
  → Fs / Format / Exec / Git
```

### Bridge 型

```text
Rust handler
  → NodeBridgeClient.request(method, params)
  → Node Bridge plugin
  → Node 侧工具链
  → events + response
  → Rust 统一渲染
```

例如 `build`：

```text
BuildCommandPlugin
  → compiler.build
  → compilerPlugin
  → Vite / Webpack / Rollup adapter
```

### 混合型 `tools`

| 子命令       | 执行方                            |
| ------------ | --------------------------------- |
| `tools list` | Node Bridge `system.listCommands` |
| `tools run`  | Rust `lania-exec` + 运行时解析    |
| `tools view` | Rust 本地文件能力                 |

### 动态命令

```text
启动阶段：
  commands.resolveDynamic
    → Node 扫描 manifest
    → 返回命令 spec + handler 注册表
    → Rust 注册 spec / BridgeCommandHandler / target hooks

执行阶段：
  BridgeCommandHandler
    → maybe_prompt_dynamic_command
    → command.invokeDynamic
    → Node 执行 local function / plugin method
```

详细开发方式见 [开发运行时命令](/guide/runtime-commands)。

## 产品命令的实际形态

当前命令树是：

```text
lan product
  generate
  dev
  build
  pack
  publish
  inspect
  doctor
```

请使用：

```bash
lan product build
lan product pack
lan product publish
```

`lan build product`、`lan pack product`、`lan publish product` 在当前实现中**不是合法命令**；如果看到旧文档或旧脚本这样写，需要改成上面的形式。

## 命令查找失败时怎么排查

1. `lan --help` 是否列出该命令。
2. 是否是动态命令：`extensions.dynamicCommands` 是否开启。
3. manifest 是否被 `schemaDiscovery` 扫到。
4. mount 是否与保留命令冲突。
5. handler 是否注册成功：启动日志中的 `commands.resolveDynamic` 与 warnings。
6. 是否被插件策略拒绝：查看 `bridge.metrics` 的 `rejectedPlugins`。

## 下一步

- [Rust ↔ Node 通信协议](/architecture/bridge-protocol)
- [开发运行时命令](/guide/runtime-commands)
- [ctx.tools 宿主能力](/guide/host-tools)
