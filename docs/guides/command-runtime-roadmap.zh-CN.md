# Lania CLI v2 命令与运行时 Roadmap

这份文档不是目录导航，而是一张“从命令到执行函数”的路线图。目标是把仓库里所有主要功能的执行链路串起来，让你在读代码时能同时回答三个问题：

- 一个命令是从哪里进入系统的
- 它最终落到 Rust workflow、Node bridge，还是本地宿主能力
- 往下该追哪些文件、函数和协议字符串

建议把本文和以下文档配合阅读：

- `learning-map.zh-CN.md`：先建立分层心智模型
- `../architecture/模块设计与通信总览.zh-CN.md`：再看模块边界和通信模型
- `../archive/phase0/node-bridge-protocol.md`：需要看协议细节时再下钻

## 1. 先记住固定主链路

不管执行哪条命令，都会先经过同一条启动主链路：

1. 用户执行 `lan ...`
2. `npm/cli/bin/lan.mjs`
   - 选择平台二进制
   - 注入 `LANIA_NODE_BRIDGE_DIR`、`LANIA_PRODUCT_ROOT`、`LANIA_RUNTIME_MODE`
3. `rust/crates/lania-cli/src/main.rs`
   - `main()`
   - 注册所有 Rust 命令插件
   - `host.initialize().await?`
   - `host.load_lan_config_snapshot_from_cwd_async(...)`
   - `host.bootstrap_project_extensions_from_cwd_async(...)`
   - `lania_command::build_cli(...)`
   - `lania_command::command_context_from_matches(...)`
   - `host.execute_command(&context).await`
4. `rust/crates/lania-host/src/runtime/execute.rs`
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
-> npm wrapper
-> Rust main()
-> HostRuntime.initialize()
-> command_context_from_matches()
-> HostRuntime.execute_command()
-> CommandHandler::execute()
-> 具体执行分支
```

## 2. 四类执行分支

仓库里的功能，最终都可以归到这四类：

| 类型 | 典型命令 | 主控层 | 执行层 |
| --- | --- | --- | --- |
| Bridge 型 | `dev` `build` `lint` `template` `tools list` `product *` | Rust | Node bridge |
| Workflow 型 | `create` `add` `generate api` `generate module` `release` `sync` | Rust | Rust workflow，必要时再调 bridge |
| 本地宿主型 | `tools run` `tools view` `config` `product dev` | Rust | Rust 本地服务 |
| 动态命令型 | 项目通过 `lan.config.*` / `lania.schemas.*` 注入的命令 | Rust | 启动时 Node 解析，运行时再回调 Node |

理解命令时，先判断它属于哪类，再往下追，不要把所有分支混在一起看。

## 3. 命令总表

### 3.1 顶层注册入口

Rust 命令插件统一在 `rust/crates/lania-cli/src/main.rs` 的 `main()` 里注册：

- `DevCommandPlugin`
- `BuildCommandPlugin`
- `LintCommandPlugin`
- `CreateCommandPlugin`
- `AddCommandPlugin`
- `GenerateCommandPlugin`
- `ReleaseCommandPlugin`
- `SyncCommandPlugin`
- `TemplateCommandPlugin`
- `ToolsCommandPlugin`
- `ConfigCommandPlugin`

### 3.2 命令族 -> 落点矩阵

| 命令族 | 入口 crate / 文件 | handler 落点 | 继续追踪 |
| --- | --- | --- | --- |
| `dev` | `rust/crates/lania-plugins-command-dev` | bridge `compiler.dev` | `ts/packages/node-bridge/src/plugins/compiler.ts` |
| `product dev` | `rust/crates/lania-plugins-command-dev` | 本地 watch / re-exec | `watch.rs` |
| `build` | `rust/crates/lania-plugins-command-build` | bridge `compiler.build` | `plugins/compiler.ts` |
| `product build/pack/publish/inspect/doctor` | `rust/crates/lania-plugins-command-build` | bridge `product.*` | `plugins/product.ts` |
| `lint` | `rust/crates/lania-plugins-command-lint` | bridge `lint.run` | `plugins/lint/plugin.ts` |
| `create` | `rust/crates/lania-plugins-command-create` | `CreateWorkflow` | `rust/crates/lania-workflows/src/create/` |
| `add` | `rust/crates/lania-plugins-command-add` | `AddWorkflow` | `rust/crates/lania-workflows/src/create/` |
| `template` | `rust/crates/lania-plugins-command-template` | bridge `template.list` | `plugins/template.ts` |
| `generate api` | `rust/crates/lania-plugins-command-generate` | `GenerateApiWorkflow` | `rust/crates/lania-workflows/src/generate/api.rs` |
| `generate module` | `rust/crates/lania-plugins-command-generate` | `GenerateModuleWorkflow` | `rust/crates/lania-workflows/src/generate/module.rs` |
| `product generate` | `rust/crates/lania-plugins-command-generate` | bridge `product.generate` | `plugins/product.ts` |
| `release` | `rust/crates/lania-plugins-command-release` | `ReleaseWorkflow` | `rust/crates/lania-workflows/src/release/` |
| `sync` | `rust/crates/lania-plugins-command-sync` | `SyncWorkflow` | `rust/crates/lania-workflows/src/sync/` |
| `tools` / `tools list` | `rust/crates/lania-plugins-command-tools` | bridge `system.listCommands` | `plugins/system.ts` |
| `tools run` | `rust/crates/lania-plugins-command-tools` | 本地执行 | `run.rs` |
| `tools view` | `rust/crates/lania-plugins-command-tools` | 本地查看 / 系统打开 | `view.rs` |
| `config` | `rust/crates/lania-plugins-command-locale` | 本地 preferences | `lania_preferences` |

## 4. 每条主功能链路怎么串

下面每一节都按同一格式描述：

`命令 -> Rust 入口 -> handler -> workflow / bridge / local -> 关键函数`

### 4.1 `lan dev`

```text
lan dev
-> npm/cli/bin/lan.mjs
-> lania-cli main()
-> lania_command::command_context_from_matches()
-> HostRuntime::execute_command()
-> rust/crates/lania-plugins-command-dev/src/handlers.rs
-> 构造 compiler.dev request
-> ctx.call_bridge(...)
-> ts/packages/node-bridge/src/entry/index.ts::handleExchange()
-> ts/packages/node-bridge/src/plugins/compiler.ts
-> compiler adapter / toolchain
-> bridge result -> CommandExecution
```

关键入口：

- `rust/crates/lania-plugins-command-dev/src/spec.rs`
- `rust/crates/lania-plugins-command-dev/src/registry.rs`
- `rust/crates/lania-plugins-command-dev/src/handlers.rs`
- `rust/crates/lania-plugins-command-dev/src/request.rs`
- `ts/packages/node-bridge/src/plugins/compiler.ts`
- `ts/packages/node-bridge/src/plugins/compiler-adapters/`

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

- `rust/crates/lania-plugins-command-dev/src/watch.rs`

### 4.3 `lan build`

```text
lan build
-> main()
-> HostRuntime::execute_command()
-> rust/crates/lania-plugins-command-build/src/handlers.rs
-> 构造 compiler.build request
-> ctx.call_bridge(...)
-> entry/index.ts::handleExchange()
-> plugins/compiler.ts
-> compiler adapter
```

关键文件：

- `rust/crates/lania-plugins-command-build/src/spec.rs`
- `rust/crates/lania-plugins-command-build/src/registry.rs`
- `rust/crates/lania-plugins-command-build/src/handlers.rs`
- `rust/crates/lania-plugins-command-build/src/request.rs`
- `ts/packages/node-bridge/src/plugins/compiler.ts`

### 4.4 `lan product build` / `pack` / `publish` / `inspect` / `doctor`

这组命令都由 `lania-plugins-command-build` 注册，但桥接到 Node 侧 `product` 插件。

```text
lan product <subcommand>
-> build plugin handler
-> 构造 product.<method> request
-> ctx.call_bridge(...)
-> entry/index.ts::handleExchange()
-> plugins/product.ts
-> plugins/product/handlers/*.ts
-> product snapshot / pack / publish / inspect 逻辑
```

关键文件：

- `rust/crates/lania-plugins-command-build/src/request.rs`
- `ts/packages/node-bridge/src/plugins/product.ts`
- `ts/packages/node-bridge/src/plugins/product/handlers/build.ts`
- `ts/packages/node-bridge/src/plugins/product/handlers/pack.ts`
- `ts/packages/node-bridge/src/plugins/product/handlers/publish.ts`
- `ts/packages/node-bridge/src/plugins/product/handlers/inspect.ts`

### 4.5 `lan lint` / `lan lint check` / `lan lint fix`

```text
lan lint [check|fix]
-> main()
-> HostRuntime::execute_command()
-> rust/crates/lania-plugins-command-lint/src/lib.rs
-> lint_run_request(...)
-> ctx.call_bridge(...)
-> entry/index.ts::handleExchange()
-> plugins/lint/plugin.ts
-> plugins/lint/runners.ts
-> 返回诊断与 exitCode
```

关键文件：

- `rust/crates/lania-plugins-command-lint/src/lib.rs`
- `ts/packages/node-bridge/src/plugins/lint/plugin.ts`
- `ts/packages/node-bridge/src/plugins/lint/runners.ts`

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

- `rust/crates/lania-plugins-command-create/src/lib.rs`
- `rust/crates/lania-workflows/src/create/create_workflow.rs`
- `rust/crates/lania-workflows/src/create/capability.rs`
- `rust/crates/lania-workflows/src/create/helpers/`
- `ts/packages/node-bridge/src/plugins/template.ts`
- `ts/packages/templates/src/`

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

- `rust/crates/lania-plugins-command-add/src/lib.rs`
- `rust/crates/lania-workflows/src/create/add_workflow.rs`
- `rust/crates/lania-workflows/src/create/capability.rs`

### 4.8 `lan template`

```text
lan template [name]
-> main()
-> HostRuntime::execute_command()
-> TemplateCommandPlugin handler
-> bridge request template.list
-> entry/index.ts::handleExchange()
-> plugins/template.ts
-> Rust 侧渲染列表或详情
```

关键文件：

- `rust/crates/lania-plugins-command-template/src/lib.rs`
- `ts/packages/node-bridge/src/plugins/template.ts`

### 4.9 `lan generate api ...`

```text
lan generate api <plan|diff|init>
-> main()
-> HostRuntime::execute_command()
-> GenerateCommandPlugin 分发
-> GenerateApiWorkflow::run()
-> generate_api_support/*
-> plan / diff / apply / manifest update
-> workflow result
```

关键文件：

- `rust/crates/lania-plugins-command-generate/src/lib.rs`
- `rust/crates/lania-plugins-command-generate/src/specs.rs`
- `rust/crates/lania-workflows/src/generate/api.rs`
- `rust/crates/lania-workflows/src/generate_api_support/`

### 4.10 `lan generate module ...`

```text
lan generate module <plan|diff|init|apply>
-> main()
-> HostRuntime::execute_command()
-> GenerateCommandPlugin 分发
-> GenerateModuleWorkflow::run()
-> generate_module_prepare / generate_module_manifest
-> 文件生成、注入、manifest 维护
```

关键文件：

- `rust/crates/lania-workflows/src/generate/module.rs`
- `rust/crates/lania-workflows/src/generate_module_prepare/`
- `rust/crates/lania-workflows/src/generate_module_manifest/`

### 4.11 `lan product generate`

```text
lan product generate
-> main()
-> HostRuntime::execute_command()
-> GenerateCommandPlugin 分发
-> bridge request product.generate
-> entry/index.ts::handleExchange()
-> plugins/product.ts
-> plugins/product/handlers/generate.ts
```

关键文件：

- `rust/crates/lania-plugins-command-generate/src/lib.rs`
- `ts/packages/node-bridge/src/plugins/product/handlers/generate.ts`

### 4.12 `lan release ...`

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

- `rust/crates/lania-plugins-command-release/src/lib.rs`
- `rust/crates/lania-workflows/src/release/mod.rs`
- `rust/crates/lania-workflows/src/release/plan.rs`
- `rust/crates/lania-workflows/src/release/execution.rs`
- `rust/crates/lania-workflows/src/release/state.rs`

### 4.13 `lan sync ...`

```text
lan sync <status|commit|push>
-> main()
-> HostRuntime::execute_command()
-> SyncCommandPlugin handler
-> SyncWorkflow::run()
-> build_sync_plan()
-> git status / prompt / commit message
-> commitizen.run / commitlint.run
-> git commit / git push
```

关键文件：

- `rust/crates/lania-plugins-command-sync/src/lib.rs`
- `rust/crates/lania-workflows/src/sync/mod.rs`
- `ts/packages/node-bridge/src/plugins/commitizen.ts`
- `ts/packages/node-bridge/src/plugins/commitlint.ts`

### 4.14 `lan tools` / `lan tools list`

```text
lan tools [list]
-> main()
-> HostRuntime::execute_command()
-> ToolsCommandHandler::execute()
-> list::execute()
-> bridge request system.listCommands
-> entry/index.ts::handleExchange()
-> plugins/system.ts
-> Rust 侧做 plain / group / unique / namesOnly 变换
```

关键文件：

- `rust/crates/lania-plugins-command-tools/src/lib.rs`
- `rust/crates/lania-plugins-command-tools/src/list.rs`
- `ts/packages/node-bridge/src/plugins/system.ts`

### 4.15 `lan tools run`

```text
lan tools run <file>
-> main()
-> HostRuntime::execute_command()
-> ToolsCommandHandler::execute()
-> run::execute()
-> 识别 shebang / 扩展名
-> 构造本地运行计划
-> exec 服务执行
```

关键文件：

- `rust/crates/lania-plugins-command-tools/src/run.rs`

这是纯宿主路径，不走 bridge，不走 workflow。

### 4.16 `lan tools view`

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

- `rust/crates/lania-plugins-command-tools/src/view.rs`

### 4.17 `lan config` / `config get` / `config set`

```text
lan config ...
-> main()
-> HostRuntime::execute_command()
-> ConfigRootHandler / ConfigGetHandler / ConfigSetHandler
-> lania_preferences::load_preferences / save_preferences
```

关键文件：

- `rust/crates/lania-plugins-command-locale/src/lib.rs`
- `rust/crates/lania-config/src/service.rs`

这组命令也是纯本地路径。

## 5. Node Bridge 总分发图

所有桥接命令最终都会落到：

- `ts/packages/node-bridge/src/entry/stdio.ts`
- `ts/packages/node-bridge/src/entry/index.ts`

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

- `ts/packages/node-bridge/src/core/plugin-registry.ts`

内建插件包括：

- `config`
- `dynamic-commands`
- `lifecycle`
- `compiler`
- `product`
- `lint`
- `system`
- `template`
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
-> plugins/dynamic-commands/index.ts
-> resolveDynamicCommands()
-> Rust 注册 command specs
-> Rust 注册 handlers
-> Rust 注册 target hooks
```

关键文件：

- `rust/crates/lania-host/src/runtime/config/project_extensions_dynamic_commands.rs`
- `ts/packages/node-bridge/src/plugins/dynamic-commands/index.ts`
- `ts/packages/node-bridge/src/plugins/dynamic-commands/parse.ts`

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

- `rust/crates/lania-host/src/runtime/dynamic/command.rs`
- `rust/crates/lania-host/src/runtime/dynamic/prompt.rs`
- `ts/packages/node-bridge/src/plugins/dynamic-commands/execute.ts`
- `ts/packages/node-bridge/src/plugins/dynamic-commands/execute-runner.ts`

## 7. Host RPC 路线图

Host RPC 是反向链路：不是 Rust 调 Node，而是 Node 执行过程中回调 Rust 宿主能力。

```text
Node plugin / dynamic command
-> ts/packages/node-bridge/src/core/host-rpc.ts::hostCall()
-> stdout 写出 host_request
-> rust/crates/lania-node-bridge/src/client/process.rs
-> handle_host_request(...)
-> rust/crates/lania-host/src/runtime/host_rpc.rs::dispatch_host_rpc()
-> host.exec.* / host.fs.* / host.git.* / host.pm.* / ...
-> host_response
-> Node pending promise resolve
```

关键文件：

- `ts/packages/node-bridge/src/core/host-rpc.ts`
- `ts/packages/node-bridge/src/entry/stdio.ts`
- `rust/crates/lania-node-bridge/src/client/process.rs`
- `rust/crates/lania-host/src/runtime/host_rpc.rs`
- `rust/crates/lania-host/src/runtime/host_rpc/`

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
6. 再追到 Node 插件或 Rust 子模块

### 8.1 最常用检索切口

```bash
rg -n "command_context_from_matches|execute_command" rust ts
rg -n "call_bridge|handleExchange|handlePluginRequest" rust ts
rg -n "compiler.dev|compiler.build|lint.run|template.list|system.listCommands|product.generate|product.build|product.pack|product.publish|product.inspect" rust ts
rg -n "CreateWorkflow::run|AddWorkflow::run|GenerateApiWorkflow::run|GenerateModuleWorkflow::run|ReleaseWorkflow::run|SyncWorkflow::run" rust
rg -n "commands.resolveDynamic|command.invokeDynamic|host_request|host_response" rust ts
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

- `rust/crates/lania-host/src/runtime/tests.rs`
- `ts/packages/node-bridge/src/entry/index.test.ts`
- `ts/packages/node-bridge/src/plugins/dynamic-commands-test/`

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
3. `lan generate module`
4. `lan release`
5. 动态命令

这五条链把仓库里最重要的边界几乎都覆盖了。

## 10. 深挖入口

从这一页开始就不再重复展开函数级细节了。`roadmap` 的职责只保留“命令总路由图”和“跨模块链路图”，更细的函数、结构体和中间对象统一下沉到专题索引页。

如果你要继续往下追，直接用这三页：

- `create`：[create-symbol-index.zh-CN.md](file:///Users/bytedance/Desktop/files/projects/self/lania-zip/lania-cli-v2/docs/guides/create-symbol-index.zh-CN.md)
- `generate module`：[generate-module-symbol-index.zh-CN.md](file:///Users/bytedance/Desktop/files/projects/self/lania-zip/lania-cli-v2/docs/guides/generate-module-symbol-index.zh-CN.md)
- `release`：[release-symbol-index.zh-CN.md](file:///Users/bytedance/Desktop/files/projects/self/lania-zip/lania-cli-v2/docs/guides/release-symbol-index.zh-CN.md)

建议的下钻顺序：

1. `create`
   - 先看 `TemplateCapability`
   - 再看 `prompt_state`、`template_runtime_options` 和 `write_files_with_hooks(...)`
2. `generate module`
   - 先看 `PreparedGenerateModulePlan`
   - 再看 `render_module_entry(...)` 和 `apply_contract_generation(...)`
3. `release`
   - 先看 `ReleasePlan` 和 `ReleaseStateSnapshot`
   - 再看 `stage_commands(...)` 和 `execute_release_plan(...)`

这些索引页固定收录：

- 关键结构体
- 关键字段
- 关键函数
- 上游调用方
- 下游被调用方
- 检索切口
