# Lania CLI v2 命令与运行时 Roadmap

这份文档不是目录导航，而是一张“从命令到执行函数”的路线图。目标是把仓库里所有主要功能的执行链路串起来，让你在读代码时能同时回答三个问题：

- 一个命令是从哪里进入系统的
- 它最终落到 Rust workflow、Node bridge，还是本地宿主能力
- 往下该追哪些文件、函数和协议字符串

建议把本文和以下文档配合阅读：

- `learning-map.zh-CN.md`：先建立分层心智模型
- `../architecture/模块设计与通信总览.zh-CN.md`：再看模块边界和通信模型
- `../archive/phase0/node-bridge-protocol.md`：需要看协议细节时再下钻

> 仓库已按包拆分（packages/ 布局）。本文所有路径均为拆分后路径；`generate` 命令已删除并归档到本地分支 `archive/generate-command`，重新设计后再启用。

## 1. 先记住固定主链路

不管执行哪条命令，都会先经过同一条启动主链路：

1. 用户执行 `lan ...`
2. `packages/distribution/core` 的 launcher
   - 按 `known-commands.json` 选中对应子包二进制
   - 注入 `LANIA_NODE_BRIDGE_DIR`、`LANIA_PRODUCT_ROOT`、`LANIA_RUNTIME_MODE`
3. `packages/distribution/cli/rust/lania-cli/src/main.rs`
   - `main()`
   - 注册 Rust 命令插件
   - `host.initialize().await?`
   - `host.load_lan_config_snapshot_from_cwd_async(...)`
   - `host.bootstrap_project_extensions_from_cwd_async(...)`
   - `lania_command::build_cli(...)`
   - `lania_command::command_context_from_matches(...)`
   - `host.execute_command(&context).await`
4. `packages/distribution/cli/rust/lania-host/src/runtime.rs`
   - `HostRuntime::execute_command()`
   - 按 `handler_id` 取 handler
   - 触发 `onCommandPreInit` / `onArgsParsed`
   - 组装 `CommandExecutionContext`
   - 调 `handler.execute(...)`
5. handler 再决定后续分支
   - Rust workflow
   - Node bridge
   - 本地宿主能力
   - 动态命令

可以把它记成：

```text
lan
-> launcher (packages/distribution/core)
-> 子包二进制 Rust main()
-> HostRuntime.initialize()
-> command_context_from_matches()
-> HostRuntime.execute_command()
-> CommandHandler::execute()
-> 具体执行分支
```

## 2. 四类执行分支

仓库里的功能，最终都可以归到这四类：

| 类型        | 典型命令                                                 | 主控层 | 执行层                              |
| ----------- | -------------------------------------------------------- | ------ | ----------------------------------- |
| Bridge 型   | `dev` `build` `lint` `template` `tools list` `product *` | Rust   | Node bridge                         |
| Workflow 型 | `create` `add` `release` `git`                           | Rust   | Rust workflow，必要时再调 bridge    |
| 本地宿主型  | `tools run` `tools view` `config` `product dev`          | Rust   | Rust 本地服务                       |
| 动态命令型  | 项目通过 `lan.config.*` / `lania.schemas.*` 注入的命令   | Rust   | 启动时 Node 解析，运行时再回调 Node |

理解命令时，先判断它属于哪类，再往下追，不要把所有分支混在一起看。

## 3. 命令总表

### 3.1 顶层注册入口

内置命令共 11 个：`dev`、`build`、`lint`、`create`、`add`、`release`、`git`、`tools`、`env`、`pi`、`config`。Rust 命令插件统一在 `packages/distribution/cli/rust/lania-cli/src/main.rs` 的 `main()` 里注册：

- `DevCommandPlugin`
- `BuildCommandPlugin`
- `LintCommandPlugin`
- `CreateCommandPlugin`
- `AddCommandPlugin`
- `ReleaseCommandPlugin`
- `GitCommandPlugin`
- `ToolsCommandPlugin`
- `EnvCommandPlugin`
- `PiCommandPlugin`
- `ConfigCommandPlugin`

`template` 是 `create` 的二级命令；`generate` 已删除并归档（分支 `archive/generate-command`）。

### 3.2 命令族 -> 落点矩阵

| 命令族                                      | 入口 crate / 文件                                             | handler 落点                 | 继续追踪                                                                      |
| ------------------------------------------- | ------------------------------------------------------------- | ---------------------------- | ----------------------------------------------------------------------------- |
| `dev`                                       | `packages/commands/dev/rust`                                  | bridge `compiler.dev`        | `bridge-slices/compiler/ts/src/plugins/compiler.ts`                           |
| `product dev`                               | `packages/commands/dev/rust`                                  | 本地 watch / re-exec         | `packages/commands/dev/rust/src/watch.rs`                                     |
| `build`                                     | `packages/commands/build/rust`                                | bridge `compiler.build`      | `bridge-slices/compiler/ts/src/plugins/compiler.ts`                           |
| `product build/pack/publish/inspect/doctor` | `packages/commands/build/rust`                                | bridge `product.*`           | `bridge-slices/product/ts/src/plugins/product.ts`                             |
| `lint`                                      | `packages/commands/lint/rust`                                 | bridge `lint.run`            | `bridge-slices/lint/ts/src/plugins/lint/`                                     |
| `create`                                    | `packages/commands/create/rust`                               | `CreateWorkflow`             | `capabilities/rust/lania-workflows/src/create/`                               |
| `add`                                       | `packages/commands/add/rust`                                  | `AddWorkflow`                | `capabilities/rust/lania-workflows/src/create/`                               |
| `template`（`create` 二级命令）             | `packages/commands/create/rust`                               | bridge `template.list`       | `bridge-slices/template/ts/src/plugins/template.ts`                           |
| `generate api`                              | 已归档（`archive/generate-command` 分支）                     | —                            | 重新设计后再启用                                                              |
| `generate module`                           | 已归档（`archive/generate-command` 分支）                     | —                            | 重新设计后再启用                                                              |
| `release`                                   | `packages/commands/release/rust`                              | `ReleaseWorkflow`            | `capabilities/rust/lania-workflows/src/release.rs`                            |
| `git`                                       | `packages/commands/git/rust`                                  | `GitWorkflow`                | `capabilities/rust/lania-workflows/src/git/`                                  |
| `git sync` / `commit` / `push`              | `packages/commands/git/rust`                                  | `SyncWorkflow`               | `capabilities/rust/lania-workflows/src/sync/plan.rs`                          |
| `tools` / `tools list`                      | `packages/commands/tools/rust`                                | bridge `system.listCommands` | `bridge-slices/system/ts/src/plugins/system.ts`                               |
| `tools run`                                 | `packages/commands/tools/rust`                                | 本地执行                     | `packages/commands/tools/rust/src/run.rs`                                     |
| `tools view`                                | `packages/commands/tools/rust`                                | 本地查看 / 系统打开          | `packages/commands/tools/rust/src/view.rs`                                    |
| `config`                                    | `packages/distribution/cli/rust/lania-plugins-command-locale` | 本地 preferences             | `capabilities/rust/lania-preferences` 与 `packages/foundation/package-config` |

## 4. 每条主功能链路怎么串

下面每一节都按同一格式描述：

`命令 -> Rust 入口 -> handler -> workflow / bridge / local -> 关键函数`

### 4.1 `lan dev`

```text
lan dev
-> launcher 选中 cmd-dev 二进制
-> lania-cli main()
-> lania_command::command_context_from_matches()
-> HostRuntime::execute_command()
-> packages/commands/dev/rust/src/handlers.rs
-> 构造 compiler.dev request
-> ctx.call_bridge(...)
-> bridge-runtime ts/src/entry/index.ts::handleExchange()
-> bridge-slices/compiler/ts/src/plugins/compiler.ts
-> compiler adapter / toolchain
-> bridge result -> CommandExecution
```

关键入口：

- `packages/commands/dev/rust/src/spec.rs`
- `packages/commands/dev/rust/src/registry.rs`
- `packages/commands/dev/rust/src/handlers.rs`
- `packages/commands/dev/rust/src/request.rs`
- `packages/foundation/bridge-slices/compiler/ts/src/plugins/compiler.ts`
- `packages/foundation/bridge-slices/compiler/ts/src/plugins/compiler-adapters/`

你在函数级继续追踪时，优先找：

- `compiler.dev`
- `handleExchange`
- `handlePluginRequest`

### 4.2 `lan product dev`

这条链不是 bridge，也不是 workflow，而是 Rust 本地 watch 链路。

```text
lan product dev
-> main()
-> HostRuntime::execute_command()
-> dev handler
-> watch.rs
-> resolve_product_dev_options()
-> run_product_dev_once() / run_product_dev_watch()
-> 本地 re-exec / watch
```

关键文件：

- `packages/commands/dev/rust/src/watch.rs`

### 4.3 `lan build`

```text
lan build
-> launcher 选中 cmd-build 二进制
-> main()
-> HostRuntime::execute_command()
-> packages/commands/build/rust/src/handlers.rs
-> 构造 compiler.build request
-> ctx.call_bridge(...)
-> entry/index.ts::handleExchange()
-> bridge-slices/compiler/ts/src/plugins/compiler.ts
-> compiler adapter
```

关键文件：

- `packages/commands/build/rust/src/spec.rs`
- `packages/commands/build/rust/src/registry.rs`
- `packages/commands/build/rust/src/handlers.rs`
- `packages/commands/build/rust/src/request.rs`
- `packages/foundation/bridge-slices/compiler/ts/src/plugins/compiler.ts`

### 4.4 `lan product build` / `pack` / `publish` / `inspect` / `doctor`

这组命令都由 build 命令包注册，但桥接到 Node 侧 `product` 切片。

```text
lan product <subcommand>
-> build plugin handler
-> 构造 product.<method> request
-> ctx.call_bridge(...)
-> entry/index.ts::handleExchange()
-> bridge-slices/product/ts/src/plugins/product.ts
-> bridge-slices/product/ts/src/plugins/product/handlers/*.ts
-> product snapshot / pack / publish / inspect 逻辑
```

关键文件：

- `packages/commands/build/rust/src/request.rs`
- `packages/foundation/bridge-slices/product/ts/src/plugins/product.ts`
- `packages/foundation/bridge-slices/product/ts/src/plugins/product/handlers/build.ts`
- `packages/foundation/bridge-slices/product/ts/src/plugins/product/handlers/pack.ts`
- `packages/foundation/bridge-slices/product/ts/src/plugins/product/handlers/publish.ts`
- `packages/foundation/bridge-slices/product/ts/src/plugins/product/handlers/inspect.ts`

### 4.5 `lan lint` / `lan lint check` / `lan lint fix`

```text
lan lint [check|fix]
-> main()
-> HostRuntime::execute_command()
-> packages/commands/lint/rust/src/lib.rs
-> lint_run_request(...)
-> ctx.call_bridge(...)
-> entry/index.ts::handleExchange()
-> bridge-slices/lint/ts/src/plugins/lint/
-> 返回诊断与 exitCode
```

关键文件：

- `packages/commands/lint/rust/src/lib.rs`
- `packages/foundation/bridge-slices/lint/ts/src/plugins/lint/`

### 4.6 `lan create`

这条链是最典型的“Rust 编排，Node 提供模板能力”。

```text
lan create
-> main()
-> HostRuntime::execute_command()
-> CreateCommandPlugin handler
-> CreateWorkflow::run()
-> create/capability.rs
   -> template.list
   -> template.getQuestions
   -> template.getDependencies
   -> template.getOutputTasks
   -> template.render
-> Rust 侧做 format / conflict check / fs write / install / git init
-> workflow result
```

关键文件：

- `packages/commands/create/rust/src/lib.rs`
- `packages/foundation/capabilities/rust/lania-workflows/src/create/create_workflow.rs`
- `packages/foundation/capabilities/rust/lania-workflows/src/create/capability.rs`
- `packages/foundation/capabilities/rust/lania-workflows/src/create/helpers/`
- `packages/foundation/bridge-slices/template/ts/src/plugins/template.ts`
- `packages/foundation/templates/src/`

继续追时，重点看：

- `CreateWorkflow::run`
- `CreateTemplateCapability`
- `template.render`

### 4.7 `lan add`

```text
lan add
-> main()
-> HostRuntime::execute_command()
-> AddCommandPlugin handler
-> AddWorkflow::run()
-> create/capability.rs::addTemplate.render
-> Rust 侧做上下文装载、冲突检查、format、写盘、hooks
```

关键文件：

- `packages/commands/add/rust/src/lib.rs`
- `packages/foundation/capabilities/rust/lania-workflows/src/create/add_workflow.rs`
- `packages/foundation/capabilities/rust/lania-workflows/src/create/capability.rs`

### 4.8 `lan create template ...`

`template` 是 `create` 的二级命令。

```text
lan create template [name]
-> main()
-> HostRuntime::execute_command()
-> create 命令包的 template 子命令 handler
-> bridge request template.list
-> entry/index.ts::handleExchange()
-> bridge-slices/template/ts/src/plugins/template.ts
-> Rust 侧渲染列表或详情
```

关键文件：

- `packages/commands/create/rust/src/lib.rs`
- `packages/foundation/bridge-slices/template/ts/src/plugins/template.ts`

### 4.9 `lan generate api ...` / `lan generate module ...`（已归档）

`generate` 命令已删除，相关实现归档到本地分支 `archive/generate-command`，重新设计后再启用。此节仅保留路线占位，链路细节随归档分支封存。

### 4.10 `lan release ...`

```text
lan release <plan|run|resume|status>
-> main()
-> HostRuntime::execute_command()
-> ReleaseCommandPlugin handler
-> ReleaseWorkflow::run()
-> build_release_plan()
-> state 持久化
-> execute_release_plan() / resume / status
-> git / pm / exec / tasks / progress
```

关键文件：

- `packages/commands/release/rust/src/lib.rs`
- `packages/foundation/capabilities/rust/lania-workflows/src/release.rs`

### 4.11 `lan git ...`

```text
lan git <status|log|diff|branch|tag|add|restore|sync|commit|push>
-> main()
-> HostRuntime::execute_command()
-> GitCommandHandler::execute()
-> GitWorkflow::run()
-> 只读：git status / log / diff / branch list / tag list
-> 写操作：先算 git 参数，--dry-run 只输出计划
-> sync/commit/push：复用 build_sync_plan()
-> git status / prompt / commit message
-> commitizen.run / commitlint.run
-> git commit / git push
```

关键文件：

- `packages/commands/git/rust/src/lib.rs`
- `packages/foundation/capabilities/rust/lania-workflows/src/git/mod.rs`
- `packages/foundation/capabilities/rust/lania-workflows/src/sync/plan.rs`（sync/commit/push 内核）
- `packages/foundation/bridge-slices/commitizen/ts/src/plugins/commitizen.ts`
- `packages/foundation/bridge-slices/commitlint/ts/src/plugins/commitlint.ts`

### 4.12 `lan tools` / `lan tools list`

```text
lan tools [list]
-> main()
-> HostRuntime::execute_command()
-> ToolsCommandHandler::execute()
-> list::execute()
-> bridge request system.listCommands
-> entry/index.ts::handleExchange()
-> bridge-slices/system/ts/src/plugins/system.ts
-> Rust 侧做 plain / group / unique / namesOnly 变换
```

关键文件：

- `packages/commands/tools/rust/src/lib.rs`
- `packages/commands/tools/rust/src/list.rs`
- `packages/foundation/bridge-slices/system/ts/src/plugins/system.ts`

### 4.13 `lan tools run`

```text
lan tools run <file>
-> main()
-> HostRuntime::execute_command()
-> ToolsCommandHandler::execute()
-> run::execute()
-> ctx.runtime().run(...)
-> lania-runtime 识别 shebang / 扩展名
-> 构造运行计划 + 可选 prepare
-> lania-exec 执行并清理临时产物
```

关键文件：

- `packages/commands/tools/rust/src/run.rs`
- `packages/foundation/capabilities/rust/lania-runtime/src/lib.rs`
- `packages/foundation/capabilities/rust/lania-runtime/src/registry.rs`

这是纯宿主路径，不走 bridge，不走 workflow。运行时识别/计划/执行统一由 `lania-runtime` 提供，`host.runtime.*` 与 `ctx.tools.runtime` 复用同一实现。

### 4.14 `lan tools view`

```text
lan tools view <path>
-> main()
-> HostRuntime::execute_command()
-> ToolsCommandHandler::execute()
-> view::execute()
-> 文本直接读取
-> 媒体文件走系统默认打开
```

关键文件：

- `packages/commands/tools/rust/src/view.rs`

### 4.15 `lan config` / `config get` / `config set`

```text
lan config ...
-> main()
-> HostRuntime::execute_command()
-> ConfigRootHandler / ConfigGetHandler / ConfigSetHandler
-> lania_preferences::load_preferences / save_preferences
```

关键文件：

- `packages/distribution/cli/rust/lania-plugins-command-locale/src/lib.rs`
- `packages/foundation/capabilities/rust/lania-config/src/service.rs`
- `packages/foundation/package-config/`（lania-package-config 偏好引擎与 lania-command-config 共享 config 实现）

这组命令也是纯本地路径。各命令子包自带 `config` 二级命令（bare/get/set/init/doctor/sync + `--global`），总包额外提供 `lan config packages/apply` 与 `--package` 归一化。

## 5. Node Bridge 总分发图

Node Bridge 是「核 + 切片」结构：协议核在 `bridge-runtime`，能力按 12 个切片拆在 `bridge-slices/*`。所有桥接命令最终都会落到：

- `packages/foundation/bridge-runtime/ts/src/entry/stdio.ts`
- `packages/foundation/bridge-runtime/ts/src/entry/index.ts`

关键函数是：

- `createHandshakeResponse()`
- `handleRequest()`
- `handleExchange()`

其中 `handleExchange()` 是最重要的总分发点：

```text
bridge request
-> handleBuiltinRequest(...)
-> handlePluginRequest(...)
-> plugin.handle(method, params, context)
```

插件注册中心在：

- `packages/foundation/bridge-runtime/ts/src/core/plugin-registry.ts`

内建切片包括（`packages/foundation/bridge-slices/`）：

- `compiler`
- `lint`
- `template`
- `product`
- `toolchain`
- `system`
- `lifecycle`
- `config`
- `agent`
- `dynamic-commands`
- `commitizen`
- `commitlint`

## 6. Dynamic Commands 路线图

动态命令不是某一个固定命令，而是一条独立机制。

### 6.1 启动时的 resolve 链路

```text
main()
-> host.bootstrap_project_extensions_from_cwd_async(...)
-> runtime/config/project_extensions_dynamic_commands.rs::bootstrap_dynamic_commands()
-> node bridge request "commands.resolveDynamic"
-> bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands/index.ts
-> resolveDynamicCommands()
-> Rust 注册 command specs
-> Rust 注册 handlers
-> Rust 注册 target hooks
```

关键文件：

- `packages/distribution/cli/rust/lania-host/src/runtime/config/project_extensions_dynamic_commands.rs`
- `packages/foundation/bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands/index.ts`
- `packages/foundation/bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands/parse.ts`

### 6.2 运行时的 invoke 链路

```text
用户执行动态命令
-> HostRuntime::execute_command()
-> BridgeCommandHandler::execute()
-> maybe_prompt_dynamic_command(...)
-> node bridge request "command.invokeDynamic"
-> invokeDynamicCommand()
-> runDynamicExecutor()
-> local executor 或 manifest handler
```

关键文件：

- `packages/distribution/cli/rust/lania-host/src/runtime/dynamic/command.rs`
- `packages/distribution/cli/rust/lania-host/src/runtime/dynamic/prompt.rs`
- `packages/foundation/bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands/execute.ts`
- `packages/foundation/bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands/execute-runner.ts`

## 7. Host RPC 路线图

Host RPC 是反向链路：不是 Rust 调 Node，而是 Node 执行过程中回调 Rust 宿主能力。

```text
Node slice / dynamic command
-> bridge-runtime ts/src/core/host-rpc.ts::hostCall()
-> stdout 写出 host_request
-> lania-node-bridge client process
-> handle_host_request(...)
-> lania-host runtime host_rpc dispatch
-> host.exec.* / host.fs.* / host.git.* / host.pm.* / ...
-> host_response
-> Node pending promise resolve
```

关键文件：

- `packages/foundation/bridge-runtime/ts/src/core/host-rpc.ts`
- `packages/foundation/bridge-runtime/ts/src/entry/stdio.ts`
- `packages/products/bridge-hub/adaptors/ts/lania-node-bridge/src/client/`
- `packages/distribution/cli/rust/lania-host/src/runtime/`

常见域包括：

- `host.exec.*`
- `host.fs.*`
- `host.git.*`
- `host.pm.*`
- `host.log.*`
- `host.tasks.*`
- `host.progress.*`
- `host.interaction.*`

## 8. 函数级追踪应该怎么做

如果你要从“命令”一直追到“执行函数”，建议固定按下面顺序：

1. 找命令 spec
2. 找 handler 注册
3. 找 handler 的 `execute()`
4. 判断它走 bridge、workflow、local 还是 dynamic
5. 找请求 method 或 workflow `run()`
6. 再追到 Node 切片或 Rust 子模块

### 8.1 最常用检索切口

```bash
rg -n "command_context_from_matches|execute_command" packages
rg -n "call_bridge|handleExchange|handlePluginRequest" packages
rg -n "compiler.dev|compiler.build|lint.run|template.list|system.listCommands|product.build|product.pack|product.publish|product.inspect" packages
rg -n "CreateWorkflow::run|AddWorkflow::run|ReleaseWorkflow::run|SyncWorkflow::run" packages
rg -n "commands.resolveDynamic|command.invokeDynamic|host_request|host_response" packages
```

### 8.2 追变量时的建议

变量级调用关系不要只靠全文搜索。这个仓库大量使用：

- `handler_id`
- `method`
- `target`
- `argv`
- `serde_json::json!(...)`
- TS/Rust 之间的 JSON 协议字段

所以变量追踪建议组合使用：

1. IDE 的 `Go to Definition`
2. IDE 的 `Find References`
3. `rg` 搜协议字段和字符串常量
4. 测试文件看最短调用样例

尤其推荐看这些测试：

- `packages/distribution/cli/rust/lania-host/src/`（宿主运行时测试）
- `packages/foundation/bridge-runtime/ts/src/entry/index.test.ts`
- `packages/foundation/bridge-slices/dynamic-commands/ts/src/plugins/dynamic-commands-test/`

## 9. 推荐阅读顺序

如果你的目标是“把所有功能链路串起来”，建议按这个顺序执行：

1. 本文第 1 节和第 2 节，先记住固定主链路和四类分支
2. 看第 3 节命令总表，先建立“命令 -> 落点”映射
3. 从第 4 节开始，按命令族逐条过一遍
4. 再看第 6 节和第 7 节，把动态命令和 Host RPC 补齐
5. 最后用第 8 节的检索切口追具体函数和变量

如果只想先吃透一条最典型链路，优先顺序是：

1. `lan create`
2. `lan dev`
3. `lan release`
4. `lan git`
5. 动态命令

这五条链把仓库里最重要的边界几乎都覆盖了。

## 10. 深挖入口

从这一页开始就不再重复展开函数级细节了。`roadmap` 的职责只保留“命令总路由图”和“跨模块链路图”，更细的函数、结构体和中间对象统一下沉到专题索引页。

如果你要继续往下追，直接用以下索引页：

- `create`：[create-symbol-index.zh-CN.md](./create-symbol-index.zh-CN.md)
- `release`：[release-symbol-index.zh-CN.md](./release-symbol-index.zh-CN.md)

建议的下钻顺序：

1. `create`
   - 先看 `TemplateCapability`
   - 再看 `prompt_state`、`template_runtime_options` 和 `write_files_with_hooks(...)`
2. `release`
   - 先看 `ReleasePlan` 和 `ReleaseStateSnapshot`
   - 再看 `stage_commands(...)` 和 `execute_release_plan(...)`

这些索引页固定收录：

- 关键结构体
- 关键字段
- 关键函数
- 上游调用方
- 下游被调用方
- 检索切口
