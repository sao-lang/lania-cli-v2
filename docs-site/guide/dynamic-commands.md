# 动态命令与插件

<div class="lead">
动态命令让你在不修改 Rust 宿主的前提下，把项目自己的命令挂到同一棵 CLI 命令树上。命令定义来自 lania.schemas.*，执行时由 Node Bridge 承接。
</div>

## 启动时序

```text
lan 启动
  → Rust 加载 lan.config.*
  → Node Bridge 扫描 lania.schemas.* / .lania/schemas/
  → 生成命令规格与 bridge handler
  → Rust 注册动态命令
  → 执行时转发 command.invokeDynamic
```

## 最小配置

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
};
```

## 最小命令

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      command: { about: 'Ops tools', alias: 'o' },
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
              input: ctx.argv.options,
              exitCode: 0,
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
```

## 参数与补问

命令可以声明：

- `args`：位置参数
- `options`：长选项
- `prompt`：缺失参数时的补问
- `when` / `goto` / `onAnswered`：补问流程控制

示例：

```ts
{
  name: 'deploy',
  about: 'Deploy service',
  args: [{ name: 'service', required: true, multiple: false }],
  options: [
    { long: 'project', valueKind: 'string' },
    { long: 'mode', valueKind: 'string', required: true },
    { long: 'confirm', valueKind: 'bool' }
  ],
  prompt: [
    {
      id: 'mode',
      field: 'mode',
      message: { zh: '请选择部署模式', en: 'Select deploy mode' },
      kind: 'select',
      choices: [
        { label: 'simple', value: 'simple' },
        { label: 'advanced', value: 'advanced' }
      ]
    }
  ],
  handler: async (ctx) => ({
    result: { ok: true, input: ctx.argv.options, exitCode: 0 },
    events: []
  })
}
```

## Hook

动态命令支持生命周期 Hook：

```ts
hooks: {
  preRun: [
    async () => ({
      events: [
        {
          method: 'event.log',
          params: { level: 'info', message: 'preRun hook' },
        },
      ],
    }),
  ];
}
```

常见 Hook：

- `onCommandPreInit`（旧名 `preRun`）
- `onArgsParsed`
- `onSuccess`
- `onError`

完整 20 个 hook key、默认 waterfall/parallel 语义与绑定写法见 [插件与 Hook 系统](/architecture/plugin-system) 与 [运行时清单参考](/reference/manifest#hookbindings)。Hook 由 Rust Hook Runtime 统一调度。

## 项目插件

```ts
export default {
  plugins: [
    {
      package: './lania.plugin.js',
      methods: ['commands.deploy', 'hooks.audit'],
    },
  ],
  hooks: {
    onSuccess: [
      {
        plugin: './lania.plugin.js',
        handler: 'hooks.audit',
      },
    ],
  },
  pluginAllowlist: ['./lania.plugin.js'],
  pluginMethodAllowlist: ['commands.deploy', 'hooks.audit'],
};
```

安全边界：

- 插件来源必须落在允许范围内。
- 插件方法必须在声明与 allowlist 的交集中。
- 可启用签名策略。

## ctx.tools

动态命令的 handler 可以通过 `ctx.tools` 使用受控宿主能力：

| 命名空间      | 作用                   | 执行位置                 |
| ------------- | ---------------------- | ------------------------ |
| `config`      | 读取配置快照           | Rust 配置快照 / 策略过滤 |
| `bridge`      | 调用 Bridge 内置方法   | Node 内部分发            |
| `compiler`    | 编译控制               | Node                     |
| `lint`        | Lint 执行              | Node                     |
| `path`        | 路径拼接与规范化       | Node                     |
| `workspace`   | 工作区根、项目信息     | Node + Host              |
| `env`         | 环境变量读取           | Node                     |
| `json`        | 读取 JSON / JSONC      | Node                     |
| `result`      | 构造标准结果           | Node                     |
| `text`        | 模板字符串、ASCII 渲染 | Node                     |
| `host`        | 反向 Host RPC 原始入口 | Rust                     |
| `exec`        | 子进程执行             | Rust                     |
| `git`         | Git 操作               | Rust                     |
| `pm`          | 包管理器检测与执行     | Rust                     |
| `scaffold`    | 模板脚手架             | Node + Rust              |
| `fs`          | 文件系统操作           | Rust                     |
| `log`         | 日志读写               | Rust                     |
| `tasks`       | 任务注册与进度         | Rust                     |
| `progress`    | 进度条                 | Rust                     |
| `interaction` | 交互补问               | Rust                     |

这些能力受 `lan.config.*` 中 `tools` 策略约束。`ctx.tools.bridge` 不是万能的：它只支持 `bridge.*`、`config.*`、`compiler.*`、`lint.*`、`system.*`、`commitizen.*`、`commitlint.*`，不支持 `product.*` 与 `template.*`。

完整的 API 表、策略字段和执行边界见 [ctx.tools 宿主能力](/guide/host-tools) 与 [ctx.tools API 参考](/reference/host-tools)。

## 示例工程

参考：

```text
examples/schema-tools-demo/
examples/product-cli-demo/
```

## 下一步

- [命令模型与加载原理](/guide/command-model)
- [运行时命令开发](/guide/runtime-commands)
- [插件与 Hook 系统](/architecture/plugin-system)
- [Schema 与协议](/reference/schemas)
- [运行时清单参考](/reference/manifest)
- [配置字段参考](/reference/config)
