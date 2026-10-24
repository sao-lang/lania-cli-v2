# Rust 宿主运行时

<div class="lead">
HostRuntime 是 Lania CLI 的中枢：它持有命令、handler、能力、Hook 和 Node Bridge 客户端，并负责把一次 CLI 调用变成可执行结果。
</div>

## 启动流程

```text
main()
  → HostRuntime::new()
  → register_plugin(EmptyPlugin)
  → register_plugin(Dev/Build/Lint/...)
  → initialize()
  → load_lan_config_snapshot_from_cwd_async()
  → load_preferences()
  → resolve_locale() / output_profile()
  → bootstrap_project_extensions_from_cwd_async()
  → command_specs()
  → build_cli()
  → execute_command()
  → shutdown()
```

## 关键组件

### HostRuntime

职责：

- 注册插件。
- 初始化命令、handler、能力与 Hook。
- 加载项目配置。
- 注册动态命令。
- 执行 `CommandContext`。
- 管理 Node Bridge 生命周期。

### CommandSpec

命令规格是 Rust 与 clap 之间的契约：

```rust
CommandSpec::new("build", "Run build", "command.build")
    .with_options(...)
    .with_subcommands(...)
```

`lania-command` 会把它转换成 clap 命令树，再把匹配结果还原成 `CommandContext`。

### CommandContext

包含：

- `cwd`
- `argv.args`
- `argv.options`
- `handler_id`
- `trace_id`

handler 只依赖 `CommandContext`，因此命令解析与执行逻辑解耦。

### CapabilityContainer

能力容器按需向插件提供：

- Logger
- NodeBridge
- Exec
- Fs
- Git
- Progress
- Prompt
- Task
- Config

插件声明 `requires`，宿主负责装配。

### HookBus

HookBus 允许插件和项目在命令生命周期中插入逻辑：

- `onInitialize`
- `onCommandRegister`
- `onArgsParsed`
- `preRun`
- `postRun`
- `onSuccess`
- `onError`

## 工作流分层

复杂命令不直接写在 handler 里，而是进入 `lania-workflows`：

```text
command handler
  → workflow
  → capability / service
  → bridge 或本地实现
```

例如 `create`：

```text
CreateCommandPlugin
  → CreateWorkflow::run()
  → TemplateCapability
  → Node Bridge template.render
  → Fs / Format / Exec / Git
```

## 反向 Host RPC

Node Bridge 不是单向被调用者。项目插件和动态命令通过 `ctx.tools` 发起的宿主操作，会以 `host_request` 回到 Rust：

```text
ctx.tools.exec.run()
  → host_request host.exec.run
  → Rust HostRpcRouter
  → lania-exec
  → host_response
```

Rust 侧按域路由：

| 域                 | 方法                                                             |
| ------------------ | ---------------------------------------------------------------- |
| `host.exec`        | `run`、`runChecked`、`shell`、`history`                          |
| `host.fs`          | `exists`、`read`、`write`、`mkdirp`、`readdir`、`remove`、`stat` |
| `host.git`         | status、diff、add、commit、push、pull、branch、log 等            |
| `host.pm`          | 包管理器检测、脚本探测、安装与运行                               |
| `host.runtime`     | 脚本运行时识别、命令计划与执行                                   |
| `host.log`         | 日志写入、读取、清空、ASCII 渲染                                 |
| `host.tasks`       | 任务注册、启动、完成、失败                                       |
| `host.progress`    | 进度开始、更新、结束                                             |
| `host.interaction` | 交互提问与默认值                                                 |

安全边界由两层组成：Node 侧 `__toolsPolicy` 先做能力过滤，Rust 侧再对 `exec`、`fs` 等高风险域做最终校验。详见 [Bridge 通信协议](/architecture/bridge-protocol)。

## 服务层

| 服务                | 职责                                                |
| ------------------- | --------------------------------------------------- |
| `lania-exec`        | 子进程、dry-run、超时、取消、Windows 可执行文件解析 |
| `lania-fs`          | 文件计划、冲突检测、写入                            |
| `lania-git`         | status / add / commit / push                        |
| `lania-pm`          | 包管理器检测与命令规划                              |
| `lania-config`      | `lan.config.*` 快照                                 |
| `lania-preferences` | 全局偏好                                            |
| `lania-progress`    | spinner / bar / none                                |
| `lania-prompt`      | 交互补问                                            |
| `lania-task`        | 任务编排                                            |
| `lania-logger`      | 日志渲染                                            |

## 跨平台执行

`lania-exec` 在 Windows 下会：

1. 按 `PATHEXT` 解析 `.exe` / `.cmd` / `.bat`。
2. 跳过不可执行的无扩展名 Unix shim。
3. 对批处理脚本通过 `cmd /C` 启动。

这保证了 `npm`、`pnpm`、`yarn`、`bun` 在 Windows 上的一致性。

## 输出

`main.rs` 把执行结果转换为统一结构：

- 成功：stdout 输出结果 JSON（按 profile 渲染）。
- 失败：stderr 输出友好错误，stdout 输出错误 JSON。
- 退出码表达成功/失败。

## 下一步

- [Node Bridge](/architecture/node-bridge)
- [插件与 Hook 系统](/architecture/plugin-system)
- [整体架构](/architecture/overview)
