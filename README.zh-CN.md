# Lania CLI v2

[English](./README.md) | [简体中文](./README.zh-CN.md)

`Lania CLI v2` 是一个以 Rust 为宿主、以 Node Bridge 承接 JavaScript 生态能力的双运行时 CLI。当前实现已经覆盖项目开发、模板脚手架、代码生成、发布编排、Git 同步，以及项目级自定义指令扩展。

它不只是一个“内置命令集合”，也可以作为一个可扩展的 CLI 宿主框架使用。项目可以通过 `lan.config.*`、`lania.schemas.*` 和项目插件，在不修改 Rust 宿主的前提下，向同一棵命令树里挂载自己的工程化命令、产品命令或领域命令。

## 文档导航

- [文档索引](./docs/README.zh-CN.md)：整理后的总入口，按用途区分阅读顺序。
- [学习地图](./docs/guides/learning-map.zh-CN.md)：面向首次接手仓库的开发者，解释目录结构、模块协作和关键链路。
- [命令与运行时 Roadmap](./docs/guides/command-runtime-roadmap.zh-CN.md)：按命令族梳理入口、handler、workflow / bridge / local 分支，以及关键函数追踪切口。
- [符号索引](./docs/README.zh-CN.md#guides)：补充 `create`、`generate module`、`release` 三条重链路的结构体、字段、函数和上下游调用索引。
- [模块设计与通信总览](./docs/architecture/模块设计与通信总览.zh-CN.md)：逐模块解释 npm wrapper、Rust host runtime、Node Bridge、插件系统、Dynamic Commands、Host RPC、workflow 与模板层之间的协作和通信。
- [框架化与产品化总览](./docs/architecture/框架化与产品化总览.zh-CN.md)：框架定位、作者 API 和产品化闭环。
- [专题设计与对账](./docs/design/专题设计与对账.zh-CN.md)：`ctx.tools`、模板边界、插件安全、类型契约与工程化增强收敛。
- [Node Bridge 协议](./docs/archive/phase0/node-bridge-protocol.md)：STDIO 通信协议与事件模型。
- [核心抽象](./docs/archive/phase0/core-abstractions.md)：Host、Command、Plugin、Workflow 等抽象定义。

## 当前能力

- 内置命令覆盖 `dev`、`build`、`lint`、`tools`、`create`、`add`、`template`、`generate api`、`generate module`、`release`、`sync`、`config`。
- 支持三类执行路径：
  - Bridge 型命令：`dev`、`build`、`lint`
  - 混合型命令：`tools`，其中 `tools list` 走 Node Bridge，`tools run/view` 走本地宿主能力
  - Workflow 型命令：`create`、`add`、`generate`、`release`、`sync`
  - 运行时扩展命令：由项目通过 `lan.config.*` 和 `lania.schemas.*` 动态注入
- 支持项目级插件与方法白名单、来源约束、签名策略等安全机制。
- 支持 `json`、`jsonl`、`human` 三种输出模式，以及 `spinner`、`bar`、`none` 三种进度渲染策略。
- 支持 `lan.config.js`、`lan.config.cjs`、`lan.config.json`、`lan.config.ts`。
- 支持模板元信息查询、项目脚手架生成、局部模板补充、契约生成、模块生成、发布状态恢复。
- 支持项目级动态命令与 Hook 绑定，允许在不修改 Rust 宿主的前提下扩展命令树。
- 支持将当前 CLI 作为项目级工程化 CLI 宿主进行二次开发：可以为团队封装自己的命令入口、交互流程、校验逻辑和宿主能力调用。

## 架构概览

系统由三部分组成：

1. `rust/`
   - 提供 `lania-cli` 二进制、HostRuntime、命令解析、工作流编排，以及日志、进度、交互、文件系统、Git、任务调度等宿主能力。
2. `ts/`
   - 提供 Node Bridge、配置加载、插件注册、安全校验、模板运行时，以及与 JS 工具链直接交互的能力。
3. `npm/`
   - 提供面向最终用户的 `lan` 包装器，负责选择平台二进制并为 Rust 宿主注入 Node Bridge 载荷路径。

一句话概括：

- Rust 负责命令树、执行编排和统一输出。
- Node 负责配置加载、模板运行时、编译器、Lint 与动态插件执行。
- npm 包负责安装和分发体验。

## 快速开始

### 使用 CLI

查看帮助：

```bash
lan --help
```

在当前项目中启动开发流程：

```bash
lan dev
```

创建新项目：

```bash
lan create --name demo-app --template spa-react
```

查看模板信息：

```bash
lan template
lan template toolkit
```

查看当前终端可直接运行的全局命令：

```bash
lan tools
lan tools --plain --names-only --unique
lan tools --group-by-source
```

运行代码文件或查看本地文件：

```bash
lan tools run ./scripts/demo.py
lan tools run ./scripts/demo.ts arg1 arg2
lan tools run ./scripts/demo.rs
lan tools run ./scripts/demo.go
lan tools view ./src/index.ts
lan tools view ./assets/logo.png
```

初始化契约生成配置：

```bash
lan generate api init
```

规划一次发布：

```bash
lan release plan --profile package --version 1.2.3
```

规划一次产品包发布：

```bash
lan build product
lan pack product
lan publish product --dist-tag next --channel next
```

执行 `publish-manifest` 的 dry-run：

```bash
lan publish product --execute --dry-run
```

说明：

- 这一步会先对 `publish-manifest.json` 做发布前校验，而不仅仅是“本地模拟执行”。
- 当前默认校验包含 `npm whoami` 与版本冲突检查，因此即使是 `--dry-run`，也需要目标 registry 可访问，且当前环境已经完成 npm 登录。
- 如果本机尚未登录 npm，建议先执行认证配置，或直接走本地 registry 演练：`pnpm publish:verdaccio`。
- 如果你的目标只是验证“产品 CLI 是否已经成功产出发布物”，执行 `lan publish product` 即可；它会生成 tarball、wrapper、`publish-report.json` 和 `publish-manifest.json`，但不会真正执行 registry publish。

对真实 registry 执行发布：

```bash
NODE_AUTH_TOKEN=xxxxx node ./scripts/configure-npm-auth.mjs \
  --manifest ./.lania/publish/product/npm-package/publish-manifest.json \
  --output ./.lania/tmp/npmrc.publish

NPM_CONFIG_USERCONFIG=./.lania/tmp/npmrc.publish \
./rust/target/debug/lania-cli publish product \
  --execute \
  --yes \
  --max-retries 2 \
  --retry-delay-ms 2000
```

如果要先做本地 registry 演练：

```bash
pnpm publish:verdaccio
```

如果要走 GitHub Actions 的一体化流水线：

- `Publish Product`：串联 `lan build product`、`lan pack product`、`lan publish product`
- `Publish Manifest`：对已有 `publish-manifest.json` 做受控执行
- `Publish Verdaccio Smoke`：自动跑 Verdaccio 发布演练

可以直接参考仓库内置的产品 CLI 示例：

- `examples/product-cli-demo/README.md`
- `examples/product-cli-demo/lan.config.json`
- `examples/product-cli-demo/product/lania.schemas.ts`

一个推荐的产品 CLI 打包/发布验证顺序：

```bash
lan product generate --preset demo --name "Acme CLI" --binary-name acme --output-dir ./acme-cli --force
cd ./acme-cli

lan product build
lan product pack
lan product publish

# 如果已经完成 npm 登录，再继续：
lan product publish --execute --dry-run --yes
```

这个顺序分别对应：

- `product generate`：生成一个可打包的产品 CLI 工程骨架
- `product build`：产出 product snapshot
- `product pack`：产出 install-root 布局
- `product publish`：生成 npm 包、wrapper、bundle 和 `publish-manifest.json`
- `product publish --execute --dry-run --yes`：对 manifest 做真正的发布预检与 dry-run 执行

### 仓库开发环境

```bash
cd ts
pnpm install
pnpm build
cd ../rust
cargo build -p lania-cli
```

如果需要从仓库根目录验证 CLI：

```bash
cd rust
cargo run -p lania-cli -- help
cargo run -p lania-cli -- create --preview --name demo-app
```

## 命令总览

| 命令 | 类型 | 作用 |
| --- | --- | --- |
| `dev` | Bridge | 启动当前项目的开发流程 |
| `build` | Bridge | 触发构建或 watch 构建 |
| `lint` | Bridge | 执行 ESLint、Prettier、Stylelint、Textlint 等检查或修复 |
| `tools` | 混合命令 | `list` 枚举环境命令；`run` 按文件类型匹配运行时执行代码；`view` 直接查看文本或调用系统查看器打开媒体文件 |
| `create` | Workflow | 以模板创建新项目 |
| `add` | Workflow | 向现有仓库补充局部模板、配置或代码片段 |
| `template` | Bridge 查询 | 列出模板或查看模板详情 |
| `generate api` | Workflow | 从 `lania.contract.yaml` 生成契约 DTO 与传输层代码 |
| `generate module` | Workflow | 从 `lania.module.yaml` 生成 `lania-g` 相关模块与注入代码 |
| `release` | Workflow | 规划、执行、恢复或查询一次发布流程 |
| `sync` | Workflow | 暂存、提交、推送当前工作区变更 |
| `config` | Workflow | 读取或更新 CLI 全局偏好，例如语言和输出模式 |

## 关键链路

### `dev` / `build` / `lint`

- `lan` 包装器启动 Rust 二进制。
- Rust Host 完成命令解析，并把请求转发给 Node Bridge。
- Node Bridge 根据方法名分发到 `compiler` 或 `lint` 插件。
- `compiler` / `lint` 插件负责工具链执行。
- 运行事件和结果通过 Bridge 回传给 Rust，由 Host 统一渲染并生成最终输出。

### `tools list`

- 默认返回当前环境中“终端可直接解析”的命令集合，包含：
  - `PATH` 中的可执行文件与脚本
  - 当前 shell 的 `builtin`
  - 当前 shell 的 `alias`
  - 当前 shell 的 `function`
- 当前 shell 采集优先支持 `zsh` / `bash`；其他 shell 会自动降级为只返回 `PATH` 命令。
- 常用参数：
  - `--filter <text>`：按命令名子串过滤
  - `--all-matches`：展开 `PATH` 中同名命令的多路径命中
  - `--no-shell`：只返回 `PATH` 命令
  - `--names-only`：只返回命令名列表
  - `--group-by-source`：按 `PATH` / `builtin` / `alias` / `function` 分组
  - `--plain`：输出纯文本
  - `--unique`：按首次出现顺序去重命令名

### `tools run`

- 对输入文件优先解析 shebang；若文件头已声明运行时，将优先按 shebang 执行。
- 支持常见的 shebang 形式，包括：
  - `#!/usr/bin/env python3`
  - `#!/usr/bin/env -S node --no-warnings`
  - `#!/usr/bin/env FOO=bar python3 -u`
- JS/TS 运行时会优先查找项目本地 `node_modules/.bin`，再回退到系统 `PATH`。
- 若没有 shebang，则按扩展名匹配运行时：
  - `js` / `mjs` / `cjs`：优先 `node`，回退 `bun`
  - `ts` / `tsx` / `jsx`：优先 `tsx`，回退 `bun`、`ts-node`
  - `py`：优先 `python3`，回退 `python`
  - `sh` / `bash` / `zsh`：优先扩展名对应 shell，再回退 `bash`、`sh`
  - `rb`：`ruby`
  - `php`：`php`
  - `go`：`go run`
  - `java`：`java <file>`
  - `lua`：`lua`
  - `dart`：`dart run`
  - `nim`：`nim r`
  - `zig`：`zig run`
  - `kotlin` / `kts`：优先 `kotlin`，回退 `kotlinc -script`
  - `swift`：`swift`
- 对需要编译后执行的单文件语言，当前策略为：
  - `rs`：`rustc <file> -o <tmp-bin>` 后执行
  - `c`：`cc/clang/gcc <file> -o <tmp-bin>` 后执行
- 编译型语言会先执行 prepare 阶段；如果编译失败，会直接返回编译器错误，不再继续执行临时产物。
- 文件后的额外参数会透传给目标脚本，例如：`lan tools run ./demo.py a b`

### `tools view`

- 文本文件会直接输出内容，并带行号、字节大小与实际行范围。
- 可通过以下参数增强文本查看：
  - `--lines <N>`：限制最多输出多少行
  - `--head <N>`：查看前 N 行
  - `--start <N>`：从第 N 行开始查看
  - `--end <N>`：查看到第 N 行结束
  - `--tail <N>`：查看最后 N 行
- `--grep <text>`：按子串过滤文本行或目录项
- `--regex <pattern>`：按正则过滤文本行或目录项
- `--ignore-case`：对子串或正则搜索启用忽略大小写
- 目录路径会直接列出目录内容，并支持 `--head/--tail/--grep`
- `--tree`：以递归树形方式查看目录内容
- `--files-only`：目录查看时只保留文件项
- `--dirs-only`：目录查看时只保留目录项
- 树模式过滤命中子项时，会自动保留父目录上下文
- `--max-depth <N>`：限制目录树向下递归的层级深度
- `--sort name|size|time|ext`：按名称、大小、修改时间或扩展名排序目录项
- `--reverse`：反转目录排序结果
- `--hidden`：显示隐藏文件和隐藏目录
- `--tree` 现在使用更接近 `tree` 命令的连接线样式显示目录结构
- 图片、视频、音频、PDF 会识别媒体类型并调用系统默认查看器打开。
- 未知二进制文件默认输出十六进制预览，而不是直接拉起外部查看器。
- `--hex-bytes <N>`：限制二进制十六进制预览的字节数
- 可通过 `--meta-only` 只查看文件元信息，不真正打开外部查看器。

### `create` / `add`

- Rust 命令插件先把 CLI 参数转成工作流输入。
- `lania-workflows` 负责问题补全、任务编排、文件计划、冲突检测与写入事务。
- 需要模板目录、模板问题或渲染内容时，工作流通过 Node Bridge 访问 `template` 插件。
- 模板运行时由 `@lania-cli/templates` 提供，最终由 Rust 完成文件落盘、依赖安装、Git 初始化等宿主动作。

### `generate api` / `generate module`

- Rust 工作流读取对应 YAML 配置。
- 生成链路负责构建计划、输出差异、清理旧文件、维护 manifest 锁文件。
- `generate module` 还会负责 `main.go` 标记块注入与幂等校验。
- `generate module` 支持按 `inputs[].targets` 做 source -> target 映射，并生成 `lania-g` 风格 DSL 注册代码：
  - `protobuf -> grpc`
  - `thrift -> http`
  - `json|yaml -> ws`
  - `graphql -> graphql`
- schema 内可直接声明 transport metadata：
  - `proto/thrift`：单行注释 `// lania:...`
  - `json/yaml`：扩展字段 `x-lania-operations`
  - 详细规范见 [模块设计与通信总览](./docs/architecture/模块设计与通信总览.zh-CN.md) 与 [专题设计与对账](./docs/design/专题设计与对账.zh-CN.md)

### `release` / `sync`

- 统一由 Rust 工作流执行。
- 通过 `lania-config`、`lania-git`、`lania-exec`、`lania-pm`、`lania-task`、`lania-progress` 等服务完成配置解析、Git 查询、命令执行、任务编排与状态持久化。
- `release` 使用 `.lania/release-state.json` 进行状态恢复；`sync` 支持普通提交、交互式提交和独立推送子命令。

### 自定义指令

- 项目在 `lan.config.*` 中开启 `extensions.dynamicCommands`。
- Node Bridge 扫描 `lania.schemas.*` 或 `.lania/schemas/` 下的 manifest，生成动态命令树与 handler 绑定。
- Rust Host 在启动阶段注册这些命令，并在运行时把执行请求转发回 Node Bridge。
- 动态 handler 可执行本地函数或项目插件方法；Hook 绑定则由 Rust Hook Runtime 统一调度。

### 自定义指令配置示例

最小 `lan.config.ts`：

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
};
```

更完整的 `lan.config.ts`：

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
  schemaDiscovery: {
    files: ['lania.schemas.ts', 'lania.schemas.js'],
    dirs: ['.lania/schemas'],
    allowExtensions: ['.ts', '.js', '.json', '.yaml', '.yml'],
  },
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
  ui: {
    output: { mode: 'human' },
    progress: { mode: 'spinner' },
  },
  pluginAllowlist: ['./lania.plugin.js'],
  pluginMethodAllowlist: ['commands.deploy', 'hooks.audit'],
};
```

说明：

- `extensions.dynamicCommands`：开启运行时动态命令发现与注册。
- `schemaDiscovery`：控制去哪里扫描 `lania.schemas.*` manifest。默认会扫描仓库根的 `lania.schemas.ts/js/cjs`，以及 `.lania/schemas/` 目录。
- `plugins`：声明项目插件；如果 manifest handler 指向插件方法，插件必须先在这里声明。
- `hooks`：全局 Hook 绑定，作用于内置命令和动态命令的统一生命周期。
- `pluginAllowlist` / `pluginMethodAllowlist`：限制哪些插件和方法可以被加载/调用。

### 自定义指令 Manifest 示例

最小 `lania.schemas.ts`：

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
            { long: 'endpoint', valueKind: 'string', help: 'Endpoint', required: true },
          ],
          prompt: [
            {
              field: 'endpoint',
              message: 'Endpoint?',
              kind: 'input',
              whenMissing: ['endpoint'],
            },
          ],
          hooks: {
            preRun: [
              async () => ({
                events: [
                  { method: 'event.log', params: { level: 'info', message: 'preRun hook' } },
                ],
              }),
            ],
          },
          handler: async (ctx) => ({
            result: { ok: true, input: ctx.argv.options, exitCode: 0 },
            events: [],
          }),
        },
      ],
    },
  ],
};
```

完整流程版 `lania.schemas.ts`（含 `when/goto/onAnswered`）：

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      command: { about: 'Ops tools', alias: 'o' },
      commands: [
        {
          name: 'deploy',
          about: 'Deploy service',
          args: [{ name: 'service', required: true, multiple: false, help: 'service name' }],
          options: [
            { long: 'project', valueKind: 'string', help: 'project key' },
            { long: 'mode', valueKind: 'string', help: 'deploy mode', required: true },
            { long: 'region', valueKind: 'string', help: 'deploy region' },
            { long: 'confirm', valueKind: 'bool', help: 'need confirm' },
          ],
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
              when: { type: 'truthy', key: 'project' },
              goto: 'confirm',
              validate: ['required', { type: 'one_of', values: ['simple', 'advanced'] }],
              mapFunctions: ['trim', { type: 'lowercase' }],
              onAnswered: [
                {
                  type: 'set_context_from_answer',
                  key: 'isAdvanced',
                  mapFunctions: [{ type: 'split', separator: ',' }],
                },
                {
                  type: 'goto_if',
                  when: { type: 'equals', key: 'mode', value: 'advanced' },
                  target: 'region',
                },
              ],
            },
            {
              id: 'region',
              field: 'region',
              message: { zh: '请输入 region', en: 'Input region' },
              kind: 'input',
              when: { type: 'equals', key: 'mode', value: 'advanced' },
              validate: [{ type: 'min_length', min: 2 }],
              timeoutMs: 10000,
              contextKey: 'deployRegion',
            },
            {
              id: 'confirm',
              field: 'confirm',
              message: { zh: '确认执行部署？', en: 'Confirm deploy?' },
              kind: 'confirm',
              defaultValue: false,
              returnable: true,
            },
          ],
          hooks: {
            onArgsParsed: [
              {
                type: 'plugin',
                kind: 'waterfall',
                plugin: './lania.plugin.js',
                handler: 'commands.normalizeDeployArgs',
              },
            ],
          },
          handler: async (ctx) => ({
            result: {
              ok: true,
              traceId: ctx.traceId,
              mount: ctx.mount,
              path: ctx.path,
              service: ctx.argv.args.service,
              options: ctx.argv.options,
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

如果要调用项目插件方法而不是内联函数：

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'deploy',
          about: 'Deploy project',
          handler: {
            plugin: './lania.plugin.js',
            method: 'commands.deploy',
          },
        },
      ],
    },
  ],
};
```

说明：

- `mount` 会成为顶级命令，例如上例生成 `lan ops ping`。
- `commands` 支持多级 `subcommands`。
- `handler` 可以是内联函数，也可以是 `{ plugin, method }`。
- `prompt` 用于缺参补问；`required: true` 会让 Rust Host 在执行前校验必填选项。
- `prompt` 现在也支持 `lania-prompt` 的流程字段子集：
  - `when`
  - `goto`
  - `validate`
  - `timeoutMs`
  - `contextKey`
  - `accumulation`
  - `returnable`
  - `mapFunctions`
  - `onAnswered`
- `prompt.whenMissing` 仍然可用，但它只是“缺参才问”的兼容写法；复杂分支建议直接用 `when/goto/onAnswered`。
- `hooks.preRun` / `hooks.postRun` 会分别映射到 `onCommandPreInit` / `onSuccess`。
- 顶级保留命令如 `dev`、`build`、`generate`、`release` 不能被 `mount` 覆盖。

## 用当前 CLI 开发项目自己的 CLI

如果你想做的不只是“给现有命令加一点配置”，而是希望基于当前 CLI 开发团队自己的工程化 CLI，这个仓库已经具备完整路径：

- 用 `lan.config.*` 开启扩展能力、限制插件来源和宿主权限
- 用 `lania.schemas.*` 描述命令树、参数、交互流程和 Hook
- 用 inline handler 或项目插件实现真正的业务逻辑
- 在 handler 里通过 `ctx.tools` 调用 `git`、`fs`、`exec`、`pm`、`workspace`、`log` 等宿主能力
- 继续复用现有 CLI 的统一输出、进度、日志、审计和策略边界

这意味着你可以把 `lan` 看作“团队 CLI 的运行时底座”，而不是只能使用内置的 `dev/build/lint/create`。

### 一个最小 demo

下面这个 demo 会给当前项目挂载一个 `lan team hello` 命令：

- 读取当前工作区的 `package.json`
- 获取当前 Git 分支
- 在 `.lania/reports/team-cli.json` 中写一份报告
- 返回结构化结果，并沿用现有 CLI 的输出模式

`lan.config.ts`：

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
  schemaDiscovery: {
    files: ['lania.schemas.ts'],
  },
  tools: {
    allow: ['workspace', 'git', 'fs', 'path', 'log', 'result'],
    fs: {
      writeRoot: '.',
    },
  },
};
```

`lania.schemas.ts`：

```ts
export default {
  runtimeCommands: [
    {
      mount: 'team',
      command: {
        about: 'Team engineering commands',
      },
      commands: [
        {
          name: 'hello',
          about: 'Generate a simple team CLI report',
          options: [
            {
              long: 'name',
              valueKind: 'string',
              help: 'Your name',
            },
          ],
          prompt: [
            {
              field: 'name',
              message: 'Your name?',
              kind: 'input',
              whenMissing: ['name'],
            },
          ],
          handler: async (ctx) => {
            const name = ctx.argv.options.name ?? 'anonymous';
            const reportPath = ctx.tools.path.resolve(
              ctx.cwd,
              '.lania',
              'reports',
              'team-cli.json',
            );
            const pkg = await ctx.tools.workspace.packageJson();
            const branch = await ctx.tools.git.branch.current();

            await ctx.tools.fs.ensureDir(ctx.tools.path.dirname(reportPath));
            await ctx.tools.fs.writeJson(reportPath, {
              name,
              packageName: pkg?.name ?? null,
              branch,
            });

            await ctx.tools.log.success(`team report written: ${reportPath}`);

            return ctx.tools.result.ok({
              greeting: `hello, ${name}`,
              packageName: pkg?.name ?? null,
              branch,
              reportPath,
            });
          },
        },
      ],
    },
  ],
};
```

运行方式：

```bash
lan team hello --name alice
```

或者直接走交互：

```bash
lan team hello
```

执行完成后，你会得到两类结果：

- 终端里看到统一渲染的命令结果
- 工作区里生成 `.lania/reports/team-cli.json`

当你想让业务逻辑更复杂时，再把 inline `handler` 提取到项目插件文件中即可。

### 从零到可用的完整流程

如果你的目标是“为团队开发一个真正可落地的项目 CLI”，推荐按下面顺序推进：

1. 明确命令边界
   - 先定义你要做的是“项目命令”还是“通用工具命令”。
   - 例如：`lan app deploy`、`lan ops inspect`、`lan team bootstrap`。

2. 打开扩展能力
   - 在 `lan.config.ts` 里开启 `extensions.dynamicCommands: true`。
   - 同时配置 `schemaDiscovery`，明确从哪些文件或目录发现命令清单。

3. 先写命令树，不急着写实现
   - 在 `lania.schemas.ts` 里先写出 `mount`、子命令、参数、选项、`prompt`、Hook。
   - 先把命令的使用方式和交互体验定下来，再补真正逻辑，后续维护会轻松很多。

4. 用 inline handler 快速验证
   - 在 manifest 里先直接写 `handler: async (ctx) => { ... }`。
   - 适合验证命令模型、参数补问、结果结构和 `ctx.tools` 是否够用。

5. 复杂后再迁移到项目插件
   - 当逻辑开始变长，或需要多人协作维护时，把实现迁移到 `lania.plugin.ts/js`。
   - 然后在 `lan.config.ts` 的 `plugins` 中声明插件，在 `handler` 里改成 `{ plugin, method }` 形式。

6. 给命令补上 Hook 和治理策略
   - 用 `onArgsParsed`、`preRun`、`postRun` 等 Hook 处理补参、校验、审计和收尾动作。
   - 用 `pluginAllowlist`、`pluginMethodAllowlist`、`tools.allow` 等策略限制能力边界。

7. 复用宿主能力，而不是重复造轮子
   - 文件写入优先走 `ctx.tools.fs`
   - 命令执行优先走 `ctx.tools.exec`
   - Git 查询优先走 `ctx.tools.git`
   - 工作区信息优先走 `ctx.tools.workspace`
   - 这样可以持续复用当前 CLI 的统一治理和输出能力。

8. 固化为团队 CLI 约定
   - 把 `lan.config.ts`、`lania.schemas.ts`、`lania.plugin.ts` 放进项目模板或基建仓库。
   - 让业务仓库只关心“声明自己的命令和逻辑”，而不是反复搭一套新的 CLI 外壳。

### 从 demo 到生产化的演进方式

一个比较自然的演进路径是：

1. 用 inline `handler` 跑通一个命令
2. 把共享逻辑收敛进 `lania.plugin.ts`
3. 把命令定义沉淀进 `lania.schemas.ts`
4. 把默认策略和 UI 体验沉淀进 `lan.config.ts`
5. 最后把这三类文件放进模板，让新项目一键拥有团队 CLI

如果你想看一个更完整、偏运行时能力演示的示例，可以直接参考仓库内置的：

- `examples/schema-tools-demo/lan.config.ts`
- `examples/schema-tools-demo/lania.schemas.ts`
- `examples/schema-tools-demo/README.md`
- `examples/product-cli-demo/README.md`
- `examples/product-cli-demo/lan.config.json`
- `examples/product-cli-demo/product/lania.schemas.ts`

### 动态命令执行时序（交互 -> 参数 -> handler）

执行顺序（简化）：

1. 解析 CLI 参数（得到 `argv.args` / `argv.options`）
2. `onArgsParsed`（可改写参数）
3. 执行 `prompt`（支持 `when/goto/onAnswered`；已有参数会注入 prompt context）
4. 合并交互答案到 `argv.options`
5. 调用真正的 `handler(ctx)`
6. `onSuccess` / `onError`

`handler` 入参 `ctx` 关键字段：

- `cwd`
- `mount`
- `path`
- `traceId`
- `argv.args`
- `argv.options`

其中 `argv.*` 是最终值：已包含参数解析、Hook 改写和 prompt 结果。

### `ctx.tools` 与示例仓库

动态命令 handler 和 inline hook 现在都会注入统一的 `ctx.tools` 运行时对象。常用能力包括：

- 本地能力：`bridge`、`config`、`text`、`path`、`workspace`、`env`、`json`、`result`
- Host-backed 能力：`host`、`git`、`pm`、`exec`、`fs`、`log`、`tasks`、`progress`、`interaction`

可以直接参考仓库内置示例：

- `examples/schema-tools-demo/lan.config.ts`
- `examples/schema-tools-demo/lania.schemas.ts`
- `examples/schema-tools-demo/README.md`

这个示例仓库演示了两条典型链路：

1. inline `onArgsParsed` hook 使用 `ctx.tools.text` 和 `ctx.tools.bridge.call('bridge.ping')`
2. handler 使用 `ctx.tools.workspace`、`ctx.tools.pm`、`ctx.tools.git`、`ctx.tools.exec`、`ctx.tools.fs`、`ctx.tools.log` 与 `ctx.tools.result`

最小示例：

```ts
handler: async (ctx) => {
  const branch = await ctx.tools.git.branch.current();
  const manager = await ctx.tools.pm.detect();
  const out = ctx.tools.path.resolve(ctx.cwd, '.lania', 'report.json');

  await ctx.tools.fs.writeJson(out, { branch, manager });
  await ctx.tools.log.success(`report written: ${out}`);

  return ctx.tools.result.ok({ branch, manager, out });
};
```

## 配置文件

### `lan.config.*`

主配置文件由 Node Bridge 负责加载，当前支持：

- `lan.config.js`
- `lan.config.cjs`
- `lan.config.json`
- `lan.config.ts`

当前运行时重点消费以下字段：

| 字段 | 作用 |
| --- | --- |
| `buildTool` / `buildAdaptors` | 构建工具选择与适配器配置 |
| `lintTools` / `lintAdaptors` | Lint 工具与适配器配置 |
| `plugins` | 项目级插件声明 |
| `release` | 发布默认配置 |
| `extensions.dynamicCommands` | 是否启用运行时动态命令 |
| `schemaDiscovery` | 动态命令 manifest 的发现策略 |
| `hooks` | 全局 Hook 绑定 |
| `ui.output` | 输出模式与事件输出策略 |
| `ui.progress` | 进度展示模式 |
| `ui.interaction` | 交互模式、超时和默认值策略 |
| `pluginAllowlist` / `pluginMethodAllowlist` 等 | 插件安全边界 |

动态命令相关的默认发现规则：

- `schemaDiscovery.files`
  - `lania.schemas.ts`
  - `lania.schemas.js`
  - `lania.schemas.cjs`
- `schemaDiscovery.dirs`
  - `.lania/schemas`
- `schemaDiscovery.allowExtensions`
  - `.ts`
  - `.js`
  - `.cjs`
  - `.json`
  - `.yaml`
  - `.yml`

### 生成与状态文件

| 文件 | 用途 |
| --- | --- |
| `lania.contract.yaml` | `generate api` 的输入配置 |
| `lania.module.yaml` | `generate module` 的输入配置 |
| `.lania/contracts.lock.json` | 契约生成输出跟踪 |
| `.lania/module-gen.lock.json` | 模块生成输出跟踪 |
| `.lania/release-state.json` | 发布流程状态快照 |

## 输出与交互

- `stdout` 以结构化结果为核心，适合自动化和二次消费。
- `stderr` 承担友好错误提示与人类可读日志输出。
- 可通过 `lan config set output.mode <json|jsonl|human>` 设置默认输出风格。
- 交互链路由 Rust PromptService 统一控制；非交互环境可通过默认值策略或脚本化答案运行。

## 安装与分发

最终安装形态要求 Rust 二进制和 Node Bridge 载荷共同存在。典型布局如下：

```text
<install-root>/
  bin/
    lan
    lania-cli
  lib/
    node-bridge/
      package.json
      dist/
      node_modules/
```

Bridge 目录查找顺序如下：

1. `LANIA_NODE_BRIDGE_DIR`
2. `<current-exe>/../lib/node-bridge`
3. `<current-exe>/../node-bridge`
4. `<current-exe>/../Resources/node-bridge`
5. 开发态工作区 `ts/packages/node-bridge`

## 仓库结构

```text
lania-cli-v2/
  README.md
  README.zh-CN.md
  docs/
  npm/
  rust/
  ts/
```

更详细的目录说明见 [目录结构参考](./docs/references/ai/directory-structure.md)。

## 推荐阅读顺序

- 先看 [学习地图](./docs/guides/learning-map.zh-CN.md)，建立整体心智模型。
- 再看 [模块设计与通信总览](./docs/architecture/模块设计与通信总览.zh-CN.md)，理解执行边界、模块职责与通信模型。
- 如果关心框架化和产品化方向，再看 [框架化与产品化总览](./docs/architecture/框架化与产品化总览.zh-CN.md)。
- 如果要研究 Bridge 协议，再看 [Node Bridge 协议](./docs/archive/phase0/node-bridge-protocol.md)。

## 当前建议的验证命令

```bash
cd rust
cargo test -p lania-cli
cargo test -p lania-workflows
cargo run -p lania-cli -- help
cargo run -p lania-cli -- template
```
