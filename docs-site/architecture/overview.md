# 整体架构

<div class="lead">
Lania CLI 的核心设计是“Rust 宿主 + Node Bridge”的双运行时分层。Rust 掌握命令树与执行编排，Node 掌握 JavaScript 生态与动态扩展。
</div>

## 三个进程边界

```text
┌──────────────────────────────────────────────┐
│ @lania-cli-v2/core bin/lan.mjs（launcher）   │
│ - 按 known-commands.json 静态表调度          │
│ - 解析子包平台二进制 lan-<cmd>               │
│ - 注入 LANIA_NODE_BRIDGE_DIR / SLICES        │
└──────────────────────┬───────────────────────┘
                       │ spawn
                       ▼
┌──────────────────────────────────────────────┐
│ 子包二进制 lan-<cmd>（Rust）                 │
│ - HostRuntime                                │
│ - 命令注册表 / handler 注册表                 │
│ - Capability / HookBus                       │
│ - Workflow / 宿主服务                        │
└──────────────────────┬───────────────────────┘
                       │ STDIO JSON
                       ▼
┌──────────────────────────────────────────────┐
│ Node Bridge（bridge-runtime 核 + 切片）      │
│ - 配置加载                                   │
│ - 插件注册                                   │
│ - compiler / lint / template / dynamic 切片  │
│ - 项目插件                                   │
└──────────────────────────────────────────────┘
```

> 拆分说明：总包 `@lania-cli-v2/cli` 只做聚合，不再静态链接命令；`config` 是唯一
> 由总包二进制直接注册的命令。未安装子包时 launcher 会提示安装或回退到
> `lan-runtime` 的动态命令设施。

## 分层职责（按包）

| 层        | 包                                                       | 关键职责                                                |
| --------- | -------------------------------------------------------- | ------------------------------------------------------- |
| 调度层    | `packages/distribution/core`                             | 唯一 `lan` 入口、known-commands 静态表、子包管理        |
| 命令层    | `packages/commands/*`（11 个）                           | 每命令一个原子子包：spec/handler/describes/configSchema |
| 宿主层    | `packages/distribution/cli/rust/*`、`foundation/command` | HostRuntime、参数解析、本地化、统一输出                 |
| Bridge 层 | `packages/foundation/bridge-runtime` + `bridge-slices/*` | stdio 协议核与 12 个切片（compiler/lint/template/…）    |
| 资产层    | `packages/foundation/templates`                          | 模板目录、add 模板、EJS 渲染                            |
| 产品层    | `packages/products/{runex,bridge-hub,agent}`             | 自管运行时管理器 / 语言中立消息总线 / 编码 agent        |
| 设施层    | `packages/runtime`                                       | 项目级动态命令与脚手架封装命令的执行宿主                |

每包的 `lania.package.json` 的 `description` 字段是一句话职责说明（单一事实源）。

## 请求流转

以 `lan workspace build` 为例：

1. launcher（`@lania-cli-v2/core`）查 `known-commands.json`，解析已安装的
   `@lania-cli-v2/cmd-build` 平台二进制 `lan-build`，剥掉命令名后转发 argv。
2. `lan-build` 内的 `HostRuntime` 装配插件（`bridge_setup` 统一入口完成语言桥注册
   与 bridge-hub provider 注入）。
3. 读取 `lan.config.*` 与全局偏好、包级偏好。
4. Node Bridge 启动（payload 由包内 `lib/bridge` 的核 + 切片组成）并注册插件。
5. Rust 解析 `build` 命令，生成 `CommandContext`。
6. `BuildCommandPlugin` 构造 `compiler.build` bridge 请求。
7. Node Bridge 分发到 compiler 切片。
8. 事件通过 Bridge 回到 Rust，由 Host 渲染并汇总结果。

## Rust 侧模块

### 入口与总控

- `packages/distribution/core/npm/core/bin/lan.mjs`：launcher（唯一 `lan` 入口，Node）。
- `packages/distribution/cli/rust/lania-cli`：总包二进制（只注册 `config`）。
- `packages/distribution/cli/rust/lania-host`：HostRuntime、能力容器、HookBus、
  命令执行、`bridge_setup` 统一装配入口。
- `packages/foundation/command/rust/lania-command`：CommandSpec → clap → CommandContext。
- `packages/foundation/command-conformance`：命令树与本地化覆盖一致性测试。

### 命令子包

- `packages/commands/<cmd>/rust`：`lania-plugins-command-<cmd>`，每包双形态
  （lib API + 独立 `lan-<cmd>` bin），由 `lania-host` 与 `lania-cli-kit` 支撑。

### 工作流

- `packages/foundation/capabilities/rust/lania-workflows`：create / add / release / git 等编排。

### 基础设施

- `packages/foundation/capabilities/rust/`：`lania-config`（配置快照）、`lania-exec`
  （子进程执行）、`lania-fs`（文件事务）、`lania-git`、`lania-pm`、`lania-progress`、
  `lania-prompt`、`lania-task`、`lania-logger`、`lania-preferences` 等 16 个 leaf crate。
- `packages/foundation/package-config/rust/`：包级偏好引擎 + 共享 config 二级命令实现。
- `packages/foundation/cli-kit/rust/`：输出渲染与单命令执行支点。

## Node 侧模块

- `packages/foundation/bridge-runtime/ts/src/`：Bridge 请求入口、插件注册表与切片装配
  （1:1 / hub 双模式）。
- `packages/foundation/bridge-slices/*/ts/src/plugins/`：按能力拆分的切片——
  `compiler`（Vite/Webpack/Rollup）、`lint`（ESLint/Prettier/Stylelint/Textlint）、
  `template`（模板查询与渲染）、`dynamic-commands`（动态命令）、`product`（构建/打包/发布）、
  `toolchain`、`system`、`lifecycle`、`config`、`agent`、`commitizen`、`commitlint`。
- `packages/products/agent/ts/`：pi 编码 agent 引擎。
- `packages/foundation/templates/src/`：模板清单、问答与 EJS 渲染运行时。

## 为什么这样分层

- 命令树与执行安全边界稳定在 Rust。
- JavaScript 生态能力保持原生 Node 实现。
- 模板资产与项目插件可以独立演进。
- 输出、日志、进度和错误处理统一由宿主控制。

## 三个方向的调用链

| 方向        | 载体                             | 典型场景                                                          |
| ----------- | -------------------------------- | ----------------------------------------------------------------- |
| Rust → Node | `request` / `response` / `event` | 编译、Lint、模板渲染、动态命令                                    |
| Node → Rust | `host_request` / `host_response` | `ctx.tools.exec`、`ctx.tools.fs`、`ctx.tools.git`、`ctx.tools.pm` |
| Node 内部   | `ctx.tools.bridge`               | 动态命令调用 `config.*`、`compiler.*`、`lint.*`                   |

`ctx.tools` 不是 Rust 直接注入的全局对象，而是 Node Bridge 在每次插件调用前创建的能力集合；其中 `host`、`exec`、`fs` 等命名空间最终通过反向 Host RPC 回到 Rust 执行。

## 下一步

- [Rust 宿主运行时](/architecture/rust-host)
- [Node Bridge](/architecture/node-bridge)
- [Bridge 通信协议](/architecture/bridge-protocol)
- [命令模型与加载原理](/guide/command-model)
- [插件与 Hook 系统](/architecture/plugin-system)
- [ctx.tools 宿主能力](/guide/host-tools)
