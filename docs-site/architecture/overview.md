# 整体架构

<div class="lead">
Lania CLI 的核心设计是“Rust 宿主 + Node Bridge”的双运行时分层。Rust 掌握命令树与执行编排，Node 掌握 JavaScript 生态与动态扩展。
</div>

## 三个进程边界

```text
┌──────────────────────────────────────────────┐
│ npm/cli/bin/lan.mjs                          │
│ - 选择平台二进制                             │
│ - 注入 LANIA_NODE_BRIDGE_DIR                  │
└──────────────────────┬───────────────────────┘
                       │ spawn
                       ▼
┌──────────────────────────────────────────────┐
│ Rust 宿主 lania-cli                          │
│ - HostRuntime                                │
│ - 命令注册表 / handler 注册表                 │
│ - Capability / HookBus                       │
│ - Workflow / 宿主服务                        │
└──────────────────────┬───────────────────────┘
                       │ STDIO JSON
                       ▼
┌──────────────────────────────────────────────┐
│ Node Bridge                                  │
│ - 配置加载                                   │
│ - 插件注册                                   │
│ - compiler / lint / template / dynamic       │
│ - 项目插件                                   │
└──────────────────────────────────────────────┘
```

## 分层职责

| 层        | 目录                      | 关键职责                                 |
| --------- | ------------------------- | ---------------------------------------- |
| 安装层    | `npm/`                    | `lan` 包装器、平台包、Bridge 路径注入    |
| 宿主层    | `rust/`                   | 命令树、参数解析、工作流、服务、统一输出 |
| Bridge 层 | `ts/packages/node-bridge` | 配置、插件、编译、Lint、模板、动态命令   |
| 资产层    | `ts/packages/templates`   | 模板目录、add 模板、EJS 渲染             |

## 请求流转

以 `lan build` 为例：

1. `lan.mjs` 启动 Rust 二进制。
2. `HostRuntime::initialize()` 注册内置插件。
3. 读取 `lan.config.*` 与全局偏好。
4. Node Bridge 启动并注册插件。
5. Rust 解析 `build` 命令，生成 `CommandContext`。
6. `BuildCommandPlugin` 构造 `compiler.build` bridge 请求。
7. Node Bridge 分发到 compiler 插件。
8. 事件通过 Bridge 回到 Rust，由 Host 渲染并汇总结果。

## Rust 侧模块

### 入口与总控

- `lania-cli`：二进制入口、插件装配、输出渲染。

### 宿主运行时

- `lania-host`：HostRuntime、能力容器、HookBus、命令执行。
- `lania-command`：CommandSpec → clap → CommandContext。
- `lania-hooks`：Hook 定义与调度。

### 工作流

- `lania-workflows`：create / add / generate / release / sync 等编排。

### 基础设施

- `lania-config`：配置快照。
- `lania-exec`：子进程执行、dry-run、超时、取消。
- `lania-fs`：文件计划与写入事务。
- `lania-git`：Git 操作。
- `lania-pm`：包管理器识别与命令规划。
- `lania-progress`：进度渲染。
- `lania-prompt`：交互补问。
- `lania-task`：任务调度。
- `lania-logger`：日志渲染。

### 命令插件

`lania-plugins-command-*` 按命令族拆分。

## Node 侧模块

- `src/index.ts`：Bridge 请求入口。
- `src/plugin-registry.ts`：内置插件与项目插件注册。
- `src/plugins/compiler.ts`：Vite / Webpack / Rollup 适配器。
- `src/plugins/lint.ts`：ESLint / Prettier / Stylelint / Textlint。
- `src/plugins/template.ts`：模板查询与渲染。
- `src/plugins/dynamic-commands.ts`：动态命令解析。
- `src/plugins/product.ts`：产品构建、打包、发布。

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
