# 开发运行时命令

<div class="lead">
运行时命令（动态命令）让项目在不修改 Rust 宿主的情况下，把自定义命令挂到同一棵 CLI 命令树上。本页从零开始，覆盖 manifest、prompt、hooks、workflow、scaffold、ctx.tools 与项目插件。
</div>

## 1. 启用

`lan.config.ts`：

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
};
```

默认扫描：

- `lania.schemas.ts`
- `lania.schemas.js`
- `lania.schemas.cjs`
- `.lania/schemas/*.{ts,js,cjs,json,yaml,yml}`

自定义：

```ts
export default {
  extensions: { dynamicCommands: true },
  schemaDiscovery: {
    files: ['lania.schemas.ts', 'lania.schemas.js'],
    dirs: ['.lania/schemas'],
    allowExtensions: ['.ts', '.js', '.cjs', '.json', '.yaml', '.yml'],
  },
  schema: {
    entry: './product/lania.schemas.ts',
  },
};
```

`schema.entry` 与 `product.schemaEntry` 也会作为显式入口参与发现。

## 2. 最小命令

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      command: {
        about: 'Ops tools',
        alias: 'o',
      },
      commands: [
        {
          name: 'ping',
          about: 'Ping endpoint',
          options: [
            {
              long: 'endpoint',
              valueKind: 'string',
              help: 'Endpoint',
              required: true,
            },
          ],
          handler: async (ctx) => ({
            result: {
              ok: true,
              endpoint: ctx.argv.options.endpoint,
            },
            events: [],
          }),
        },
      ],
    },
  ],
};
```

执行：

```bash
lan ops ping --endpoint https://example.com
lan o ping --endpoint https://example.com
```

## 3. handler 的 ctx

```ts
interface DynamicCommandContext {
  cwd: string;
  mount: string;
  path: string[];
  argv: { args: Record<string, unknown>; options: Record<string, unknown> };
  traceId: string | null;
  tools: SchemaTools;
  scaffold: ScaffoldPlan;
  product: ProductContext;
  runtime: RuntimeContext;
}
```

| 字段                         | 说明                                                                                                    |
| ---------------------------- | ------------------------------------------------------------------------------------------------------- |
| `cwd`                        | 当前命令执行目录                                                                                        |
| `mount` / `path`             | 动态命令挂载点与子命令路径                                                                              |
| `argv.args` / `argv.options` | Rust 解析后的位置参数与选项                                                                             |
| `traceId`                    | 本次执行 trace                                                                                          |
| `tools`                      | 宿主能力对象，见 [ctx.tools 宿主能力](/guide/host-tools)                                                |
| `scaffold`                   | 当前 scaffold plan：模板层、依赖、merge rules、guards、post actions                                     |
| `product`                    | 产品上下文：`name`、`binaryName`、`displayName`、`version`、`productRoot`、`schemaRoot`、`templatesDir` |
| `runtime`                    | 运行时上下文：`mode`、`traceId`、`invocationCwd`、`workspaceRoot`、`productRoot`、`schemaRoot`          |

handler 返回：

```ts
{
  result: unknown
  events?: BridgeEvent[]
  exitCode?: number
}
```

`result` 会进入 `command.invokeDynamic` 的 `result`；`events` 会进入本次命令的事件流。

## 4. 加载与注册发生了什么

```text
lan 启动
  → host.bootstrap_project_extensions_from_cwd_async()
  → commands.resolveDynamic
  → Node 扫描 manifest
  → 解析 runtimeCommands[].commands
  → 生成 GeneratedCommandSpec
  → 生成 handlerId = dynamic.manifest.<mount>.<path>.<seq>
  → 返回 commands + handlers + warnings
  → Rust 注册 CommandSpec
  → Rust 注册 BridgeCommandHandler(method=command.invokeDynamic, target=...)
  → 注册 target hooks
```

执行：

```text
lan ops ping --endpoint ...
  → Rust clap 解析
  → CommandContext
  → BridgeCommandHandler
  → maybe_prompt_dynamic_command
  → command.invokeDynamic
  → Node 取 local handler / plugin handler
  → 合并 events
  → 返回 result + exitCode
```

## 5. 参数与选项

### args

```ts
args: [
  {
    name: 'service',
    required: true,
    multiple: false,
    help: 'service name',
  },
];
```

### options

```ts
options: [
  {
    long: 'env',
    short: 'e',
    valueKind: 'string',
    help: 'environment',
    required: true,
    defaultValue: 'staging',
    choices: ['staging', 'prod'],
    negatable: false,
  },
  {
    long: 'confirm',
    valueKind: 'bool',
    help: 'confirm deploy',
  },
];
```

`valueKind`：

- `bool`
- `string`
- `number`
- `optional_string`

### 必填选项与补问

`required: true` 的选项会被拆到 `requiredOptions`。如果用户没传，宿主会自动补问：

```text
缺少必填参数 --env，请输入：
```

也可以用 `prompt` 精确定义补问。

## 6. prompt

```ts
prompt: [
  {
    id: 'mode',
    field: 'mode',
    message: { zh: '请选择部署模式', en: 'Select deploy mode' },
    kind: 'select',
    choices: [
      { label: 'simple', value: 'simple' },
      { label: 'advanced', value: 'advanced' },
    ],
    whenMissing: ['mode'],
    defaultValue: 'simple',
  },
  {
    field: 'region',
    message: 'Region?',
    kind: 'input',
    when: { type: 'equals', key: 'mode', value: 'advanced' },
  },
];
```

支持的 `kind`：

`input`、`select`、`confirm`、`multi_select`、`password`、`editor`、`number`、`fuzzy_select`、`autocomplete`、`search`、`rawlist`、`expand`。

### 非交互 / CI

- `ui.interaction.mode = non_interactive`：绝不弹问题。
- `ui.interaction.mode = auto` + 非 TTY + `defaultStrategy=fail`：不弹问题。
- `ui.interaction.mode = auto` + 非 TTY + `defaultStrategy=use_defaults`：使用默认值。
- 脚本化答案：

```powershell
$env:LANIA_PROMPT_ANSWERS_JSON = '{"env":"staging","confirm":true}'
```

完整字段见 [运行时清单参考](/reference/manifest)。

## 7. hooks

```ts
hooks: {
  onArgsParsed: [
    async (payload, ctx) => ({
      ...payload,
      events: [
        ...(payload?.events ?? []),
        {
          method: 'event.log',
          params: { level: 'info', message: `args parsed in ${ctx.cwd}` }
        }
      ]
    })
  ],
  onSuccess: [
    {
      plugin: './lania.plugin.ts',
      handler: 'hooks.audit',
      kind: 'parallel',
      timeoutMs: 5000,
      onError: 'collect'
    }
  ]
}
```

### inline hook

inline hook 的签名是：

```ts
(payload, ctx) => unknown | { payload, events };
```

`ctx` 包含 `cwd`、`hook`、`kind`、`source` 和 `tools`。如果返回值是对象且包含 `payload` 字段，会被当作 `{ payload, events }` envelope；否则返回值整体作为新的 payload。

函数型 hook 不会被序列化给 Rust。Node 会：

1. 把函数注册到 `inline-hooks` registry
2. 把声明替换成 `{ type: 'inline', id }`
3. Rust 执行时通过 `hooks.invokeInline` 回调

### plugin hook

```ts
{
  type: 'plugin',
  plugin: './lania.plugin.ts',
  handler: 'hooks.audit',
  kind: 'waterfall',
  timeoutMs: 5000,
  onError: 'collect'
}
```

插件必须先在 `lan.config.plugins` 或 manifest `plugins` 中声明。

### Hook 名称

支持的 20 个 key：

`onInitialize`、`onCommandPreInit`、`onArgsParsed`、`onFilesPrepare`、`onConfigGet`、`onConfigResolve`、`onFileWrite`、`onTemplateParse`、`onDependenciesModify`、`onDependenciesInstall`、`onInteractionPrompt`、`onShellCommand`、`onPluginApiCall`、`onSuccess`、`onError`、`onPluginLoaded`、`onCommandRegister`、`onWorkflowStart`、`onWorkflowComplete`、`onShutdown`。

`preRun` / `postRun` 是旧写法，会分别映射到 `onCommandPreInit` / `onSuccess`。

## 8. workflow

命令可以直接引用命名 workflow：

```ts
workflows: {
  deploy: {
    steps: [
      'preflight',
      'resolvePreset',
      'resolveFeatures',
      'renderTemplates',
      'mergeFiles',
      'writeFiles',
      'installDependencies',
      'gitInit',
      'postActions',
      'printSummary',
    ];
  }
}
```

命令：

```ts
{
  name: 'deploy',
  workflow: 'deploy'
}
```

### 支持的 step

| step                  | 作用                |
| --------------------- | ------------------- |
| `preflight`           | 执行 guards         |
| `resolvePreset`       | 解析 preset         |
| `resolveFeatures`     | 解析 features       |
| `renderTemplates`     | 渲染模板层          |
| `mergeFiles`          | 按 merge rules 合并 |
| `writeFiles`          | 写入文件并注册回滚  |
| `installDependencies` | 安装依赖            |
| `gitInit`             | 初始化 Git          |
| `postActions`         | 执行 post actions   |
| `printSummary`        | 输出摘要            |

失败时会执行 transaction rollback。

## 9. scaffold 目录

命令可以声明：

```ts
{
  name: 'create-app',
  preset: 'app',
  features: ['auth'],
  dependencyRecipes: ['base'],
  mergeRules: ['package-json'],
  guards: ['empty-dir'],
  postActions: ['install', 'git']
}
```

manifest 顶层可以定义：

```ts
presets: {},
features: {},
dependencyRecipes: {},
mergeRules: {},
guards: {},
resolvers: {},
postActions: {}
```

条件表达式支持：

```ts
when: { field: 'options.mode', equals: 'advanced' }
when: { field: 'features', in: ['auth'] }
when: { all: [...] }
when: { any: [...] }
when: { field: 'options.force', truthy: true }
```

## 10. 项目插件

### 插件模块

```ts
// lania.plugin.ts
export default {
  name: 'demo-plugin',
  methods: ['commands.deploy', 'hooks.audit'],

  async handle(method, params, context) {
    if (method === 'commands.deploy') {
      return {
        result: { ok: true, params },
        events: [],
      };
    }

    if (method === 'hooks.invoke') {
      return {
        result: {
          payload: params.payload,
        },
        events: [],
      };
    }

    return null;
  },
};
```

### 声明

```ts
export default {
  plugins: [
    {
      package: './lania.plugin.ts',
      methods: ['commands.deploy', 'hooks.audit'],
    },
  ],
};
```

### 绑定 handler

```ts
{
  name: 'deploy',
  handler: {
    plugin: './lania.plugin.ts',
    method: 'commands.deploy'
  }
}
```

### 安全边界

本地插件：

- 必须是相对路径
- 必须位于 workspace root 内
- 扩展名必须是 `.js` / `.cjs` / `.mjs` / `.ts`

包插件：

- 必须命中 `@lania/plugin-` 或 `lania-plugin-` 前缀，或在 `pluginAllowlist` 中
- 可选 `pluginRequireSignature` + `pluginSignatureAllowlist`
- 最终方法集合 = 插件导出方法 ∩ 声明方法 ∩ `pluginMethodAllowlist`

## 11. 新增 Rust 内置命令

如果命令需要进入 Rust 宿主，而不是项目运行时：

### 1. 新建插件 crate

```text
rust/crates/lania-plugins-command-foo/
  Cargo.toml
  src/lib.rs
```

### 2. 声明命令

```rust
pub fn spec() -> CommandSpec {
    CommandSpec::new("foo", "Run foo", "command.foo")
        .with_options(vec![OptionSpec {
            long: "name".into(),
            short: Some('n'),
            help: "Name".into(),
            value_kind: ValueKind::String,
            default_value: None,
            choices: vec![],
            negatable: false,
        }])
}
```

### 3. 实现 handler

```rust
struct FooHandler;

#[async_trait(?Send)]
impl CommandHandler for FooHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        let name = ctx
            .command()
            .argv
            .options
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("world");

        let execution = lania_workflows::WorkflowExecution {
            workflow: "foo".into(),
            state: lania_workflows::WorkflowState::Completed,
            target_dir: ctx.command().cwd.clone(),
            prompts: Default::default(),
            bridge_steps: vec![],
            written_files: vec![],
            conflicts: vec![],
            command_plans: vec![],
            git_status: None,
            notes: vec![format!("hello {name}")],
            interactive_rendered: false,
        };

        Ok(CommandExecution::Workflow {
            context: ctx.command().clone(),
            execution,
            host_state: serde_json::json!({}),
            exit_code: 0,
        })
    }
}
```

### 4. 注册插件

```rust
impl Plugin for FooCommandPlugin {
    fn meta(&self) -> PluginMeta { /* ... */ }
    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_builtin_command::<FooHandler, _>(
            ctx,
            "command-foo",
            "foo",
            spec(),
            &["command.foo"],
            || FooHandler,
        )
    }
}
```

### 5. 在 main.rs 注册

```rust
host.register_plugin(Box::new(FooCommandPlugin))?;
```

### 6. 加测试

- `CommandSpec` 注册测试
- `CommandContext` → request 映射测试
- handler 执行测试
- 平台无关路径断言

## 12. 新增 Node Bridge 内置能力

如果能力属于 JavaScript 生态（例如新的编译器适配器、代码扫描器），应该加在 Node Bridge，而不是动态命令：

1. 在 `ts/packages/node-bridge/src/plugins/` 新建插件，导出 `name`、`methods`、`handle()`。
2. 在 `plugin-registry.ts` 注册。
3. 在 `protocol-catalog.ts` / `contract.test.ts` 更新方法清单。
4. 如果 Rust 需要调用，在 `lania-node-bridge` 增加 request 构造器。
5. 如果需要反向访问宿主，使用 `hostCall()` 并补充 Host RPC handler。
6. 增加 Node 测试与 Rust 协议测试。

插件的返回值约定：

```ts
return {
  result: { ok: true },
  events: [{ method: 'event.log', params: { level: 'info', message: 'done' } }],
};
```

`result` 会进入 `CommandExecution::Bridge.exchange.result`；`events` 会先于 response 写给 Rust。

## 13. 排查

| 现象                              | 检查                                                        |
| --------------------------------- | ----------------------------------------------------------- |
| 命令没出现                        | `extensions.dynamicCommands`、manifest 路径、mount 是否冲突 |
| 命令出现但执行报 method not found | handler 是否注册、target 是否正确                           |
| plugin is not declared            | `lan.config.plugins` 或 manifest `plugins`                  |
| plugin exposes no allowed methods | 方法交集为空                                                |
| prompt 不弹                       | `ui.interaction.mode`、TTY、`LANIA_PROMPT_ANSWERS_JSON`     |
| tools 被拒绝                      | `tools.allow/deny`、`exec/fs` 子策略                        |
| hook 不执行                       | hook key、handlerId 匹配、插件方法是否含 `hooks.invoke`     |

## 下一步

- [运行时清单参考](/reference/manifest)
- [ctx.tools API](/reference/host-tools)
- [Host RPC 方法目录](/reference/host-rpc)
- [Bridge 方法目录](/reference/bridge-methods)
