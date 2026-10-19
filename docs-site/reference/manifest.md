# 运行时清单参考

<div class="lead">
本页是 lania.schemas.* / .lania/schemas/* 的字段参考。它同时覆盖 runtimeCommands、commands、workflows、templates、plugins、presets、features、dependencyRecipes、mergeRules、guards、resolvers、postActions。
</div>

## 文件发现

```ts
export default {
  extensions: { dynamicCommands: true },
  schemaDiscovery: {
    files: ['lania.schemas.ts', 'lania.schemas.js', 'lania.schemas.cjs'],
    dirs: ['.lania/schemas'],
    allowExtensions: ['.ts', '.js', '.cjs', '.json', '.yaml', '.yml'],
  },
  schema: {
    entry: './product/lania.schemas.ts',
  },
};
```

显式入口：

- `schema.entry`（string 或 string[]）
- `product.schemaEntry`

显式入口 + 默认 files + dirs 会合并去重。

## 顶层结构

```ts
export default {
  runtimeCommands: [],
  commands: [],
  workflows: {},
  templates: [],
  plugins: [],
  presets: {},
  features: {},
  dependencyRecipes: {},
  mergeRules: {},
  guards: {},
  resolvers: {},
  postActions: {},
};
```

## runtimeCommands[]

```ts
interface RuntimeCommandConfig {
  mount: string;
  runtime?: Record<string, unknown>;
  options?: Record<string, unknown>;
  command?: CommandPackMeta;
  commands: CommandDeclaration[];
  schemas?: RuntimeSchemaDeclaration[]; // deprecated
}
```

### mount

挂载点名称，例如：

```ts
{
  mount: 'ops';
}
```

生成：

```text
lan ops <command>
```

会做 `sanitizeSegment`，并检查是否与保留命令冲突。

保留命令：

```text
dev, build, lint, create, add, sync, template, release, generate, help
```

实际还应避免与 `product`、`tools`、`config` 等所有内置命令重名。

### command

```ts
{
  about?: string
  alias?: string
  aliases?: string[]
  examples?: { command: string; description: string }[]
}
```

### schemas（deprecated）

旧版 schema converter：

```ts
schemas: [
  {
    plugin: './schema-converter.ts',
    method: 'convert',
    kind: 'openapi',
    source: './openapi.yaml',
  },
];
```

插件必须在 `lan.config.plugins` 或 manifest `plugins` 中声明。新项目应优先使用 `commands`。

## commands[]

```ts
interface CommandDeclaration {
  name: string;
  about?: string;
  alias?: string;
  aliases?: string[];
  args?: Array<{
    name: string;
    required?: boolean;
    multiple?: boolean;
    help?: string;
  }>;
  options?: Array<{
    long: string;
    short?: string;
    help?: string;
    valueKind?: 'bool' | 'string' | 'number' | 'optional_string';
    defaultValue?: unknown;
    choices?: Array<string | number>;
    required?: boolean;
    negatable?: boolean;
  }>;
  examples?: Array<{ command: string; description: string }>;
  prompt?: PromptSpec[];
  promptFlow?: string;
  handler?: CommandHandlerSpec | DynamicHandlerFn;
  workflow?: string;
  preset?: string;
  features?: string[];
  dependencyRecipes?: string[];
  mergeRules?: string[];
  guards?: string[];
  postActions?: string[];
  hooks?: HookBindings;
  plugin?: unknown;
  when?: ConditionSchema;
  subcommands?: CommandDeclaration[];
}
```

### handler

两种写法：

```ts
handler: async (ctx) => ({ result: {}, events: [] });
```

```ts
handler: {
  plugin: './lania.plugin.ts',
  method: 'commands.deploy'
}
```

也兼容：

```ts
handler: {
  type: 'plugin',
  plugin: './lania.plugin.ts',
  method: 'commands.deploy'
}
```

### workflow

```ts
workflow: 'deploy';
```

引用顶层 `workflows.deploy`。

### 分组命令

```ts
{
  name: 'db',
  about: 'Database commands',
  subcommands: [
    { name: 'migrate', handler: async () => ({ result: {}, events: [] }) },
    { name: 'seed', handler: async () => ({ result: {}, events: [] }) }
  ]
}
```

分组节点会返回 `group_summary`。

## PromptSpec

```ts
interface PromptSpec {
  id?: string;
  message: string | Record<string, string>;
  field: string;
  kind?: PromptKind;
  choices?: Array<{ label: string; value: unknown }>;
  defaultValue?: unknown;
  whenMissing?: string[];
  detail?: string | Record<string, string>;
  when?: PromptWhenSpec;
  goto?: string;
  validate?: PromptValidationSpec[];
  timeoutMs?: number;
  contextKey?: string;
  accumulation?: 'replace' | 'append';
  returnable?: boolean;
  mapFunctions?: PromptMapFunctionSpec[];
  onAnswered?: PromptOnAnsweredActionSpec[];
}
```

### kind

```text
input
select
confirm
multi_select
password
editor
number
fuzzy_select
autocomplete
search
rawlist
expand
```

### when

```ts
{ type: 'equals', key: 'mode', value: 'advanced' }
{ type: 'not_equals', key: 'mode', value: 'simple' }
{ type: 'exists', key: 'region' }
{ type: 'truthy', key: 'confirm' }
```

### validate

```ts
validate: [
  'required',
  { type: 'min_length', min: 3 },
  { type: 'one_of', values: ['staging', 'prod'] },
];
```

### mapFunctions

```ts
mapFunctions: [
  'trim',
  'lowercase',
  'uppercase',
  'to_number',
  'json_parse',
  { type: 'split', separator: ',' },
];
```

### onAnswered

```ts
onAnswered: [
  { type: 'set_context_value', key: 'mode', value: 'simple' },
  { type: 'set_context_from_answer', key: 'project', field: 'project' },
  { type: 'goto', target: 'next-step' },
  { type: 'goto_if', when: { type: 'equals', key: 'mode', value: 'advanced' }, target: 'advanced' },
];
```

### 本地化

```ts
message: { zh: '请选择环境', en: 'Select environment' }
```

### 非交互

- `ui.interaction.mode`
- `ui.interaction.defaultStrategy`
- `ui.interaction.timeoutMs`
- `LANIA_PROMPT_ANSWERS_JSON`

## HookBindings

```ts
interface HookBindings {
  [hookKey: string]: HookBinding[];
}
```

### inline

```ts
{
  type?: 'inline'
  kind?: 'waterfall' | 'parallel'
  id?: string
}
```

函数型 hook 会在解析时自动转成 inline id。

### plugin

```ts
{
  type: 'plugin'
  kind?: 'waterfall' | 'parallel'
  plugin: string
  handler: string
  timeoutMs?: number
  onError?: 'throw' | 'collect'
}
```

### 默认 kind

waterfall：

`onCommandPreInit`、`onArgsParsed`、`onFilesPrepare`、`onConfigGet`、`onConfigResolve`、`onTemplateParse`、`onDependenciesModify`、`onInteractionPrompt`。

其他默认 parallel。

## workflows

支持三种形态：

```ts
workflows: {
  deploy: {
    plugin: './lania.plugin.ts',
    method: 'commands.deploy'
  }
}
```

```ts
workflows: {
  deploy: async (ctx) => ({ result: {}, events: [] });
}
```

```ts
workflows: {
  deploy: {
    guards: ['empty-dir'],
    steps: ['preflight', 'renderTemplates', 'writeFiles', 'printSummary']
  }
}
```

### steps

| step                  | 说明                 |
| --------------------- | -------------------- |
| `preflight`           | 执行 guards          |
| `resolvePreset`       | 解析 preset          |
| `resolveFeatures`     | 解析 features        |
| `renderTemplates`     | 渲染 template layers |
| `mergeFiles`          | 执行 merge rules     |
| `writeFiles`          | 写文件并注册回滚     |
| `installDependencies` | 安装依赖             |
| `gitInit`             | 初始化 Git           |
| `postActions`         | 执行 post actions    |
| `printSummary`        | 输出摘要             |

## templates

```ts
templates: [
  {
    id: 'base',
    title: 'Base template',
    description?: string,
    tags?: string[],
    variables?: Array<{
      key: string
      type?: string
      required?: boolean
      default?: unknown
      description?: string
      choices?: Array<string | number>
    }>,
    hooks?: Record<string, unknown>
  }
]
```

## plugins

```ts
plugins: [
  './lania.plugin.ts',
  {
    name?: string
    package: string
    methods?: string[]
    signature?: string
    hooks?: Record<string, unknown>
  }
]
```

## presets / features / dependencyRecipes

```ts
presets: {
  app: {
    about?: string
    templateLayers?: string[]
    features?: string[]
    dependencyRecipes?: string[]
    mergeRules?: string[]
    guards?: string[]
    postActions?: string[]
    hooks?: Record<string, unknown>
    when?: ConditionSchema
  }
}

features: {
  auth: {
    about?: string
    templateLayers?: string[]
    dependencyRecipes?: string[]
    mergeRules?: string[]
    hooks?: Record<string, unknown>
    when?: ConditionSchema
  }
}

dependencyRecipes: {
  base: {
    dependencies?: string[]
    devDependencies?: string[]
    scripts?: Record<string, string>
    packageManager?: string
    when?: ConditionSchema
  }
}
```

## mergeRules / guards / resolvers / postActions

```ts
mergeRules: {
  'package-json': {
    target: 'package.json',
    strategy: 'deep_merge',
    when?: ConditionSchema
  }
}
```

`strategy`：

- `replace`
- `append`
- `deep_merge`
- `dedupe_merge`
- `patch`

```ts
guards: {
  'empty-dir': { type: 'directory_empty', when?: ConditionSchema },
  node: { type: 'node_version', range: '>=20', when?: ConditionSchema },
  git: { type: 'command_exists', command: 'git', when?: ConditionSchema },
  monorepo: { type: 'workspace_kind', value: 'monorepo', when?: ConditionSchema }
}
```

```ts
resolvers: {
  templateDir: { from: 'options.template', use: 'template', when?: ConditionSchema }
}
```

```ts
postActions: {
  install: { type: 'install_dependencies', when?: ConditionSchema },
  git: { type: 'git_init' },
  commit: { type: 'git_first_commit' },
  summary: { type: 'print_summary', message: 'Done' }
}
```

## ConditionSchema

```ts
{ field: 'mode', equals: 'advanced' }
{ field: 'features', in: ['auth'] }
{ field: 'force', truthy: true }
{ all: [ ... ] }
{ any: [ ... ] }
```

字段读取规则：

- 不含 `.`：优先 `options`，再 `args`，再 scope 顶层
- 含 `.`：按点路径读取，例如 `argv.options.template`

## 解析警告

以下情况会进入 warnings，但不会阻断整个 manifest：

- mount 为空
- mount 与保留命令冲突
- workflow 未定义
- schema converter 插件未声明
- hook key 不认识
- 局部命令缺少 name

启动日志会输出这些 warning。

## 下一步

- [开发运行时命令](/guide/runtime-commands)
- [ctx.tools API](/reference/host-tools)
- [Schema 与协议](/reference/schemas)
