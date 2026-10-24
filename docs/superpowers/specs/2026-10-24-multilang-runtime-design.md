# 多语言运行时扩展技术方案

> 状态：待评审。目标读者：能直接据此开工实现、不必再回头问作者的 builder。
> 修订说明：初稿声称"Rust 侧零改动""create 已语言无关"，经代码核验为错误，已在本版修正（见 §6.6、§6.7、§9）。

## 1. 意图与动机

当前 `lan dev` / `lan build` / `lan create` / `lan add` / `lan template` 五个内置命令的前端绑定**不在 Rust 命令层，而在 Node bridge 的 `compiler` 插件与 `templates` 包**。要让这些命令支持 Python / Go / Rust / Dart，本质是补一个"运行时（runtime）"维度：`dev`/`build` 按项目语言分发到对应工具链，`create`/`add`/`template` 按语言分发到对应模板资产。

本方案只做这件事，不改动既有前端（`web`）行为。

## 2. 目标 / 非目标 / 反目标

### 目标

- `lan dev` / `lan build` 对 `web`（现有）/ `python` / `go` / `rust` / `dart` 五类运行时开箱即用：探测项目语言后分发到对应工具链。
- `lan create` / `lan template` 能看到并渲染 Python / Go / Rust / Dart 项目模板。
- `lan add` 能为 Python / Go / Rust / Dart 生成单文件模块骨架（注意：`add` 的机制是单文件渲染，不是目录脚手架，见 §6.7）。
- 现有前端（`web` 运行时）行为 100% 不变，既有测试不回归。

### 非目标

- 不实现任何语言的构建产物缓存 / 增量编译。
- 不做各语言工具链的安装/升级管理。
- **不做 dev 运行期的实时日志流**：本迭代只保证"启动阶段输出"可见（见 §6.5）。
- 不做 runtime-aware 的依赖安装（`go mod tidy` / `cargo fetch` / `uv sync`）；`create` 仍走现有 `npm/pnpm install` 或 `--skip-install`（见 §6.7）。
- 不把内置命令改成可被任意第三方 DSL 覆盖（那是产品化路线）。
- 不实现 `build --watch` 对非 web runtime 的支持（直接拒绝，见 §6.8）。

### 反目标

- 不把语言逻辑硬编码进 Rust 命令插件：`command-dev` / `command-build` 命令层保持"薄转发"；新增的 Rust 改动只允许在"校验白名单 / spec 选项 / 退出码映射"这类边界处（见 §6.6）。
- 不为 spawn 型工具链引入依赖注入框架或抽象工厂。
- 不给非 web runtime 伪造 `http://host:port` 或假的 dev server 就绪事件。

## 3. 约束

1. **bridge 协议稳定**：Rust ↔ Node 的 `STDIO + JSONL` envelope、method 命名（`compiler.dev`/`compiler.build`/`compiler.stop`）不变。Node 内部 worker IPC 协议可扩展（它不跨 Rust 边界）。
2. **现有测试不回归**：`ts/packages/node-bridge/src/plugins/compiler.test.ts` 的 vite/webpack/rollup 用例全部保持通过。
3. **spawn 型工具链不引 DI**：测试 mock 用"配置注入命令 + Node 脚本 fixture"（§7.1），不引入依赖注入抽象。
4. **跨平台测试**：本机是 win32，测试必须在 win32 与 POSIX 都能跑（§7.1 的 fixture 方案已解决）。

## 4. 现状与前端耦合点（已核验）

```text
lan dev/build -> Rust command plugin (薄转发)
  -> bridge compiler.dev / compiler.build
    -> Node compiler 插件
      -> detectBuildTool()  -> adapter (vite/webpack/rollup)  -> 隔离 worker
```

| 位置                                                      | 现状                                                                                | 问题                             |
| --------------------------------------------------------- | ----------------------------------------------------------------------------------- | -------------------------------- |
| `ts/packages/node-bridge/src/plugins/compiler-shared.ts`  | `type BuildTool = 'vite' \| 'webpack' \| 'rollup'`；`detectBuildTool()` 默认 `vite` | 工具链枚举硬编码前端，无语言探测 |
| `ts/packages/node-bridge/src/plugins/compiler-adapters/`  | 只有 vite/webpack/rollup（进程内 import JS 模块）                                   | 无语言 adapter                   |
| `ts/packages/templates/src/templates/`                    | 只有 spa-vue/spa-react/toolkit/toolkit-monorepo                                     | 只有前端模板                     |
| `ts/packages/templates/src/add-templates/assets/`         | 只有 js/ts 单文件片段                                                               | `add` 只有 JS/TS                 |
| `rust/crates/lania-plugins-command-add/src/lib.rs`        | `--template` 的 `choices: ["v2","v3","rcc",...]` 硬编码                             | clap 拒绝未列出的模板名          |
| `rust/crates/lania-config/src/validate_lan.rs`            | 顶层字段白名单不含 `runtime`/`runtimes`                                             | 写了会报 `UnknownField`          |
| `rust/crates/lania-plugins-command-build/src/handlers.rs` | `complete_bridge(run, EXIT_SUCCESS)` 恒成功                                         | 构建失败仍退出 0                 |

### 可复用的能力

- **spawn 二进制模式**：`ts/packages/node-bridge/src/plugins/lint/shared.ts` 的 `runBinary(cwd, runtimePath, args)`（spawn + 捕获 stdout/stderr/exitCode）。
- **adapter 注册表 + 分发**：`compiler-adapters/index.ts` 的 `getCompilerAdapters()`/`resolveCompilerAdapter()`；`lint/types.ts` 的 `LINT_ADAPTORS` + `isLintAdaptor()` 模式。
- **长任务生命周期**：`workers/compiler-worker.ts` 的 `keepAlive` + `activeCompiler.stop()`；worker 由 `compiler-worker-client.ts` 用 `tsx` 拉起，隔离崩溃。
- **事件消费容错**：`rust/crates/lania-host/src/execution/bridge_helpers/events.rs` 对字段用 `unwrap_or`，缺字段不崩。

## 5. 方案选择

- **方案 A（已选）**：引入 runtime 抽象，`compiler` 降级为 web 特例；非 web 走 spawn 型 adapter。
- **方案 B（否决）**：产品化扩展（动态命令），`lan dev` 仍前端专用，体验割裂。
- **方案 C（否决）**：把语言逻辑塞进 Rust 命令插件，破坏"命令控制权在 Rust、能力扩展权在 Node"边界。

选 A：扩展点（adapter 注册表 + spawn 模式）已存在，改动集中在 Node 侧，Rust 侧只需边界性小改。

## 6. 核心设计

### 6.1 runtime 抽象

```ts
// compiler-shared.ts
export type Runtime = 'web' | 'python' | 'go' | 'rust' | 'dart';
export type BuildTool = 'vite' | 'webpack' | 'rollup'; // 仅 web runtime 内部使用，保留不变
```

`CompilerAdapterContext` 增加 `runtime: Runtime` 字段（该 context 只经 Node 内部 worker IPC 传输，不影响 Rust 协议）。

### 6.2 探测 `detectRuntime()`

新增 `detectRuntime(config, cwd, override?): Promise<Runtime>`，优先级从高到低：

1. **调用参数覆盖**：`lan dev --runtime <rt>` / `lan build --runtime <rt>`（新增选项，见 §6.6）。
2. **配置声明**：`lan.config.runtime`。
3. **特征文件探测**（按顺序扫描 `cwd`，先命中先返回）：
   - `go.mod` → `go`
   - `Cargo.toml` → `rust`
   - `pyproject.toml` → `python`
   - `pubspec.yaml` → `dart`
   - `package.json` → `web`
4. **兜底**：`web`。

> 混合仓库说明：`pyproject.toml`/`go.mod` 等是强信号，排在 `package.json` 前，因为 `package.json` 常作为工具链存在。混合仓库（monorepo）没有单一答案，用 `--runtime` 或 `lan.config.runtime` 显式指定。这是有意的取舍，不做自动 monorepo 分析。

`detectBuildTool()` 保留，语义收窄为"在 web runtime 内选 vite/webpack/rollup"，仅当 `runtime === 'web'` 时调用。

### 6.3 adapter 分层

```
compiler.dev / compiler.build
  -> detectRuntime()
    -> web:  resolveCompilerAdapter(tool)              （进程内 import，现有不变）
    -> 其它: resolveRuntimeAdapter(runtime)            （spawn 外部二进制，新增）
```

新增 `ts/packages/node-bridge/src/plugins/runtime-adapters/`：

```ts
export interface RuntimeAdapter {
  runtime: Runtime;
  handleDev(params, context): Promise<CompilerExecution | null>;
  handleBuild(params, context): Promise<CompilerExecution | null>;
}
export function getRuntimeAdapters(): RuntimeAdapter[];
export function resolveRuntimeAdapter(runtime: Runtime): RuntimeAdapter | null;
```

worker（`workers/compiler-worker.ts` 的 `handleRun`）按 `context.runtime` 分发：`web` 走 `resolveCompilerAdapter`，其它走 `resolveRuntimeAdapter`。这样 spawn 型 adapter 自动复用现有的 worker 隔离、`keepAlive`、`stop()` 生命周期，无需改 `compiler-worker-client.ts`。

spawn 型 adapter 内部复用 `runBinary` 思路：spawn → 捕获 stdout/stderr → 转事件 → 等待退出或 keepAlive。

### 6.4 dev/build 命令映射（可配置）

| runtime | dev 默认（长任务）         | build 默认（短任务）             |
| ------- | -------------------------- | -------------------------------- |
| go      | `go run .`                 | `go build -o bin/app .`          |
| rust    | `cargo run`                | `cargo build --release`          |
| python  | **必须配置**（无通用默认） | `python -m build`                |
| dart    | `dart run`                 | `dart compile exe bin/main.dart` |

**命令必须可配置**（Python dev server 框架太多，Dart 入口不固定）。配置形态：

```js
// lan.config.js
module.exports = {
  runtime: 'python', // 可选；本版需把 runtime/runtimes 加入 Rust 白名单（§6.6）
  runtimes: {
    python: {
      dev: ['python', '-m', 'uvicorn', 'app.main:app', '--reload'],
      build: ['python', '-m', 'build'],
    },
    go: {
      dev: ['go', 'run', '.'],
      build: ['go', 'build', '-o', 'bin/app', '.'],
    },
  },
};
```

读取顺序：`lan.config.runtimes[<runtime>].dev/build` → 表中内置默认。Python dev 无默认，缺配置时**失败**（不伪造成功，见 §6.5）。

占位符：build 命令里的 `{output}` 会被替换为 `--output-dir`（若提供）；无占位符时通过环境变量 `OUTPUT_DIR` 传入。

### 6.5 事件契约、失败语义与实时日志边界

**事件**：现有事件类型（`protocol/events.ts`）全部保留。

- `web`：继续发全套富事件（`dev_url`/`compiler_server_ready`/`compiler_asset`/`build_asset`），不变。
- 非 web：
  - dev：`compiler_start` / `compiler_status` / `log` / `compiler_issue` / `compiler_done`。**不发** `dev_url`/`compiler_server_ready`（除非 dev 命令确实监听 HTTP 且 adapter 显式配置了就绪探测，本版不实现）。
  - build：`compiler_start` / `compiler_status` / `compiler_issue` / `compiler_done`。产物存在时可选发 `compiler_asset`（`file` 填产物路径）。

**失败语义（关键修正）**：

- 非 web adapter 的 `result` 必须包含 `success: boolean`。
- 子进程非零退出 → `success:false` + `compiler_issue(error)` + `compiler_done(success:false)`，且**不返回 `activeCompiler`**。
- 工具链缺失（spawn ENOENT）→ 同上 fail-fast：不伪造 URL、不进入长任务、立即返回失败结果。
- Python dev 缺配置 → 同上 fail-fast。
- `BuildCommandHandler` 改为按 `result.success` 决定退出码：`success === false` → `EXIT_RUNTIME_ERROR`，否则 `EXIT_SUCCESS`。web 结果不带 `success`，默认 true，行为不变。详见 §6.6。

**实时日志边界（明确非目标）**：`compiler-worker-client.ts` 在 worker 返回 `result` 时才把缓冲的 stdout/stderr 并入事件；`activeCompiler` 返回后主进程进入 Ctrl-C 等待。因此本迭代只能保证**启动阶段**输出可见；`go run`/`uvicorn` 启动后的请求日志不会流式转发。若要实时日志，需要新增 worker→bridge 的流式 IPC 消息类型并在 resolve 之后继续发事件，这属于协议扩展，本版不做。

**stdout/stderr 映射**：启动阶段按行处理，stdout → `event.log`(info)，stderr → `compiler_issue`(warning)；非零退出时把最后一段 stderr 提升为 `compiler_issue`(error)。

### 6.6 Rust 侧改动（非零，已修正）

初稿"零改动"是错的。实际需要以下边界性改动：

| 文件                                                     | 改动                                                                                                                    | 原因                                                                                                                                                                                                                                      |
| -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lania-config/src/validate_lan.rs`                       | 白名单 `allowed` 数组加入 `runtime`、`runtimes`                                                                         | 否则用户按文档写配置会收到 `UnknownField` 诊断。**注意：Node 侧 `loadLanConfig`（`ts/packages/node-bridge/src/core/runtime.ts`）直接读原始 `lan.config.*`，不经 Rust snapshot，因此无需改 `normalize.rs`/`models.rs` 做 snapshot 透传。** |
| `lania-plugins-command-dev/src/spec.rs` + `request.rs`   | 新增 `--runtime` 选项并写入 `compiler.dev` params                                                                       | 混合仓库逃生口                                                                                                                                                                                                                            |
| `lania-plugins-command-build/src/spec.rs` + `request.rs` | 新增 `--runtime` 选项并写入 `compiler.build` params                                                                     | 同上                                                                                                                                                                                                                                      |
| `lania-plugins-command-build/src/handlers.rs`            | 退出码按 `run.exchange.response.result["success"]` 映射                                                                 | 构建失败必须非零退出                                                                                                                                                                                                                      |
| `lania-node-bridge/src/client/transport/requests.rs`     | `compiler_dev_request`/`compiler_build_with_options_request` 增加可选 `runtime` 字段透传                                | 承载 `--runtime`                                                                                                                                                                                                                          |
| `lania-plugins-command-add/src/lib.rs`                   | `--template` 的 `choices` 加入新语言 add 模板名；`lania-workflows/src/create/templates.rs` 的 `ADD_TEMPLATE_SPECS` 同步 | 否则 clap 直接拒绝新模板名                                                                                                                                                                                                                |

> 仍**不改**的部分：`CompilerBridgeCapability` trait 形状、`long_running.rs` 的长任务判断（`compiler.dev` 恒长任务；非 web dev 也长任务，正确）、`events.rs` 的事件消费（已容错；`success:false` 目前只让进度条失败，退出码改由 handler 负责）。

`mode` 选项：`build` spec 现有 `choices: ["development","production"]` 保留；非 web adapter 忽略 `mode`。不为此改 spec。

### 6.7 create / add 的真实机制（已修正）

**create（修正）**：初稿说"create 已语言无关，只差数据"不准确。`create` 流程在写完文件后会执行 `services.package_manager.install_all_command(manager)`（`create/create_workflow.rs`），默认跑 npm/pnpm install。虽然在 Go 项目里若 `npm` 缺失会因 `BinaryMissing` 被容忍并记 note，但这不是可靠路径。

本版决策：

- 新增语言模板在 `questions.ts` 里提供 `skipInstall` 问题并**默认 true**；
- 文档写明 `lan create --template go-service --skip-install`；
- 不做 runtime-aware 安装（`go mod tidy` 等）——列为后续工作。

**add（修正）**：`lan add` 走的是 `AddTemplateCapability` → `addTemplate.render`，即**单文件片段渲染**（`templates/src/add-templates/assets`），不是目录脚手架。初稿把它和 `create` 的模板目录混为一谈是错的。

本版决策：

- 每个语言新增单文件 add 模板（如 `go-handler`、`python-module`、`rust-module`、`dart-class`）；
- 同步扩展 Rust 的 `ADD_TEMPLATE_SPECS`（`lania-workflows/src/create/templates.rs`）与 add 命令的 `--template` choices；
- add 的上下文需带上 `language`，让模板按语言选扩展名/语法。

**template**：`lan template` 的 list 逻辑扫描 `templates/` 目录，新增语言目录自动纳入，无需改 Rust。

### 6.8 参数归一化规则（非 web）

| 参数                       | 非 web 行为                                                                     |
| -------------------------- | ------------------------------------------------------------------------------- |
| `build --watch`            | **拒绝**：返回 `success:false` + 明确错误"watch 不支持 runtime X"，不进入长任务 |
| `--mode`                   | 忽略（不透传给语言工具链）                                                      |
| `--open`                   | 忽略                                                                            |
| `--port` / `host`          | 以环境变量 `PORT`/`HOST` 注入 dev 子进程；不据此发 `dev_url`                    |
| `-o/--output-dir`（build） | 替换 build 命令的 `{output}` 占位符；无占位符时注入 `OUTPUT_DIR` 环境变量       |

## 7. 测试策略

仓库**没有测试 CI**（`.github/workflows/` 只有 `docs.yml`），测试通过本地/发布前 `pnpm test`（`pnpm -C ts test && cargo test --workspace`）执行。本机为 win32，所有测试必须跨平台。

### 7.1 单元测试（Node，`node --import tsx --test`）

**跨平台 mock 方案（关键）**：`runBinary` 用 `spawn` 且不开 shell。因此不要用"PATH 放假 bash 脚本"（win32 跑不了）。改为**在 `lan.config.runtimes.<rt>.dev/build` 里注入 `[process.execPath, <fixture.js>, ...]`**，fixture 是普通 JS 文件，由 Node 执行。这样：

- 跨平台（win32/POSIX 都是 `node fixture.js`）；
- 精确走 adapter 的真实 spawn 路径；
- 不需要改 PATH 或 shell。

fixture 约定：读取 `process.argv` 打印到 stdout/stderr，按参数决定退出码（如 `--fail` → `exit 1`），并把收到的 argv 写入旁边文件供断言。

**探测（`detectRuntime`）** —— `mkdtemp` 临时目录：

| 用例           | 布置                                                  | 断言       |
| -------------- | ----------------------------------------------------- | ---------- |
| flag 覆盖最高  | 目录放 `go.mod`，override 传 `rust`                   | `'rust'`   |
| 配置优先       | `lan.config.js` 写 `runtime:'go'` + 放 `package.json` | `'go'`     |
| go.mod         | 只放 `go.mod`                                         | `'go'`     |
| Cargo.toml     | 只放 `Cargo.toml`                                     | `'rust'`   |
| pyproject.toml | 只放 `pyproject.toml`                                 | `'python'` |
| pubspec.yaml   | 只放 `pubspec.yaml`                                   | `'dart'`   |
| package.json   | 只放 `package.json`                                   | `'web'`    |
| 强信号优先     | 放 `go.mod` + `package.json`                          | `'go'`     |
| 兜底           | 空目录                                                | `'web'`    |

**spawn adapter（配置注入 fixture）**：

| 用例               | 布置                                                 | 断言                                                                                                |
| ------------------ | ---------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| go build 成功      | `runtimes.go.build=[node, fixture]`，fixture exit 0  | `result.success===true`、`tool/runtime='go'`、事件含 `compiler_start`+`compiler_done(success:true)` |
| go build 失败      | fixture `exit 1` + stderr 写错误                     | `success===false`、`compiler_issue(error)`、`compiler_done(success:false)`、无 `activeCompiler`     |
| argv 透传          | fixture 记录 argv                                    | 收到的 argv 与配置一致；含 `{output}` 替换结果                                                      |
| dev 长任务         | dev 命令指向 fixture `--sleep`                       | 返回 `activeCompiler`；`stop()` 终止子进程（断言 fixture 进程退出）                                 |
| port/host 注入     | dev fixture 读 env                                   | 子进程 env 含 `PORT`/`HOST`                                                                         |
| python dev 缺配置  | 无 `runtimes.python.dev`                             | fail-fast：`success===false` + 明确错误，无 `activeCompiler`                                        |
| 工具链缺失         | 命令写不存在的二进制名（如 `__lania_missing_bin__`） | fail-fast：`success===false` + warning/error，不抛未捕获异常                                        |
| build --watch 拒绝 | `compiler.build` 传 `watch:true`                     | `success===false` + "watch not supported"                                                           |
| stdout/stderr 映射 | fixture stdout 写 `ok`、stderr 写 `warn`             | 分别产出 `log`(info) 与 `compiler_issue`(warning)                                                   |

**adapter 注册表**：

- `getRuntimeAdapters()` 返回 go/rust/python/dart；
- `resolveRuntimeAdapter('go')` 命中；
- `resolveRuntimeAdapter('web')` 返回 null。

### 7.2 回归测试（现有）

- `compiler.test.ts` 的 vite/webpack/rollup 用例**必须全部通过**，证明 web 行为不变。

### 7.3 Rust 单元测试（`cargo test --workspace`）

| 用例                                                                    | 位置                                             | 断言                                                                           |
| ----------------------------------------------------------------------- | ------------------------------------------------ | ------------------------------------------------------------------------------ |
| `compiler_build_with_options_request` 透传 runtime/outputDir/mode/watch | `lania-node-bridge/src/client/tests.rs`          | JSON params 正确                                                               |
| `compiler_dev_request` 透传 cwd/port/runtime                            | 同上                                             | JSON params 正确                                                               |
| 退出码映射                                                              | `lania-plugins-command-build` 或 host 层新增测试 | `result.success=false` → `EXIT_RUNTIME_ERROR`；缺失 `success` → `EXIT_SUCCESS` |
| config 白名单                                                           | `lania-config/src/tests.rs`                      | `runtime`/`runtimes` 不再产生 `UnknownField`                                   |
| add spec 接受新模板名                                                   | `lania-plugins-command-add` 测试                 | `--template go-handler` 不被 clap 拒绝                                         |

### 7.4 集成 / 端到端

**方式一（默认测试路径，无真实工具链）**：临时目录 + 配置注入 Node fixture，直接调 `compilerPlugin.handle('compiler.build', {...})` / `('compiler.dev', ...)`，验证"探测 → adapter → 结果/事件"。

**方式二（本地人工冒烟，真实工具链，不进 CI）**：

```bash
# Go
mkdir /tmp/lania-go-smoke && cd /tmp/lania-go-smoke
printf 'module smoke\n\ngo 1.22\n' > go.mod
printf 'package main\nfunc main(){}\n' > main.go
lan build      # 期望退出码 0
printf 'package main\nfunc main(){ broken }\n' > main.go
lan build      # 期望退出码非 0，日志有编译错误

# Rust（需 cargo）
cargo init --bin /tmp/lania-rust-smoke && cd /tmp/lania-rust-smoke && lan build

# Python（需 python + build）
# Dart（需 dart）
```

**方式三（模板）**：`lan template` 列出新语言模板；`lan create --template go-service --skip-install` 在临时目录生成文件计划。

## 8. 分阶段落地顺序

1. **阶段 1（模板资产，零 runtime 风险）**：新增 4 个语言 `create` 模板目录（含 `skipInstall` 默认 true）+ `template` 列表测试 + `create --skip-install` 冒烟。
2. **阶段 2（探测）**：`compiler-shared.ts` 加 `Runtime` + `detectRuntime()` + 单测。
3. **阶段 3（Rust 边界改动）**：config 白名单、`--runtime` 选项与透传、build 退出码映射、add spec/`ADD_TEMPLATE_SPECS` 扩展；配套 Rust 单测。
4. **阶段 4（spawn adapter 打样）**：`runtime-adapters/go.ts` + 配置注入 fixture 测试 + worker 分发接入。
5. **阶段 5（铺开）**：python/rust/dart adapter + 参数归一化（§6.8）+ add 单文件模板。
6. **阶段 6（验证）**：全量 `pnpm test` + 真实工具链人工冒烟。

## 9. 决策记录（本文档代为拍板，评审时可推翻）

1. **非 web 失败必须 fail-fast，不伪造 URL / 不挂起**：正确性优先于"看起来成功"。
2. **构建失败映射非零退出码**：打破 web 现状（web 仍吞错），但这只对非 web 生效——web 结果不带 `success`，行为不变。
3. **dev 实时日志不在本迭代范围**：只保证启动阶段输出；流式 IPC 留待后续。
4. **`build --watch` 对非 web 直接拒绝**：不静默忽略，避免"以为在 watch 其实没有"。
5. **测试用 Node fixture + 配置注入，不用 PATH 假二进制**：保证 win32 可跑。
6. **create 依赖安装靠模板默认 `skipInstall` + `--skip-install`**：不做 runtime-aware 安装。
7. **add 只做单文件片段**：目录脚手架仍由 create 负责。
8. **混合仓库靠 `--runtime` 显式覆盖**：不做自动 monorepo 探测。

## 10. 留给实现者的细节

- `detectRuntime` 用 `fs.access` 还是 `existsSync`（跟随文件内既有偏好）。
- stdout/stderr 按行 vs 按块转事件（建议按行）。
- 是否把 `runBinary` 从 `lint/shared.ts` 抽到共享模块（建议抽；lint 子目录无单测，靠类型检查 + 全量测试兜底）。
- spawn 子进程的进程树终止：POSIX 用信号组，win32 用 `taskkill /PID <pid> /T /F`。
