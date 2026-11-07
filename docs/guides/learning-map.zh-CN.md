# Lania CLI v2 学习地图

这份文档面向第一次接手 `lania-cli-v2` 的开发者。目标不是罗列全部实现细节，而是帮助你在较短时间内建立以下认知：

- 仓库由哪些部分组成
- 各部分如何协作
- 内置命令与自定义指令分别经过哪些执行链路
- 阅读代码时应先看什么、后看什么

相关入口文档：

- 如果想先看目录级导航，先读 `../README.zh-CN.md`
- 如果想看拆分后的目录结构参考，读 `../references/ai/directory-structure.md`
- 如果想看运行时模块边界与通信，继续读 `../architecture/模块设计与通信总览.zh-CN.md`
- 如果想看框架化和产品化方向，继续读 `../architecture/框架化与产品化总览.zh-CN.md`

> 本仓库已完成按包拆分（packages/ 布局）。所有能力以包为单位组织，每个包自带 `lania.package.json`；`lan` 入口由 `packages/distribution/core` 的 launcher 提供，按 `known-commands.json` 把命令调度到对应子包二进制。学习任何链路前，先记住这一点。

## 1. 先建立全局心智模型

理解这个仓库最有效的方式，不是从某个命令开始逐行阅读，而是先接受一个稳定的分层模型：

1. `安装层`
   - 位于 `packages/distribution/`（core launcher + cli 总包）与各命令包的 `npm/cmd-<cmd>/`
   - 对最终用户暴露 `lan` 命令
   - launcher 负责选择正确的子包二进制，并让 Rust 宿主能够找到 Node Bridge payload
2. `宿主层`
   - 位于 `packages/foundation/host-core`、`packages/foundation/command` 与 `packages/foundation/capabilities/rust/`
   - 负责命令注册、参数解析、工作流编排、日志、进度、交互、任务、Git、文件系统、统一输出
3. `Bridge 层`
   - 核位于 `packages/foundation/bridge-runtime/ts`，切片位于 `packages/foundation/bridge-slices/*`
   - 负责配置加载、插件解析、安全校验、模板运行时、编译器与 Lint 等 JS 生态能力
4. `资产层`
   - 位于 `packages/foundation/templates`
   - 负责模板元数据、模板问题、模板依赖和渲染资产

如果只记一句话，可以记成：

- Launcher 决定一条命令由哪个子包二进制执行。
- Rust 决定命令如何被执行。
- Node 决定 JS 能力如何被加载与调用。
- npm 决定 CLI 如何被安装与启动。

## 2. 顶层目录应该如何理解

```text
lania-cli-v2/
  docs/
  packages/
    commands/       # 11 个内置命令子包
    distribution/   # core launcher 与 cli 总包
    foundation/     # bridge、模板、宿主核心、能力 crate 等基础层
    products/       # runex / bridge-hub / agent
    runtime/        # lan-runtime 项目级动态命令宿主
    schemas/        # lania.package.json schema
  scripts/
  tests/
```

### `packages/commands/<cmd>/`

每个内置命令一个子包，共 11 个：`dev`、`build`、`lint`、`create`、`add`、`release`、`git`、`tools`、`env`、`pi`、`config`。每个子包含：

- `rust/`：命令插件 crate
- `npm/cmd-<cmd>/` 与平台二进制 `cmd-<cmd>-<platform>/`
- `describes/`：命令描述资产
- `lania.package.json`：包 manifest（含 `configSchema`）

注意：`template` 是 `create` 的二级命令；`product` 是 `dev`/`build` 的共享二级命名空间。

### `packages/foundation/`

基础层主要承担三件事：

- `bridge-runtime/ts`
  - Node 侧 Bridge server 核心（stdio 协议核）
- `bridge-slices/<slice>/ts/src/plugins/`
  - 12 个 TS 切片（compiler、lint、template、product、toolchain、system、lifecycle、config、agent、dynamic-commands、commitizen、commitlint）
  - 插件开发位置：`bridge-slices/<slice>/ts/src/plugins/`
- `templates`
  - 项目模板与 add 模板
  - 模板 catalog、questions、dependencies、runtime

其余关键成员：`host-core`（宿主运行时核心）、`command`（命令抽象）、`command-conformance`（一致性检查）、`package-config`（偏好引擎与共享 config 实现）、`capabilities/rust/`（16 个能力 leaf crate）、`runex-home`、`lania`（作者侧 DSL）。

### `packages/distribution/`

这里不是业务实现层，而是安装入口层：

- `distribution/core`
  - 唯一的 `lan` 入口（launcher），按 `known-commands.json` 调度子包二进制
- `distribution/cli`
  - 总包聚合，只静态注册 `config`

## 3. 三个进程边界要先看清

理解仓库结构之前，先明确运行边界：

1. 用户执行 `lan ...`
2. `packages/distribution/core` 的 launcher 按 `known-commands.json` 选中对应子包二进制
3. 对应命令子包的 Rust 二进制（如 `cmd-dev`）启动，拉起宿主运行时
4. 宿主在需要 JS 能力时拉起 Node Bridge 子进程
5. Rust 与 Node 通过 STDIO JSON 协议通信

这意味着：

- `lan` 只负责入口与调度
- CLI 的主控制流在各命令子包的 Rust 侧
- 配置和 JS 插件的解释权在 Node
- 模板资产与编译器生态不直接进入 Rust

## 4. Rust 宿主内部是如何协作的

建议先把 Rust 基础层理解为一条自上而下的装配链：

1. 命令子包的 `rust/` crate
   - 各命令的二进制入口与命令插件
   - 注册命令、读取全局偏好和项目配置、构建 CLI 并执行
2. `packages/foundation/host-core`
   - 持有命令注册表、handler 注册表、能力容器、HookBus、NodeBridgeClient
   - 负责 `initialize`、`execute_command`、`shutdown`
3. `packages/foundation/command`
   - 将 `CommandSpec` 转成 `clap` 命令树
   - 将匹配结果还原成 `CommandContext`
4. `packages/foundation/capabilities/rust/lania-workflows`
   - 承接复杂命令的真实编排逻辑
5. 能力 crate（`packages/foundation/capabilities/rust/`）
   - 为工作流提供配置、执行、文件、Git、任务、进度和交互能力（`lania-config`、`lania-exec`、`lania-fs`、`lania-git`、`lania-pm`、`lania-progress`、`lania-prompt`、`lania-task`、`lania-logger` 等）

## 5. Node Bridge 内部是如何协作的

Node 侧可以从三个位置入手：

- `packages/foundation/bridge-runtime/ts/src/index.ts`
  - 请求总入口，决定一个 method 应该交给哪个切片
- `packages/foundation/bridge-runtime/ts/src/plugin-registry.ts`
  - 注册内建切片插件，并按 `cwd` 加载项目插件
- `packages/foundation/bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands.ts`
  - 解析项目中的动态命令 manifest

你需要建立的认知是：

- Node Bridge 不是单一插件，而是「核 + 切片」：`bridge-runtime` 是协议核，具体能力按 12 个切片拆在 `bridge-slices/*`
- 它既承载内建能力，也承载项目级扩展能力
- 所有项目插件都会经过策略校验，而不是被直接加载

## 6. 启动阶段发生了什么

当你运行任意命令时，核心启动顺序如下：

1. launcher 找到对应子包二进制并设置 Bridge 目录
2. 子包 Rust 入口注册该命令插件
3. 宿主运行时 `initialize()` 完成插件发现、能力装配和宿主级 Hook 初始化
4. 宿主从当前目录加载 `lan.config.*`
5. 宿主调用 `bootstrap_project_extensions_from_cwd_async()`
6. 如果项目开启了动态命令，就通过 Bridge 请求 `commands.resolveDynamic`
7. 宿主把返回的动态 `CommandSpec` 和 handler 注册进命令树
8. 最后进入 CLI 参数解析与具体命令执行

这一步非常关键，因为它解释了为什么「项目级命令」可以像内置命令一样参与同一棵命令树。

## 7. 内置命令应该分成三类来学

### 第一类：Bridge 型命令

包括：

- `dev`
- `build`
- `lint`

特点：

- Rust 负责参数解析和输出统一
- Node 负责实际调用构建器或 Linter
- 结果经过 Bridge 返回给 Rust

推荐阅读顺序：

1. `packages/commands/dev/rust/src/lib.rs`
2. `packages/commands/build/rust/src/lib.rs`
3. `packages/commands/lint/rust/src/lib.rs`
4. `packages/foundation/bridge-slices/compiler/ts/src/plugins/compiler.ts`
5. `packages/foundation/bridge-slices/lint/ts/src/plugins/lint.ts`

### 第二类：Workflow 型命令

包括：

- `create`
- `add`
- `release`
- `git`
- `config`

特点：

- 主业务编排在 Rust
- 需要 JS 能力时再调用 Bridge
- 更适合从命令子包一路追到 workflow

推荐阅读顺序：

1. 对应的 `packages/commands/<cmd>/rust/src/lib.rs`
2. `packages/foundation/capabilities/rust/lania-workflows/src/`
3. 具体模块目录，例如 `create/`、`release/`、`git/`
4. 相关能力 crate

### 第三类：查询型命令

包括：

- `create template`（`create` 的二级命令）

特点：

- 不承担完整工作流
- 主要用于查询模板 catalog 与模板详情
- 是理解模板运行时边界的最好入口之一

## 8. 每条核心链路是如何流转的

### 8.1 `dev` / `build` / `lint`

这是最典型的「Rust 控制、Node 执行」链路：

1. 命令插件把 CLI 参数整理为 `CommandContext`
2. 宿主运行时 `execute_command()` 定位到对应 handler
3. handler 创建 Bridge request，例如 `compiler.dev`、`compiler.build`、`lint.run`
4. Node Bridge 根据 method 分发到 `compiler` 或 `lint` 切片
5. 切片读取 `lan.config.*`、项目本地依赖和原生工具配置
6. 实际执行工具链，并通过事件流返回日志、诊断和结果
7. Rust 将事件整合为统一输出结构

这条链路最适合用来理解 Rust 与 Node 的职责边界。

### 8.2 `create`

`create` 是一条跨 Rust Workflow 和 Node 模板运行时的复合链路：

1. CLI 参数进入 create 命令插件
2. 参数被转换为工作流输入
3. `CreateWorkflow` 决定是否需要交互补问
4. 通过 Bridge 请求模板列表、模板问题、模板渲染结果
5. Rust 根据返回结果执行冲突检测、文件写入、依赖安装、Git 初始化
6. 最终由宿主输出工作流结果

你可以把它理解为：

- 模板语义在 Node
- 事务控制在 Rust

### 8.3 `add`

`add` 与 `create` 类似，但目标不同：

- `create` 面向「生成一个项目」
- `add` 面向「在现有项目中补充局部资产」

学习时要特别留意一点：

- `add` 使用独立的 add 模板集合，不再复用项目模板

### 8.4 `generate api` / `generate module`

`generate` 命令已删除并归档到本地分支 `archive/generate-command`，重新设计后再启用。此处不再展开其链路细节。

### 8.5 `release`

`release` 是工作流编排能力最强的一条链路：

1. 读取 `lan.config.release`
2. 生成阶段计划
3. 调用 Git、Exec、PackageManager 等能力执行阶段动作
4. 将状态写入 `.lania/release-state.json`
5. 支持 `plan`、`run`、`resume`、`status`

学习重点：

- 阶段建模
- 状态恢复
- 外部命令调用
- 输出与失败处理

### 8.6 `git`

`git` 是最适合理解 Git 服务边界与「先计划后执行」模型的命令：

1. 只读子命令（`status` / `log` / `diff` / `branch list` / `tag list`）直接查询 `lania-git`
2. 写子命令先拼 `git` 参数；`--dry-run` 到此为止
3. 破坏性操作（删除分支/标签、`restore`）缺 `--yes` 时也退化成计划
4. `sync` / `commit` / `push` 复用 `sync` 内核：推断或补问 `remote`、`branch`、`message`，再执行 `add`、`commit`、`push`

阅读时重点关注：

- `packages/foundation/capabilities/rust/lania-git`
- `packages/foundation/capabilities/rust/lania-exec`
- `packages/foundation/capabilities/rust/lania-workflows/src/git/` 与 `sync/plan.rs`
- prompt 补问逻辑

## 9. 自定义指令链路要如何理解

这是当前项目与传统 CLI 最大的差异点之一。

### 9.1 配置入口

项目首先需要在 `lan.config.*` 中开启：

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
};
```

如果项目要调用插件方法、控制 manifest 扫描位置或启用全局 Hook，推荐从下面这个版本开始：

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
  pluginAllowlist: ['./lania.plugin.js'],
  pluginMethodAllowlist: ['commands.deploy', 'hooks.audit'],
};
```

随后可以通过以下位置声明运行时命令：

- `lania.schemas.ts`
- `lania.schemas.js`
- `lania.schemas.cjs`
- `.lania/schemas/` 下的 JSON、YAML、TS、JS 文件

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
          options: [{ long: 'endpoint', valueKind: 'string', help: 'Endpoint', required: true }],
          prompt: [
            {
              field: 'endpoint',
              message: 'Endpoint?',
              kind: 'input',
              whenMissing: ['endpoint'],
            },
          ],
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

如果 handler 要调用项目插件方法：

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'deploy',
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

### 9.2 注册阶段

启动时执行以下步骤：

1. Rust 宿主发现项目开启了动态命令
2. 宿主调用 Node Bridge 的 `commands.resolveDynamic`
3. Node 扫描 manifest，解析 `runtimeCommands[].commands`
4. Node 生成命令规格与 handler 注册信息
5. Rust 把这些命令注册到现有命令树中

### 9.3 执行阶段

当用户真正执行一个动态命令时：

1. Rust 仍然负责参数解析和统一交互入口
2. 如果 target 声明了需要补问的字段，Rust 会先通过 PromptService 执行交互
3. Rust 把补全后的参数通过 `command.invokeDynamic` 发回 Node Bridge
4. Node 执行 manifest 绑定的本地函数或项目插件方法
5. 结果与事件回传给 Rust，并进入统一输出模型

### 9.4 Hook 链路

Hook 的执行原则是：

- 装配顺序和调度模型在 Rust
- JS handler 的实际执行在 Node

也就是说：

1. 配置文件中的 `hooks` 在 bootstrap 阶段被注册到 Rust HookBus
2. Rust 在 `onInitialize`、`onArgsParsed`、`onInteractionPrompt`、`onSuccess`、`onError` 等阶段触发 Hook
3. 如果 Hook 目标是 JS 插件或 inline handler，Rust 再通过 Bridge 进行转发执行

## 10. 首次阅读建议顺序

如果你第一次读仓库，推荐以下顺序：

1. `README.zh-CN.md`
2. `docs/references/ai/directory-structure.md`
3. `packages/commands/dev/rust/src/lib.rs`（选一条命令的子包入口）
4. `packages/foundation/host-core/src/runtime.rs`
5. `packages/foundation/command/src/parser.rs`
6. `packages/foundation/capabilities/rust/lania-workflows/src/lib.rs`
7. `packages/foundation/bridge-runtime/ts/src/index.ts`
8. `packages/foundation/bridge-runtime/ts/src/plugin-registry.ts`
9. `packages/foundation/bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands.ts`
10. `packages/foundation/templates/src/index.ts`
11. 对应命令的子包与测试

## 11. 如果只想快速学一条命令

### 学 `create`

按这个顺序：

1. `tests/` 与命令子包的 e2e 测试（`tests/phase3_e2e/` 夹具见 `tests/phase3_e2e/common.rs`）
2. `packages/commands/create/rust/src/lib.rs`
3. `packages/foundation/capabilities/rust/lania-workflows/src/create/`
4. `packages/foundation/bridge-slices/template/ts/src/plugins/template.ts`
5. `packages/foundation/templates/src/template-runtime.ts`

### 学 `release`

按这个顺序：

1. `packages/commands/release/rust/src/lib.rs`
2. `packages/foundation/capabilities/rust/lania-workflows/src/release/`
3. `packages/foundation/capabilities/rust/lania-config`
4. `packages/foundation/capabilities/rust/lania-exec`
5. `packages/foundation/capabilities/rust/lania-git`

### 学自定义指令

按这个顺序：

1. `packages/foundation/host-core/src/runtime/config.rs`
2. `packages/foundation/host-core/src/runtime/dynamic.rs`
3. `packages/foundation/bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands.ts`
4. `packages/foundation/bridge-slices/lifecycle/ts/src/plugins/lifecycle.ts`
5. 示例项目中的 `lan.config.*` 与 `lania.schemas.*`

## 12. 阅读时建议反复问的三个问题

每看到一个模块，都建议追问：

1. 这一层的输入是什么？
2. 这一层的输出是什么？
3. 这一层不应该负责什么？

例如：

- `packages/foundation/command` 不负责业务编排
- `lania-workflows` 不负责加载 JS 模块
- `bridge-runtime` 不负责统一 CLI 输出策略
- `templates` 不负责 Git、文件事务和发布状态持久化

## 13. 一个实用建议

第一次接手这个仓库时，不要试图把所有包一次看完。更有效的方法是：

1. 先选一条命令
2. 从 launcher 调度定位到该命令的子包
3. 再沿着 handler 进入 workflow 或 bridge 切片
4. 最后回头补基础层与协议层

这样更容易建立稳定的系统认知，也能更快判断一个问题究竟属于某个命令子包、宿主基础层，还是 Node Bridge 切片。
