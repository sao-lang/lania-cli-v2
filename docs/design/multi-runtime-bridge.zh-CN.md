# 多运行时 bridge 与 dev/build/create/add/lint 重构

> 状态：**B0–B6 已落地**（B7 起未开工）。
> 目标读者：能直接据此开工的 builder。
> 配套文档：`agent-architecture.zh-CN.md`（agent 拆包）、
> `self-managed-runtime-manager.zh-CN.md`（运行时管理器拆包，§11 提供本文所需的能力）。
>
> 已落地范围（可对照 §11 里程碑）：
>
> - **B0** 工具链血缘：`RuntimeHome::activate()` + 入口调用 + `doctor` 双诊断
> - **B1** `BridgeLaunchSpec` 参数化 + `BridgeRegistry` 骨架
> - **B2** `toolchain.describe` 单点（`plugins/toolchain.ts`）
> - **B2.5** 配置候选去重 + `runtime`/`runtimes` 强类型 + 加载失败可见 + 函数求值上下文
> - **B3** `event.diag` + `Diagnostic` / `RunResult` + 全工具结构化解析
> - **B4** `LintReportRenderer`（分组/对齐/高亮）+ `--json` 新增 `diagnostics[]`
> - **B5** dev 长驻订阅：worker 输出走 `event.diag`（含实时模式）+ `[app]` 前缀渲染 +
>   `ui.dev.quietApp` / `dev --quiet-app` 开关
> - **B6** `add` 生成器重构：框架探测 + 命名推导 + 多文件产物 + 锚点注册 + NestJS 首个落地
>
> 未落地：B7–B14（nextjs/gin/fastapi/axum/flutter 生成器、其余语言 bridge、配置写入等）。

## 0. 问题陈述

最初版本只服务 Node.js：`dev` / `build` / `lint` 全部由 Node bridge 内部直接 `import` 工具库 API
（ESLint / Vite / Webpack / Stylelint / Textlint）或 `spawn` JS 生态二进制（oxlint / oxfmt）。
Node 生态 = 宿主语言，同构带来三个红利：**零序列化、可传对象、单进程**。

支持多运行时后，工具链变成异构的。当前实现（见 §1）是在 TS 层用 `spawn` 补丁式接上，
导致四类问题：

| #   | 问题                                                                                                                          | 现状证据                                                        |
| --- | ----------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| 1   | **工具链血缘断裂**：`lan env use go 1.22` 之后 `lan build` 仍可能用系统 `go`                                                  | `~/.runex/shims` 从未被任何命令前置到 PATH                      |
| 2   | **语言知识写了 4 份且互相漂移**                                                                                               | 见 §2 的知识分布表                                              |
| 3   | **`create` 跨两个进程边界**：模板渲染在 TS、依赖安装在 Rust，且非 Node 模板无法脱离 bridge 运行                               | `DECLARED_TEMPLATES` 只有 `spa-react` / `spa-vue` 两个 fallback |
| 4   | **结果表达贫乏**：多运行时 lint 只取 `filePath + severity`，丢失行号、规则名、修复建议                                        | `language-runners.ts` 各 `parse*Output` 只产出 2 字段           |
| 5   | **日志无法分类**：构建日志、服务端运行日志、被劫持的工具输出全部挤进 `event.log` / `event.compiler_issue`，无法区分来源与顺序 | §7.1 现状实测                                                   |
| 6   | **配置候选不一致**：`lan.config.*` 候选列表 Rust 硬编码 4 个（且重复两处）、TS 7 个；`runtime`/`runtimes` 未进快照            | §6.5 配置统一                                                   |

本文解决这五类问题。**另有 §6.5 把配置统一为 `lan.config.ts`**（保留函数能力，
修掉 Rust/TS 候选列表不一致、`runtime` 未结构化等问题）。
**不解决**工具链本身的实现缺陷。

## 0.1 核心目标（问题 5 展开）

需求方原话：

> build create lint dev 等命令需要可以劫持项目中对终端日志的打印，结果要进行统一，
> 特别是 lint 的结果要有统一的格式和高亮；同时对于项目的其他日志也要可以打印出来，
> 比如服务端的实时接口日志。

拆成四条可验收的要求：

| #   | 要求                                       | 本文对应章节 |
| --- | ------------------------------------------ | ------------ |
| R1  | 能劫持工具链子进程的 stdout/stderr         | §7.2         |
| R2  | 统一的结果模型（结构化，非纯文本）         | §7.3         |
| R3  | lint 结果统一格式 + 高亮                   | §7.4         |
| R4  | 项目运行期日志（如 HTTP 请求日志）也能透出 | §7.5         |

## 1. 现状实测：每条命令到底怎么调工具链

### 1.1 三类调用机制（不是两类）

| 机制                   | 含义                                           | 使用者                                                                                            |
| ---------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| **A. 进程内 API**      | Node 内 `import` 工具库，直接调用导出的函数/类 | ESLint、Stylelint、Textlint、Vite、Webpack、Rollup、Nest、ejs                                     |
| **B. spawn 二进制**    | `child_process.spawn` 一个 CLI，解析 stdout    | oxlint、oxfmt、ruff、black、mypy、flake8、golangci-lint、`go vet`、`cargo clippy`、`dart analyze` |
| **C. Rust 直接 spawn** | 不经 bridge，`lania-exec` 直接执行             | `create` 的 `install-deps`、`lan tools run`                                                       |

**关键事实：只有「有 JS 模块形态」的工具才走 A。** 这不是桥接设计的选择，是运行时字节码约束：
`ruff`（Rust 编译）、`golangci-lint`（Go 编译）、`dart analyze`（Dart AOT）都无法被 Node `import`。

### 1.2 逐命令调用链

| 命令          | 执行位置                                    | 运行时判定                           | 工具链调用                               | 进程层数          |
| ------------- | ------------------------------------------- | ------------------------------------ | ---------------------------------------- | ----------------- |
| `add`         | 纯 TS（ejs 渲染）+ 可选 Rust `install_deps` | 模板名即运行时                       | **不调用任何运行时工具链**               | 1                 |
| `create`      | Rust workflow + TS 渲染                     | `template_runtime()` 硬编码 4 项映射 | `lania-pm` 命令表 → `lania-exec`         | 1（+bridge 渲染） |
| `build`/`dev` | Rust → bridge → compiler worker → spawn     | 特征文件表（`compiler-shared.ts`）   | `spawn('go'\|'cargo'\|'python'\|'dart')` | **3**             |
| `lint`        | Rust → bridge → spawn                       | adaptor 枚举                         | `spawn` 或进程内 API                     | **2**             |

`build` 的 3 层进程是现状最脆弱处：`Rust → node → compiler-worker → go build`。
任一层崩溃都要靠上层重启，且 POSIX 下要靠每层都设 `detached` 才能整组 kill。

### 1.3 运行时知识分布（问题 2 的证据）

同一份知识被写在 4 个地方，且不一致：

| 知识                     | 位置 A                                | 位置 B                                | 位置 C                | 位置 D          | 一致？         |
| ------------------------ | ------------------------------------- | ------------------------------------- | --------------------- | --------------- | -------------- |
| 特征文件 → 运行时        | `compiler-shared.ts`（5 项）          | `lania-env/language.rs`（版本文件）   | —                     | —               | ❌             |
| 运行时 → 包管理器        | `lania-pm`（`managers_for_language`） | `templates/language-options.ts`       | —                     | —               | ❌             |
| 运行时 → lint 工具集     | `language-runners.ts`（8 个 adaptor） | `lania-env/language.rs`（companions） | —                     | —               | ❌             |
| 运行时 → 可执行文件位置  | `lania-exec::resolve_executable`      | `language-runners.ts::resolveTool`    | `spawn()`（交给系统） | `lan env which` | ❌             |
| 运行时 → 默认 build 命令 | `runtime-adapters/commands.ts`        | —                                     | —                     | —               | ✅（唯一单点） |

### 1.4 七运行时全景（本文覆盖范围）

本文的 bridge 覆盖 **7 个运行时**，它们分属**两个 JS 运行时簇**与**三个独立簇**：

| 运行时     | 簇         | 包管理器          | build/dev 命令                           | lint 工具                                      | 现状             |
| ---------- | ---------- | ----------------- | ---------------------------------------- | ---------------------------------------------- | ---------------- |
| **node**   | JS（同构） | npm / pnpm / yarn | vite / webpack / rollup / nest / tsc     | eslint / stylelint / textlint / oxlint / oxfmt | ✅ 已实现        |
| **bun**    | JS（同构） | bun               | `bun run` + bun 内置 bundler             | 复用 eslint 生态（bun 可直跑）                 | ⚠️ 未接          |
| **deno**   | JS（同构） | deno              | `deno run` / `deno task` / `deno bundle` | `deno lint` / `deno fmt`                       | ⚠️ 未接          |
| **python** | 独立       | pip / uv / poetry | `python -m build`                        | mypy / ruff / black / flake8                   | ⚠️ 部分（spawn） |
| **go**     | 独立       | go                | `go build` / `go run`                    | golangci-lint / go vet                         | ⚠️ 部分（spawn） |
| **rust**   | 独立       | cargo             | `cargo build` / `cargo run`              | cargo clippy                                   | ⚠️ 部分（spawn） |
| **dart**   | 独立       | pub               | `dart compile exe` / `dart run`          | dart analyze                                   | ⚠️ 部分（spawn） |

**关键结构性差异：bun / deno 与 node 同属 JS 运行时簇，但工具链入口不同。**

| 差异点                  | node                              | bun                           | deno                                    |
| ----------------------- | --------------------------------- | ----------------------------- | --------------------------------------- |
| 是否可 `import` JS 工具 | ✅                                | ✅（Node 兼容层）             | ⚠️ 部分（npm: 说明符，无 node_modules） |
| 内置 lint               | ❌（需 eslint/oxlint）            | ❌（用 eslint，bun 直接执行） | ✅ `deno lint` 内置                     |
| 内置 fmt                | ❌（prettier/oxfmt）              | ❌                            | ✅ `deno fmt`                           |
| 内置 bundler            | ❌（vite/webpack）                | ✅ `bun build`                | ✅ `deno bundle`                        |
| 锁文件                  | `package-lock` / `pnpm-lock.yaml` | `bun.lockb` / `bun.lock`      | `deno.lock`                             |
| 依赖目录                | `node_modules`                    | `node_modules`                | **无**（`DENO_DIR` 全局缓存）           |

**因此 bun / deno 不能简单归入 node bridge**：

- `deno lint` / `deno fmt` **是 Deno 内置的 Rust 实现**，没有 JS 模块形态，
  Node bridge 无法 `import`，只能 spawn `deno lint`。
- Deno **没有 `node_modules`**，`resolveModuleFromCwd`（现有 `core/runtime.ts`）在 Deno 项目里
  会解析失败——因为 eslint 根本不会出现在项目目录里。
- Bun 有 Node 兼容层，`import('eslint')` 可行，但**不能假定**用户用 bun 就一定装了 eslint。

**结论**（这是 §3 分簇设计的依据）：

```text
node / bun / deno   → 共用「JS 簇 bridge」的传输与解析层，但工具选择表不同
                      （deno 的 lint 走内置 Rust 实现 = spawn；node/bun 可走 API）
python / go / rust / dart → 各自独立 bridge
```

## 2. 目标架构

### 2.1 分层职责划分

```text
┌───────────────────────────────────────────────────────────────────┐
│ Rust 宿主  lania-host / lania-cli                                 │
│  命令 handler：解析 argv → 选择 bridge → 收集事件                 │
├───────────────────────────────────────────────────────────────────┤
│ 统一结果层（§7.3 / §7.4）                                         │
│  Diagnostic / RunResult                                           │
│  LintReportRenderer · BuildReportRenderer（lania-presentation）    │
├───────────────────────────────────────────────────────────────────┤
│ 传输层  lania-node-bridge（保持现状，参数化启动）                 │
│  握手 / 心跳 / 事件先于响应 / 超时 / kill tree                    │
│  事件：event.diag · event.*（§7.2）                               │
├───────────────────────────────────────────────────────────────────┤
│ 运行时 bridge 进程（每运行时一个，说同一套 jsonl 协议）           │
│  ├─ node/bun/deno  @lania-cli-v2/node-bridge（JS 簇，按运行时切工具表）│
│  ├─ python         lan-py-bridge                                  │
│  ├─ go             lan-go-bridge                                  │
│  ├─ rust           lan-rs-bridge                                  │
│  └─ dart           lan-dart-bridge                                │
│  ├─ go     lan-go-bridge               Go 工具 API                │
│  └─ rust   lan-rs-bridge               Rust 工具 API              │
│  共同职责：劫持子进程 stdout/stderr → event.diag（§7.2）          │
│            解析为 Diagnostic（§7.3）                              │
├───────────────────────────────────────────────────────────────────┤
│ 工具链  eslint / vite / mypy / ruff / golangci-lint / cargo       │
└───────────────────────────────────────────────────────────────────┘
```

**分层的意义**：语言差异只存在于最下两层（bridge + 工具链）。
`Diagnostic` / `RunResult` / 渲染器**对所有运行时完全相同**，
因此「新增一个运行时」只需实现一个 bridge，不需要碰渲染与输出层。

### 2.2 调度模型：Rust 侧注册表（已定）

**不采用**「Node bridge 当路由器」（会让 `build`(Go) 变成 4 层进程）。
采用 **Rust 侧按运行时持有客户端，按需启动**：

```rust
// lania-host/src/runtime/core.rs（改造示意）
pub struct HostRuntimeServices {
    // 现有单例保留：指向 node bridge（兼作 bun/deno 的 JS 簇 bridge）
    node_bridge: NodeBridgeClient,
    // 新增：运行时 bridge 注册表，懒加载
    bridges: BridgeRegistry,
}

pub struct BridgeRegistry { /* HashMap<RuntimeKind, OnceCell<NodeBridgeClient>> */ }

pub enum RuntimeKind { Node, Bun, Deno, Python, Go, Rust, Dart }
```

`CommandExecutionContext` 现有 `node_bridge()` 保持不变（指向 JS 簇）。
新增 `bridge(kind)`，handler 按项目运行时选择。

**JS 簇的特殊处理**：`Node` / `Bun` / `Deno` 共用**同一个 bridge 进程**
（`@lania-cli-v2/node-bridge`），通过 `params.runtime` 传入具体运行时，
由 bridge 内部切换工具表（见 §3.2）。理由：

- 三者都是 JS/TS 生态，ejs 模板渲染、`lan.config` 加载等能力完全共享；
- 为 bun/deno 各起一个 Node 进程毫无收益（它们没有独立于 Node 的工具生态）；
- Deno 的 `deno lint` 本身就是 spawn，与它在 Node 还是 Deno 进程里执行无关。

**降级策略（必须实现）**：请求的运行时 bridge 不可用时（未安装/启动失败），
必须回落到 CLI spawn，而不是报错。回落路径在 §5.3 定义。

### 2.3 协议复用：不新增协议

现有 `HandshakeRequest` 已含 `transport: String`，`HandshakeResponse` 已含 `bridge_name` /
`methods` / `events` / `failure_strategy`。多运行时 bridge **完全复用**：

```jsonc
// handshake 响应示例（Go bridge）
{ "protocolVersion": "0.1.0", "bridgeName": "lan-go-bridge",
  "methods": ["lint.go", "build.go"], "events": ["event.lint_file", ...] }
```

Rust 侧唯一需要改的是 `bridge_launch_spec()` 参数化：

```rust
// 现状：硬编码找 node-bridge 包目录
pub(super) fn bridge_launch_spec() -> Result<BridgeLaunchSpec>;

// 改为：调用方指定可执行文件与工作目录
pub(super) fn bridge_launch_spec_for(program: &Path, args: &[String], dir: &Path)
    -> Result<BridgeLaunchSpec>;
```

## 3. 各运行时 bridge 设计

### 3.1 通用约定（所有运行时 bridge 必须遵守）

| #   | 约定                                                           | 理由                                          |
| --- | -------------------------------------------------------------- | --------------------------------------------- |
| 1   | **stdout 只输出协议 envelope**；所有日志走 stderr              | 否则上游 jsonl 解析失败（node bridge 已踩过） |
| 2   | 事件必须先于对应 request 的 response 发出                      | `BridgeActiveCall` 依赖这个顺序               |
| 3   | 逐行 JSON，禁止跨行、禁止 BOM                                  | `BufReader::lines()` 实现前提                 |
| 4   | 收到 `shutdown` 必须优雅退出；父进程消失时自杀（读 stdin EOF） | 避免孤儿进程                                  |
| 5   | 长任务（dev/watch）必须保持 stdout 打开并持续 emit             | Rust 侧靠 stdout 未关闭判断存活               |
| 6   | **必须劫持子进程 stdout/stderr → `event.diag`（§7.2）**        | 否则日志丢失或无法分类                        |
| 7   | **必须产出统一 `Diagnostic`（§7.3）**                          | 否则 lint 结果无法统一渲染                    |

### 3.2 JS 簇 bridge（node / bun / deno）

三者共用 `@lania-cli-v2/node-bridge` 进程，按 `params.runtime` 切换工具表。

```text
ts/packages/node-bridge/src/plugins/
├── compiler.ts            ← 按 runtime 分派：node→vite/webpack；bun→bun build；deno→deno bundle
├── lint/
│   ├── plugin.ts          ← 统一入口
│   ├── runners.ts         ← node/bun：eslint/stylelint/textlint（进程内 API）
│   ├── deno-runners.ts    ← deno：spawn `deno lint` / `deno fmt`（内置 Rust 实现，无 API）
│   └── language-runners.ts ← 其他运行时（Python/Go/Rust/Dart），迁移后变薄
└── runtime-adapters/      ← 迁移到各独立 bridge 后删除
```

**工具表（按运行时）**：

| 运行时 | build                                | dev                | lint                                                          |
| ------ | ------------------------------------ | ------------------ | ------------------------------------------------------------- |
| node   | vite / webpack / rollup / nest / tsc | vite / webpack dev | eslint / stylelint / textlint（API）+ oxlint / oxfmt（spawn） |
| bun    | `bun build`（内置 bundler）          | `bun run --watch`  | **复用 eslint 生态**（bun 可直接执行 eslint 二进制）          |
| deno   | `deno bundle` 或 `deno task build`   | `deno task dev`    | `deno lint` / `deno fmt`（**内置，必须 spawn**）              |

**必须处理的三个 Deno 特有问题**：

| 问题                                     | 原因                                                     | 处理                                                                                    |
| ---------------------------------------- | -------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `resolveModuleFromCwd` 解析失败          | Deno 无 `node_modules`，eslint 不在项目目录              | 遇到 `runtime=deno` 时**跳过** JS 模块解析，直接用 `deno lint`                          |
| `deno lint` 输出格式是**人类可读文本**   | 无 `--json`（v2 有 `--json`，v1 无）                     | 优先用 `deno lint --json`（v2+）；v1 回退正则解析                                       |
| `deno.lock` 与 `package-lock` 共存时判定 | 项目可能同时有 `deno.json` 与 `package.json`（双运行时） | 判定优先级：`deno.json`/`deno.jsonc` > `package.json` 存在 `bun.lockb` > `package.json` |

**Bun 注意事项**：

- `bun.lockb`（旧）与 `bun.lock`（新，文本）都存在，判定时两者都要认。
- Bun 执行 eslint 的方式是 `bunx eslint` 或 `bun run eslint`，**不是** `node_modules/.bin/eslint`。
  `resolveTool` 需要按 runtime 选择执行器。

**为什么不为 bun/deno 单起进程**：它们的工具生态就是 JS 生态（或内置 Rust 实现），
没有独立于 Node 的工具需要"在原生进程内调用"。单独起进程只增加复杂度。

### 3.3 Python bridge（`lan-py-bridge`）

**为什么最有价值**：`mypy`、`flake8`、`build` 提供**真 Python API**，
进程内调用可省掉解释器启动（~200ms/次，lint 场景高频）。
且 Python 是**纯脚本**，无需编译，**零平台包**。

```text
rust/crates/lania-py-bridge/          ← Rust crate：定位 python + 分发脚本
└── assets/lan_py_bridge.py           ← 单文件 Python 脚本，随 binary 打包

实现要点：
  · readline 循环读 stdin，逐行 json.loads
  · 按 method 分派：lint.mypy / lint.ruff / lint.flake8 / build.python
  · 用 importlib 懒加载工具，未安装的工具返回 structured error（不崩）
  · 事件通过 print(json.dumps(...), flush=True) 写 stdout
  · 工具调用若必须子进程（如 python -m build），优先尝试 API：
      build.ProjectBuilder(...).build(...)
```

**工具 API 可用性实测**：

| 工具   | Python API                            | 可用性 | 备注                                                    |
| ------ | ------------------------------------- | ------ | ------------------------------------------------------- |
| mypy   | `mypy.api.run()`                      | ✅     | 返回 `(stdout, stderr, status)`，需自行解析文本为结构化 |
| ruff   | `ruff.__main__` 仅 CLI                | ⚠️     | Python 包只是二进制分发壳，**无 API**，退化为 spawn     |
| flake8 | `flake8.api.legacy.get_style_guide()` | ✅     | 但已过时，建议默认不启用                                |
| build  | `build.ProjectBuilder`                | ✅     | wheel 构建内部仍会 spawn 后端                           |

**结论**：Python bridge 的真实收益集中在 `mypy`（否则退化为 `spawn`）。仍需实现，因为：

1. 统一事件模型（`LintFile` / `LintSummary`）比解析 mypy 文本输出可靠；
2. 为后续接入 `pylint` / `pyright`（有 API）留出位置。

### 3.4 Go bridge（`lan-go-bridge`）

**为什么建**：`go/analysis`、`go/packages`、`go/ast` 是真正的 lib，
可在 Go 进程内完成静态分析，避免 `golangci-lint` 的配置解析开销。

```text
rust/crates/lania-go-bridge/          ← Rust crate：定位 go toolchain + 分发源码
└── assets/                            ← Go 源码（构建期编译成二进制，见 §8）
```

**工具 API 可用性实测**：

| 工具            | 进程内 API                      | 可用性 | 备注                                   |
| --------------- | ------------------------------- | ------ | -------------------------------------- |
| `go/analysis`   | `unitchecker` / `singlechecker` | ✅     | 可跑任意 Analyzer                      |
| `go/packages`   | ✅                              | ✅     | 加载类型信息                           |
| `go vet`        | 等价于 `unitchecker.Main`       | ✅     | 可完全进程内                           |
| `go build`      | ❌ `cmd/go` 官方不支持 import   | ❌     | **必须** `exec.Command("go", "build")` |
| `golangci-lint` | `pkg/lint` 半公开，配置绑 CLI   | ⚠️     | 建议保留 spawn                         |

**Go bridge 的净收益**：`go vet` 与自定义 Analyzer 可进程内；
`go build` / `golangci-lint` 仍是子进程（bridge 内 `exec`）。

### 3.5 Rust bridge（`lan-rs-bridge`）

**结论：零 API 收益，仅作架构完整性保留。**

| 工具                  | 进程内 API                        | 可用性 | 备注                                                        |
| --------------------- | --------------------------------- | ------ | ----------------------------------------------------------- |
| `cargo build`         | ❌ `cargo` lib 不公开             | ❌     | 必须子进程                                                  |
| `cargo clippy`        | ❌ 是 rustc driver                | ❌     | 架构上必须由 cargo 以子进程驱动                             |
| `cargo fmt`           | ❌（rustfmt 需 toolchain 上下文） | ❌     | 子进程                                                      |
| `syn` / `tree-sitter` | ✅                                | ✅     | 但这些是 lania 自身能力（`lania-agent` 已用），不属于工具链 |

**建议**：Rust bridge 在本轮**只实现协议骨架 + `lint.rust`（委托 `cargo clippy` spawn）**，
不追求 API 化。目的是让「七个运行时同构」成立，便于统一事件模型。

### 3.6 Dart bridge（`lan-dart-bridge`）

**结论：零 API 收益，但 build/dev 必须独立（不能由 Node 代 spawn）。**

| 工具               | 进程内 API                       | 可用性 | 备注                                           |
| ------------------ | -------------------------------- | ------ | ---------------------------------------------- |
| `dart analyze`     | `analysis_server` 是 LSP，非 lib | ❌     | **必须 spawn `dart analyze --format machine`** |
| `dart compile exe` | ❌ 需 Dart SDK 子进程            | ❌     | 子进程                                         |
| `dart run`         | ❌                               | ❌     | 子进程（dev 长驻）                             |
| `dart pub get`     | ❌                               | ❌     | 已在 Rust `lania-pm`（`pub get`），无需 bridge |
| `dart format`      | ❌                               | ❌     | 已在 `lania-format`，无需 bridge               |

**Dart bridge 的存在意义**（与 Rust bridge 不同）：

- 现状 Dart 的 build/dev 由 **Node bridge 的 `runtime-adapters/dart.ts`** 代 spawn，
  这是"用 Node 替 Dart 起进程"的错位。独立 Dart bridge 后，
  进程链变成 `Rust → dart bridge → dart compile`，去掉 Node 这一层。
- `dart analyze` 的 `--format machine` 输出需要专门解析（`|` 分隔 8 字段），
  放在 Dart bridge 内比放在 Node 的 `language-runners.ts` 更内聚。

```text
rust/crates/lania-dart-bridge/        ← Rust crate：定位 dart SDK + 分发源码
└── assets/                            ← Dart 源码（构建期编译成二进制，见 §9）
```

**注意**：Dart bridge 是**纯 spawn 型**（无 API 可用），因此它的实现量最小
（约 300 行：协议循环 + 3 个 method + machine 格式解析），是验证
"Rust 侧注册表 + 多 bridge" 最合适的**第一个落地样本**。

> ⚠️ 砍需求的优先级（维护成本 > 收益时）：
>
> 1. 先砍 Rust bridge（零收益）；
> 2. 再砍 Go bridge（仅 `go vet` 有 API 收益，`go build`/`golangci-lint` 仍需 spawn）；
> 3. **Python / Dart 不建议砍**（Python 有 mypy API；Dart 是去掉 Node 中转的关键）。
>    砍掉的语言继续走 Rust 侧 `lania-exec` 直接 spawn（参考 §5.3 的回落路径）。

## 4. 命令重构

### 4.1 重构总原则

1. **事件保持语言中立**：`BridgeEventMethod` 不新增语言相关内容。
   多运行时差异只体现在 bridge 内部，Rust 侧渲染代码不感知语言。
2. **每个命令只选一个 bridge**：不再出现「`create` 一半 Rust 一半 Node」。
3. **语言知识单点**：语言 → (特征文件, 包管理器, lint 工具, 默认命令) 只在
   `lania-runtime-manager` 定义一次。
4. **输出统一走 `RunResult`**：所有命令的日志与诊断都收敛到 `Diagnostic` / `RunResult`
   再由渲染器输出（详见 §7）。这是"结果统一 + 高亮 + 运行期日志"的实现基础。

### 4.2 `build` / `dev`

现状 3 层进程（`Rust → node → compiler-worker → spawn`）。重构后：

```text
Node 项目：  Rust → node bridge（进程内 vite/webpack API）          2 层（无 worker）
Go 项目：    Rust → go bridge（内 exec go build）                    2 层
Python 项目：Rust → python bridge（内 import build）                2 层
Rust 项目：  Rust → rust bridge（内 spawn cargo build）             2 层
```

**保留 compiler-worker 的理由**：dev/watch 需要把生命周期绑定到独立进程（崩溃不带崩 bridge）。
纯 build（非 watch）不需要 worker，直接在主 bridge 内跑。

**日志与实时输出（§7.2、§7.5）**：

- worker 的 stdout/stderr 现在映射成 `logEvent` / `compilerIssueEvent`（会把任意 stderr 当 warning）。
  改为统一发 `event.diag`，带 `channel: "build"` 与 `channel: "app"` 区分。
- `compiler_dev` 返回后必须**继续订阅事件**（`open_call` + 不设总超时），
  否则服务端启动后的 `GET /api/...` 日志完全看不到。这是现状最大缺口。

**改动清单**：

| 文件                                            | 改动                                                                |
| ----------------------------------------------- | ------------------------------------------------------------------- |
| `lania-plugins-command-build/src/request.rs`    | `build_request` 增加 `bridgeKind` 参数，由 `language` 决定          |
| `lania-plugins-command-dev/src/request.rs`      | 同上                                                                |
| `lania-plugins-command-dev/src/handlers.rs`     | dev handler 从 `call_async` 改为 `open_call` + 事件循环（§7.5.2）   |
| `node-bridge/plugins/compiler.ts`               | 只保留 web runtime；非 web runtime 返回 `unsupported` 让 Rust 转投  |
| `node-bridge/plugins/compiler-worker-client.ts` | `attachStreamCapture` 改为产出 `event.diag`（含 `seq` / `channel`） |
| `node-bridge/plugins/runtime-adapters/*`        | 迁移到对应语言 bridge（或保留作为回落，见 §5.3）                    |

**关键决策**：`runtime-adapters/` 里已有 `go.ts` / `python.ts` / `rust.ts` / `dart.ts`（各 2 行），
它们本质是「Node 替其他语言 spawn」。迁移后这些文件删除，由各语言 bridge 接管。

### 4.3 `lint`

现状：`runAdaptor` 先试 `runLanguageAdaptor`（spawn），失败再走 JS adaptor（API）。

重构后按项目语言分流：

```text
JS 项目     → node bridge（eslint/stylelint/textlint 走 API；oxlint/oxfmt 走 spawn）
Python 项目 → python bridge（mypy 走 API；ruff/black/flake8 走 spawn）
Go 项目     → go bridge（go vet 走 API；golangci-lint 走 spawn）
Rust 项目   → rust bridge（clippy 走 spawn）
```

**同时修问题 4（结果表达贫乏）**：所有 bridge 统一产出 §7.3 定义的 `Diagnostic`：

```jsonc
{
  "tool": "ruff",
  "id": "E501",
  "severity": "error",
  "message": "line too long (95 > 88)",
  "file": "src/main.py",
  "range": { "start": { "line": 12, "col": 5 }, "end": { "line": 12, "col": 20 } },
  "url": "https://docs.astral.sh/ruff/rules/line-too-long",
  "fix": { "available": true, "kind": "safe" },
}
```

**这是本方案对现有输出契约的破坏性变更**（`lan lint --json` 结构变化）。
迁移策略与兼容期见 §7.6。

**渲染与日志分流见 §7.4**：`lint` 的 TTY 输出必须按文件分组、列对齐、按 severity 高亮；
`--json` 输出 `RunResult`。这两件事都在 `lania-presentation` 新增的
`LintReportRenderer` 里完成，不在 bridge 里做。

### 4.4 `create`

**解决跨进程边界（问题 3）**：`create` 是"生成文件 + 装依赖"，两件事都不需要 JS 生态。
但模板渲染目前在 TS（ejs）。

方案：**保留 TS 渲染（ejs 生态成熟），但补 Rust 内建 fallback**。

| 模板                                                  | 现在      | 重构后                                                |
| ----------------------------------------------------- | --------- | ----------------------------------------------------- |
| `spa-react` / `spa-vue`                               | Rust 声明 | 不变                                                  |
| `go-service` / `rust-lib` / `python-cli` / `dart-cli` | 仅 TS ejs | **Rust 声明式 fallback**（新增 `DECLARED_TEMPLATES`） |
| `toolkit` / `toolkit-monorepo` / `nestjs` / `express` | 仅 TS ejs | 不变（Node 项目，bridge 必然可用）                    |

**依赖安装不变**：继续走 `lania-pm` + `lania-exec`（Rust 侧），这是正确的。

**配置生成统一**：所有模板产出的项目配置一律是 **`lan.config.ts`**（§6.5），
使用 `defineConfig` 以获得类型提示。
`template_runtime()` 的映射从"运行时 → 包管理器"扩展为"运行时 → 包管理器 + `runtime` 字段值"。

**为什么只补这 4 个**：bridge 不可用时的损失集中在非 Node 语言项目，
因为 Node 项目不可能没有 bridge（bridge 就是 Node 进程）。

**依赖安装不变**：继续走 `lania-pm` + `lania-exec`（Rust 侧），这是正确的。

### 4.5 `add`：从「平铺模板」重构为「框架感知的生成器」

这是本次重构中**唯一需要重新设计交互模型**的命令。

#### 4.5.1 现状问题

当前 `add` 是 16 个**平铺的模板名**（`v2` / `rfc` / `rcc` / `svelte` / `astro` /
`go-handler` / `python-module` / `rust-module` / `dart-class` / `eslint` / …），
本质是「选一个模板 → 渲染一个文件」。三个结构性问题：

| #   | 问题                      | 证据                                                                                              |
| --- | ------------------------- | ------------------------------------------------------------------------------------------------- |
| 1   | **与框架无关**            | `rfc` 生成的 React 组件不区分 Next.js / Remix / Vite；`python-module` 不区分 Django / FastAPI     |
| 2   | **只产出一个文件**        | `AddWorkflow` 的 `files` 恒为 `vec![PlannedFile{..}]` 单元素；无法生成 Nest 那种"5 个文件一套"    |
| 3   | **语言模板只有 3 行骨架** | `go-handler.ejs` 仅一个 `func X(input string) string`，没有框架约定（handler 签名、路由注册、DI） |

**参考对象：Nest CLI 的 `generate`（`nest g`）**，它的关键设计值得借鉴：

```bash
nest g resource users        # 一次生成 module + controller + service + dto + entity + 测试
nest g controller users      # 单个
nest g module users
# 自动：命名风格转换、目录推导、注册到父 module、生成 spec 文件
```

Nest 模型的四个可借鉴点：

| 借鉴点                  | Nest 做法                                        | 我们的对应设计                                         |
| ----------------------- | ------------------------------------------------ | ------------------------------------------------------ |
| **生成器（schematic）** | 每个 `generate` 子命令是一个 schematics 集合     | `Generator` 抽象，一个生成器产出 N 个文件              |
| **命名推导**            | `users` → `UsersController` / `users.service.ts` | 统一 `name` 规范与风格转换（camel/Pascal/kebab/snake） |
| **框架约束**            | 生成内容符合 Nest DI 约定（`@Injectable` 等）    | **按框架注册生成器**，非通用模板                       |
| **副作用注册**          | 自动把 controller 注册进所在 module              | 支持"修改已有文件"（见 §4.5.5）                        |

#### 4.5.2 目标模型

```text
lan add <generator> [name] [options]
        │
        ├── 框架探测：读 lan.config 或特征文件 → 确定 framework
        ├── 生成器解析：<framework> 下找 <generator>
        ├── 提示补全：缺失的必填项（name / path / 变体）
        └── 渲染 → N 个 PlannedFile → 冲突检查 → 合并/注册 → 写盘 → 格式化
```

**三级结构**：`框架` → `生成器` → `产物（N 个文件）`。

```rust
// lania-workflows/src/add/generator.rs
pub struct Generator {
    /// 例如 "nestjs" / "nextjs" / "fastapi" / "gin"
    pub framework: &'static str,
    /// 例如 "resource" / "controller" / "module"
    pub name: &'static str,
    pub label_en: &'static str,
    pub label_zh: &'static str,
    /// 生成器参数（位置 + 选项）
    pub questions: &'static [GeneratorQuestion],
    /// 产物清单：由模板 + 命名推导得出
    pub artifacts: &'static [ArtifactRule],
    /// 需要注册到已有文件的副作用（可选）
    pub registrations: &'static [RegistrationRule],
}

pub struct ArtifactRule {
    /// 路径模板，支持 {name} {Name} {name_kebab} 等占位符
    pub path: &'static str,
    /// ejs 模板名
    pub template: &'static str,
    /// 条件：仅在某些变体下生成
    pub when: Option<&'static str>,
}
```

#### 4.5.3 框架与生成器矩阵

**先做四个框架**（覆盖现有 4 个语言模板 + Node 生态最常见的两个约定）：

| 框架        | 运行时 | 生成器                                                                                                                                                                   |
| ----------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **nestjs**  | node   | `resource` `module` `controller` `service` `dto` `entity` `guard` `interceptor` `filter` `pipe` `middleware` `decorator` `interface` `class` `enum` `resolver` `gateway` |
| **nextjs**  | node   | `page` `layout` `route` `loading` `error` `not-found` `component` `hook` `middleware` `api` `action`                                                                     |
| **gin**     | go     | `handler` `model` `repository` `service` `middleware` `router`                                                                                                           |
| **fastapi** | python | `router` `endpoint` `model` `schema` `crud` `dependency` `middleware`                                                                                                    |
| **axum**    | rust   | `handler` `model` `repository` `service` `middleware` `router`                                                                                                           |
| **flutter** | dart   | `page` `widget` `controller` `model` `service` `repository`                                                                                                              |

**通用生成器（框架无关，保留现有能力并归位）**：

| 生成器                                                                                      | 产物     |
| ------------------------------------------------------------------------------------------- | -------- |
| `component`（按框架展开为 `rfc`/`rcc`/`v2`/`v3`/`svelte`/`astro`/`widget`）                 | 组件文件 |
| `config`（`eslint` / `prettier` / `stylelint` / `tsconfig` / `editorconfig` / `gitignore`） | 配置文件 |

**框架探测优先级**（复用 §5 的 `toolchain.describe`，不新增知识来源）：

```
1. lan.config.ts 的 add.framework（显式）
2. 特征文件推断：
     nest-cli.json                    → nestjs
     next.config.{js,ts,mjs}          → nextjs
     go.mod + gin-gonic/gin           → gin
     pyproject.toml + fastapi         → fastapi
     Cargo.toml + axum                → axum
     pubspec.yaml + flutter           → flutter
3. 回退：按运行时给默认框架（node→nextjs? 不，回退到"通用生成器"，只列 component/config）
```

**关键决策：探测失败时只暴露通用生成器**，不猜框架。猜错的代价（生成不符合约定的文件）高于
"让用户显式选"的成本。

#### 4.5.4 命名推导（Nest 风格）

输入一个 `name`，推导所有变体。这是"一个名字生成 5 个文件"的基础：

| 变体              | `users`（复数输入） | `user-profile`（kebab 输入） |
| ----------------- | ------------------- | ---------------------------- |
| `{name}`          | `users`             | `user-profile`               |
| `{Name}`          | `Users`             | `UserProfile`                |
| `{name_pascal}`   | `Users`             | `UserProfile`                |
| `{name_camel}`    | `users`             | `userProfile`                |
| `{name_kebab}`    | `users`             | `user-profile`               |
| `{name_snake}`    | `users`             | `user_profile`               |
| `{name_singular}` | `user`              | `user-profile`               |
| `{NameSingular}`  | `User`              | `UserProfile`                |

**单复数转换**：内置简单规则（`-s` / `-es` / `-ies` / 不规则表约 30 词），
**不引入 `inflector` 依赖**——生成器场景下错误命名可被用户立即发现并改名，
不值得为此加依赖。

#### 4.5.5 副作用注册（`registrations`）

Nest 的 `resource` 之所以"一步到位"，是因为它会**修改已有的 module 文件**把新的
controller/service 注册进去。我们同样需要，但要控制在最小范围：

```rust
pub struct RegistrationRule {
    /// 目标文件路径模板，例如 "src/{name_kebab}/{name_kebab}.module.ts"
    pub target: &'static str,
    /// 插入位置锚点：正则或固定标记
    pub anchor: RegistrationAnchor,
    /// 插入内容模板
    pub snippet: &'static str,
}
```

**v1 只支持三种安全的注册模式**：

| 模式           | 适用框架              | 实现方式                                                   |
| -------------- | --------------------- | ---------------------------------------------------------- |
| `MarkerInsert` | nestjs                | 在 `// <lania:imports>` 标记处插入；标记不存在则跳过并警告 |
| `RustModDecl`  | axum / gin（Rust/Go） | 在 `mod.rs` / `router.go` 的声明块追加 `pub mod {name};`   |
| `None`         | 其余                  | 不做修改，只在输出里打印"请手动注册"及代码片段             |

**禁止**：v1 不做 AST 级修改（`tree-sitter` 改写 TypeScript/Go 风险高，且用户格式化风格各异）。
`MarkerInsert` 是显式标记 + 纯文本插入，可预测、可回滚。

#### 4.5.6 与现有实现的关系

| 现有资产                                                  | 处置                                                               |
| --------------------------------------------------------- | ------------------------------------------------------------------ |
| `add-templates/assets/*.ejs`（16 个）                     | **保留为模板**，但归属到具体生成器的 `artifacts[].template`        |
| `add-template-runtime.ts` 的 `ADD_TEMPLATES`              | **删除**（平铺清单），改为从 Rust 的 `Generator` 表读取            |
| `rust/.../create/templates.rs` 里的 `ADD_TEMPLATE_*` 常量 | 同上，改为 `generators.rs`                                         |
| `AddWorkflow` 的 `vec![PlannedFile]` 单文件               | 改为 `Vec<PlannedFile>`（支持 N 个）+ 注册副作用                   |
| `run_add_prompt`                                          | 改为两级：先选生成器（按框架过滤），再填生成器专属问题             |
| `archetype`（v2/v3/rfc/rcc/…）                            | 保留，但作为 `component` 生成器的 `variant` 选项，不再是一级模板名 |

#### 4.5.7 示例交互

```bash
# NestJS 项目
$ lan add resource users
? Which variant? (REST API | GraphQL)
? Generate CRUD entry points? (Y/n)
✔ CREATE src/users/users.module.ts
✔ CREATE src/users/users.controller.ts
✔ CREATE src/users/users.service.ts
✔ CREATE src/users/dto/create-user.dto.ts
✔ CREATE src/users/dto/update-user.dto.ts
✔ CREATE src/users/entities/user.entity.ts
✔ CREATE src/users/users.controller.spec.ts
✔ UPDATE src/app.module.ts  (imports: UsersModule)

# Go + Gin 项目
$ lan add handler user
✔ CREATE internal/handler/user_handler.go
✔ CREATE internal/service/user_service.go
✔ CREATE internal/repository/user_repository.go
! 请手动在 internal/router/router.go 注册路由：
    r.GET("/user/:id", handler.GetUser)

# 无框架（纯 Go 项目）
$ lan add component button
✔ CREATE internal/component/button.go
```

#### 4.5.8 验收标准

- [x] `lan add <generator> <name>` 一次生成多个文件，路径与命名符合框架约定。
- [x] `nest-cli.json` 存在时，`lan add resource users` 自动注册进 `app.module.ts`。
- [x] 无框架项目执行 `lan add` 时，只列出通用的 `component` / `config` 生成器。
- [x] 同名文件已存在时询问（覆盖 / 跳过 / 重命名），`--force` 直接覆盖。
- [x] 生成器表在仓库内**只有一处**定义（`lania-workflows/src/create/add/generators.rs`），
      TS 通过 bridge `addGenerator.list`（宿主 host RPC）读取。
- [x] `lan add --list` 按框架分组输出所有可用生成器。

> **B6 落地位置**：`create/add/{generators,templates,names,detect,render}.rs` +
> `create/add_workflow.rs`。注册锚点是 `// <lania:imports>`（`MarkerInsert`），
> 标记缺失时只打印可复制的代码片段，不做 AST 改写（§4.5.5）。
> 旧的平铺模板名（`rfc` / `eslint` …）保留为 `component` / `config` 生成器的 `variant`。
> 冲突处理：`--force` 直接覆盖；交互式终端逐条询问「覆盖 / 跳过 / 重命名」；
> 非交互环境（CI / 管道）保持「跳过 + 计入 `conflicts`」，不会阻塞等待 stdin。

## 5. 运行时知识单点（解决问题 2）

### 5.1 数据模型

在 `lania-runtime-manager` 新增（`language.rs` 扩展）：

```rust
pub struct RuntimeToolchain {
    pub runtime: Language,          // 复用现有 Language 枚举（已含 Node/Bun/Deno/Python/Go/Rust/Dart）
    /// 特征文件：任一存在即判定；**按数组顺序做优先级判定**
    pub feature_files: &'static [&'static str],
    /// 包管理器
    pub package_managers: &'static [PackageManager],
    /// 默认 build / dev 命令；空表示不支持
    pub build_command: Option<&'static [&'static str]>,
    pub dev_command: Option<&'static [&'static str]>,
    /// lint 工具（含"是否可进程内调用"标记）
    pub lint_tools: &'static [LintTool],
    /// 是否为 JS 簇（node/bun/deno 共用 node-bridge 进程）
    pub js_cluster: bool,
}

pub struct LintTool {
    pub id: &'static str,          // "ruff"
    pub binary: &'static str,      // "ruff"
    pub args: &'static [&'static str],
    pub in_process: bool,          // true = bridge 内可 import
    /// 该工具的执行器覆盖（bun 用 `bunx`；deno 用内置）
    pub runner: Option<&'static str>,
}
```

**双运行时项目的判定顺序**（必须固定，否则同一项目在两次运行中判定不同）：

```text
deno.json / deno.jsonc        → deno       （最高优先，因为 deno 项目常同时有 package.json）
bun.lockb / bun.lock         → bun
package.json                 → node
pyproject.toml               → python
go.mod                       → go
Cargo.toml                   → rust
pubspec.yaml                 → dart
```

判定结果**必须**通过 `diagnostic` 输出告知用户（"检测到 deno.json 与 package.json，
已选择 deno；可用 `--runtime node` 覆盖"），避免用户困惑。

**`lan.config` 的 `runtime` 字段优先级最高**（高于特征文件）：

```text
1. CLI 参数 --runtime
2. `lan.config.ts` 的 `runtime` 字段（§6.5.5）
3. 特征文件推断（本节的顺序）
4. 回退：node
```

### 5.2 消费方式

| 消费者                           | 读取方式                                                      |
| -------------------------------- | ------------------------------------------------------------- |
| Rust `create` 工作流             | 直接 `use lania_runtime_manager::toolchain`                   |
| Rust 命令 handler                | 直接调用，决定选哪个 bridge                                   |
| JS 簇 bridge（node/bun/deno）    | 通过 `toolchain.describe` bridge 方法读取一次，进程内缓存     |
| 其他 bridge                      | 同上（各自只取自己那一条）                                    |
| **`lan config sync` / `doctor`** | 直接调用（Rust 侧），用于检测配置与实际情况的一致性（§6.5.7） |

**新增 bridge 方法**：

```
toolchain.describe { runtime? } -> {
  runtimes: [{
    id, jsCluster, featureFiles, buildCommand, devCommand,
    lintTools: [{ id, binary, args, inProcess, runner }]
  }]
}
```

**同时新增配置读取方法**（Rust 侧不再自己维护候选列表，见 §6.5.3）：

```
config.describe { } -> {
  configPath: string | null,       // 实际命中的配置文件（通常是 lan.config.ts）
  searchPlaces: string[],          // 权威候选列表（7 个），Rust 直接复用
  runtime: string | null,          // 显式声明的运行时
  runtimes: { [id]: { build?, dev?, packageManager? } }
}
```

**方向明确**：`LAN_CONFIG_SEARCH_PLACES` 仍留在 `node-bridge/src/core/runtime.ts`（唯一权威），
Rust 侧 `runtime/config/loading.rs` 的**两处硬编码列表删除**，改为不传 `candidates`
（让 bridge 用默认）或从 `config.describe` 读一次缓存。

JS 簇 bridge 侧新增 `plugins/toolchain.ts`，替换 `compiler-shared.ts` 里的
`RUNTIME_FEATURE_FILES` 与 `runtime-adapters/commands.ts` 里的 `DEFAULT_COMMANDS`。

### 5.3 回落矩阵（必须实现）

运行时 bridge 不可用时的行为：

| 命令                                     | 首选                                  | 回落                       | 用户可见               |
| ---------------------------------------- | ------------------------------------- | -------------------------- | ---------------------- |
| `lint` / `build` / `dev` (node/bun/deno) | JS 簇 bridge（必有，Node 就是运行时） | —                          | —                      |
| `lint` / `build` (Python)                | `lan-py-bridge`                       | Rust 侧 `lania-exec` spawn | 无差异（事件模型相同） |
| `lint` / `build` (Go)                    | `lan-go-bridge`                       | 同上                       | 无差异                 |
| `lint` / `build` (Rust)                  | `lan-rs-bridge`                       | 同上                       | 无差异                 |
| `lint` / `build` (Dart)                  | `lan-dart-bridge`                     | 同上                       | 无差异                 |
| `add`（任意框架）                        | Rust 侧生成器（不经 bridge）          | —                          | —                      |

**为什么必须回落**：用户可能只装了 Node（`npm i -g`），没装其他 bridge 二进制。
回落路径保证「装了就更好，不装也能用」，这是分阶段发布的必要条件。

**回落实现要求**：回落路径必须复用同一套 `Diagnostic` 产出逻辑，
不能是"另一套简化输出"。否则混合场景（有 bridge 的项目 vs 没 bridge 的项目）结果不一致。

**`add` 不经过任何 bridge**：生成器表与模板渲染都在 Rust 侧与 TS 模板包内完成，
与运行时 bridge 完全解耦。这是 `add` 相比其他命令的架构优势，必须保持。

## 6. 工具链血缘（解决问题 1）

### 6.1 缺陷

`lan env use go 1.22` 只做两件事：改 `~/.runex/state.json` 的 `current`、重写 shim 文件。
但 `build` / `lint` / `create` 的子进程**继承父进程 PATH**，而 `~/.runex/shims` 从不在其中。
结果：managed 版本永远不生效（除非用户手改 shell profile）。

### 6.2 修法：入口统一前置，而非逐处注入

```rust
// lania-runtime-manager/src/home.rs（新增）
impl RuntimeHome {
    pub fn prepend_shims_to_path(&self, current: Option<&str>) -> String;
    /// 幂等：先剔除已有 shims 条目再前置
    pub fn activate(&self);
}

// lania-cli/src/main.rs（入口调用一次）
fn activate_managed_runtimes() {
    if let Ok(home) = RuntimeHome::resolve() { home.activate(); }
}
```

**为什么在入口改而不是每个 spawn 点改**：

| 方案                  | 需改文件数 | 覆盖度                                      |
| --------------------- | ---------- | ------------------------------------------- |
| 入口设置进程环境变量  | **2**      | Rust exec / bridge / compiler worker 全自动 |
| 每处 spawn 显式传 env | 6+         | 容易漏；新增 spawn 点必然忘                 |

子进程继承机制天然覆盖三层进程链（`Rust → node → compiler-worker`），无需逐层传递。

### 6.3 三条硬约束（必须写成代码注释）

| #   | 约束                                                              | 违反后果                                                    |
| --- | ----------------------------------------------------------------- | ----------------------------------------------------------- |
| 1   | `RuntimeStore::which` 只读 `state.json`，**不得回退到 PATH 扫描** | POSIX shim 是 `exec "$(lan env which ...)"`，会自引用死循环 |
| 2   | `activate()` 必须在 shims 目录**存在时**才前置                    | 把不存在目录塞进 PATH，部分工具报错                         |
| 3   | `activate()` 必须幂等（先剔除已有条目）                           | `lan agent` 等会重复调用的场景 PATH 无限增长                |

### 6.4 POSIX shim 的启动开销

POSIX shim 每次执行都要启动一次 `lan`（约 5-15ms）。影响面：

| 场景                    | 调用频次          | 影响       | 处理                                                  |
| ----------------------- | ----------------- | ---------- | ----------------------------------------------------- |
| `build` / `dev`         | 1 次/进程         | 可忽略     | —                                                     |
| `lint`（`runBinary`）   | 1 次/工具         | 可忽略     | —                                                     |
| `lint` 的 `resolveTool` | 1 次/工具（探测） | 叠加       | **保留「项目虚拟环境优先」**，只有 fallback 才走 PATH |
| Windows `.cmd` shim     | 直连绝对路径      | 无额外开销 | —                                                     |

**因此 `language-runners.ts::resolveTool` 的探测顺序必须保持**：
`.venv/Scripts/ruff.exe` → `.venv/bin/ruff` → PATH。不能为了统一而改成纯 PATH。

## 6.5 配置：统一为 `lan.config.ts`

### 6.5.1 决策

**结论：`lan.config.ts` 是唯一受支持的项目配置文件名与格式。**

保留 `.ts` 而不是改成 JSON，核心原因只有一个但足够充分：

> **配置需要能写函数**。`lan.config.ts` 目前可以导出函数、函数型 hooks、按环境分支、
> 动态 `plugins`、引用其它配置模块。这些能力改成 JSON 会**直接丢失**，
> 且没有无损替代方案。

次要是：`.ts` 有类型提示（配合 `@lania-cli-v2/cli` 的 `defineConfig`）、可写注释、
可复用项目里已有的工具函数。

### 6.5.2 现状核实：Rust 侧不解析 `.ts`，全部走 bridge

这一点决定了"保留 `.ts`"是**低风险**的 —— 因为架构上从来不需要 Rust 解析它：

```rust
// lania-host/src/runtime/config/loading.rs:45-60
pub fn load_lan_config_snapshot_from_cwd(&self, cwd: impl Into<String>) -> Result<LanConfigSnapshot> {
    // Rust 不读文件，只是把 cwd 发给 bridge
    let request = self.services.node_bridge.load_lan_config_request(cwd.into());
    let exchange = self.services.node_bridge.call(request);
    let payload = exchange.response.result.as_ref()
        .ok_or_else(|| anyhow!("config.loadLan returned no payload"))?;
    // 拿到的是 bridge 已经"求值完"的 JSON
    ConfigService::load_lan_snapshot(payload)
}
```

完整链路：

```text
Rust: load_lan_config_snapshot_from_cwd()
  └─ bridge.call("config.loadLan", { cwd, searchFrom, candidates })
       │
       ├─ TS: loadLanConfig → loadConfigModule
       │        ├─ 动态 import(lan.config.ts)   ← tsx loader 在这里
       │        ├─ 若导出函数 → await fn({})     ← 函数在这里执行
       │        └─ sanitizeConfigHooks           ← 函数型 hook 转 inline 引用
       │
       └─ 返回 JSON（函数已被求值/替换）
  └─ Rust: ConfigService::load_lan_snapshot(payload)  → LanConfigSnapshot
```

**关键结论**：Rust 拿到的永远是"已求值的 JSON"。所以"支持 `.ts` 函数"这件事，
技术上只需要保证 **bridge 侧能加载 `.ts`**，而这一点已经具备（`--import tsx`）。

### 6.5.3 真正的缺陷只有一处：候选文件名两侧不一致

现状核实：

| 侧   | 位置                                                         | 候选列表                                                         |
| ---- | ------------------------------------------------------------ | ---------------------------------------------------------------- |
| Rust | `lania-host/src/runtime/config/loading.rs`（**两处硬编码**） | `js` / `cjs` / `json` / `ts`（**4 个**）                         |
| TS   | `node-bridge/src/core/runtime.ts`                            | `js` / `cjs` / `mjs` / `json` / `ts` / `mts` / `cts`（**7 个**） |

Rust 侧不仅列表短，还是**两处重复硬编码**（`load_lan_config_snapshot_from_cwd_async`
的首次 payload 与 waterfall 的 fallback 各写一遍）。

**后果**：配置只写了 `lan.config.mts` 时，Rust 传下去的 `candidates` 不含它，
bridge 就找不到 → 报"无配置"。而如果用户直接调 bridge（其它路径），却能找到。
**同一项目两种行为**。

**修法（本题唯一必改项）**：Rust 不再自己维护列表，改为：

```rust
// 方案：Rust 传空 candidates，让 bridge 用自己的默认列表
json!({ "cwd": cwd, "searchFrom": search_from /* 不传 candidates */ })

// bridge 侧 config.ts:28-32 已支持"未传则用默认"
const candidates = Array.isArray(params.candidates) ? ... : undefined;
const loaded = await loadLanConfig(cwd, { searchFrom, candidates });  // undefined → 用 LAN_CONFIG_SEARCH_PLACES
```

**为什么这样最简**：`node-bridge/src/core/runtime.ts` 的 `LAN_CONFIG_SEARCH_PLACES`
是唯一权威来源，Rust 只负责"把 cwd 告诉它"。
`ON_CONFIG_GET` hook 仍然可以增删候选项（那是显式扩展点，保留）。

### 6.5.4 保留哪些扩展名

既然统一为 `.ts`，其余格式的处置：

| 扩展名                            | 处置                                                                        |
| --------------------------------- | --------------------------------------------------------------------------- |
| **`lan.config.ts`**               | ✅ **唯一官方格式**，文档、模板、`lan config init` 都只产出这个             |
| `lan.config.mts` / `.cts`         | ⚠️ **继续支持但不推荐**（TS 生态的自然变体，拒绝支持的收益极低）            |
| `lan.config.js` / `.cjs` / `.mjs` | ⚠️ 继续支持（向后兼容，且是 `ts` 编译产物形态）                             |
| `lan.config.json`                 | ⚠️ 继续支持（无逻辑的纯数据配置仍有价值；也是 `lan config set` 的写入目标） |

**这是"收敛"而非"砍掉"**：对外只宣传一种（`.ts`），但加载器保持 7 个候选不变，
避免破坏现有项目。与 §6.5.3 的修法一致 —— 让两侧都用同一份 7 元素列表。

**为什么不再进一步砍**：砍掉 `.json` 会让"纯数据配置"必须包一层 `.ts`；
砍掉 `.mts` 会让 ESM 项目报错。收益低、风险高。

### 6.5.5 `runtime` / `runtimes` 必须提升为强类型

这是与本决策**独立**但必须一起做的一件事。实测 `LanConfigSnapshot` 里没有这两个字段：

```rust
pub struct LanConfigSnapshot {
    pub build_tool: String,          // ← 只有 buildTool 被结构化
    pub lint_tools: Vec<String>,
    // ... 没有 runtime，没有 runtimes
    pub raw: Value,                  // ← runtime 只能从这里摸
}
```

而 `validate_lan.rs` 白名单里**有** `runtime` / `runtimes`（说明是合法字段），
`tests.rs` 也只断言"不报错"。

TS 侧唯一真正消费它的地方是 `runtime-adapters/commands.ts:24`：
`asRecord(context.lanConfig.runtimes)[runtime][action]`。

**目标结构**：

```rust
pub struct LanConfigSnapshot {
    // ...
    /// 显式声明的运行时；None 表示按特征文件自动探测
    pub runtime: Option<Language>,
    /// 每个运行时的 build/dev 命令覆盖
    pub runtimes: BTreeMap<String, RuntimeCommandOverride>,
}

pub struct RuntimeCommandOverride {
    pub build: Option<Vec<String>>,
    pub dev: Option<Vec<String>>,
    pub package_manager: Option<PackageManager>,
}
```

**校验要求**（`validate_sections.rs`）：`runtimes.<id>.build` 必须是 `string[]`；
未知运行时 id 报 **warning 而非 error**（前向兼容未来运行时）。

**运行时判定优先级**（与 §5.1 一致）：

```text
1. CLI 参数 --runtime
2. lan.config.ts 的 runtime 字段
3. 特征文件推断（deno.json > bun.lockb > package.json > pyproject.toml > go.mod > Cargo.toml > pubspec.yaml）
4. 回退 node
```

### 6.5.6 配置结构示例（`defineConfig`）

```typescript
// lan.config.ts
import { defineConfig } from '@lania-cli-v2/cli';

export default defineConfig({
  runtime: 'go',

  runtimes: {
    go: { build: ['go', 'build', '-o', 'bin/app', '.'] },
  },

  lintTools: ['golangci-lint'],

  hooks: {
    // 函数型 hook 保持原样 —— 这是保留 .ts 的核心理由
    onBuild: (ctx) => ({ ...ctx, extraArgs: ['--verbose'] }),
  },

  plugins: [require.resolve('./lania.plugin.ts')],
});
```

也可以导出函数（按环境分支）：

```typescript
export default ({ env }: { env: string }) =>
  env === 'prod'
    ? { runtime: 'go', lintTools: ['golangci-lint'], release: { profile: 'package' } }
    : { runtime: 'go', lintTools: ['golangci-lint'] };
```

### 6.5.7 函数式配置的求值上下文（新增）

现状 `core/runtime.ts:242` 传的是**空对象**：

```typescript
const resolved = typeof candidate === 'function' ? await candidate({}) : candidate;
```

于是"按环境分支"这类配置**写了也没用** —— 拿不到 `env`，只能返回同一个结果。
这是函数式配置现有能力的一个真实缺口（不是本次改动引入的）。

**决策：扩展求值上下文，但保持向后兼容。**

```typescript
export interface ConfigContext {
  /** 当前环境名；来自 LANIA_ENV，缺省 "development" */
  env: string;
  /** 配置文件所在目录的绝对路径 */
  cwd: string;
  /** 当前执行的命令名（如 "build" / "lint"）；配置预加载阶段为空串 */
  command: string;
}

const resolved = typeof candidate === 'function' ? await candidate(configContext) : candidate;
```

**兼容性**：

| 现有配置写法                    | 扩展后行为                               |
| ------------------------------- | ---------------------------------------- |
| `export default { ... }`        | 不变                                     |
| `export default () => ({...})`  | 不变（忽略参数）                         |
| `export default ({env}) => ...` | **从"拿到 undefined"变为"拿到真实 env"** |

第三行是**行为变化**：之前 `env` 永远是 `undefined`（函数走 `if (env === 'prod')` 永远为假），
现在会拿到真实值。这对"本来就想按环境分支"的用户是修复；对"依赖 `undefined` 巧合工作"的
配置是行为变化 —— 但那种配置本质上是坏的，且规模可忽略。

**`env` 的来源**（按优先级）：

```text
1. CLI 参数 --env <name>
2. 环境变量 LANIA_ENV
3. 缺省 "development"
```

**注意：不读 `NODE_ENV`**。原因：Go / Rust / Python / Dart 项目根本没有 `NODE_ENV`，
用它会让配置行为随运行时变化，违背"运行时中立"。

**`command` 字段的处理**：配置在命令执行前就要加载（用于决定走哪个 bridge、哪些 hook），
此时命令名已知（`argv[0]`），可以填。但**配置若依赖 `command` 做不同返回，
会导致同一进程内不同命令看到不同配置** —— 这是允许的（用户显式选择），
但要在文档中警告：`hooks` 与 `runtime` 不要依赖 `command`。

**不做的事**：不给 `ConfigContext` 加 `runtime` / `packageManager` 等"探测结果"字段。
原因：那会让配置加载依赖探测，而探测又可能依赖配置（循环）。`env` / `cwd` / `command`
是"输入"而非"探测结果"，安全。

### 6.5.8 写入能力（受 `.ts` 限制）

因为配置是可执行 TS，**不能像 JSON 那样安全地程序化改写**。因此：

| 子命令                      | 行为                                                                    |
| --------------------------- | ----------------------------------------------------------------------- |
| `lan config init`           | ✅ 创建 `lan.config.ts`（含 `defineConfig`）                            |
| `lan config doctor`         | ✅ 只读诊断，不改文件                                                   |
| `lan config set <path> <v>` | ⚠️ **只在纯数据配置（无函数/无 import）时可用**；否则打印建议片段不写入 |
| `lan config sync`           | ⚠️ 同上；输出"建议补充的内容"，由用户手动粘贴                           |

**判定"纯数据配置"的方法**：加载时若 `config` 对象可 `JSON.stringify` 无损往返，
且源文件不含 `import` / `export default (`.\*`=>` / `function`，则视为纯数据，可安全改写。

**这是保留 `.ts` 的代价**：放弃"工具自动改写配置"的能力。
但换来的是"函数配置可用"，权衡上更合理 —— 配置是低频修改、手动编辑可接受；
而函数能力一旦不能表达，用户只能去写 hook 脚本，反而更绕。

### 6.5.9 与 TS 侧加载器的对齐项

| 项                 | 现状                                                         | 目标                                                 |
| ------------------ | ------------------------------------------------------------ | ---------------------------------------------------- |
| 候选列表权威来源   | Rust 与 TS 各有一份                                          | **只有 TS 一份**，Rust 传 `undefined`                |
| `tsx` 依赖         | bridge 启动时 `--import tsx`                                 | 不变（`.ts` 配置与 `.ts` 模板都需要）                |
| 函数求值           | `typeof candidate === 'function' ? await fn({}) : candidate` | **改为 `await fn({ env, cwd, command })`**（§6.5.7） |
| 函数型 hook 跨进程 | `sanitizeConfigHooks` → `{type:'inline', id}`                | 不变                                                 |
| 加载失败的行为     | 返回空对象 `{}`（静默）                                      | **改为明确报错**（见下）                             |

**"加载失败静默返回空对象"必须改**：现状 `loadStructuredConfig` 解析失败时 `return {}`，
`loadConfigFile` 的 `import` 抛错也没被捕获。后果是配置写错时命令"看起来正常"但行为全错
（典型：`lintTools` 丢失 → lint 什么都没跑却报成功）。

改为：加载失败 → `validation_errors` 里加一条 `ConfigValidationError`，
并在 CLI 层以 warning 显示；**不阻断命令**（保持容错），但用户能看到。

### 6.5.10 验收标准

- [ ] Rust 侧不再硬编码 `lan.config.*` 候选列表；`searchFrom` 之外的列表来自 bridge。
- [ ] `lan.config.mts` / `.cts` 在 Rust 与 bridge 两条路径下都能被找到（行为一致）。
- [ ] `lan.config.ts` 导出函数时，返回值被正确求值并成为配置。
- [ ] **函数式配置能拿到 `{ env, cwd, command }`**；`LANIA_ENV=prod` 时 `({env}) => ...` 返回 prod 分支。
- [ ] 函数型 hook（`hooks.onBuild: (ctx) => ...`）在 `.ts` 配置下正常工作。
- [ ] `LanConfigSnapshot.runtime` / `.runtimes` 是强类型字段，可直接读取。
- [ ] `.ts` 配置加载失败（语法错误 / import 失败）时，`validation_errors` 有明确条目，CLI 显示 warning。
- [ ] `lan config set` 在检测到函数/import 时**不写入**，只打印建议片段。
- [ ] `lan config init` 生成的模板使用 `defineConfig` 且有类型提示。

## 7. 日志劫持与统一渲染（解决问题 5）

### 7.1 现状实测：日志已经"被劫持"，但劫持得不对

先纠正一个直觉：**日志现在其实已经被劫持了**，只是劫持的结果被压平成了字符串，信息全丢。

| 日志来源                   | 当前捕获点                                          | 变成什么                                                             | 丢失的信息                         |
| -------------------------- | --------------------------------------------------- | -------------------------------------------------------------------- | ---------------------------------- |
| 工具链子进程 stdout        | `compiler-worker-client.ts::attachStreamCapture`    | `logEvent(line, 'info')` → `event.log`                               | 来源（哪个工具）、是否结构化       |
| 工具链子进程 stderr        | 同上                                                | `compilerIssueEvent(tool, 'warning', line)` → `event.compiler_issue` | **把任意 stderr 都当 warning**     |
| ESLint / mypy 等结构化结果 | `createLintRunResult`                               | `event.lint_file`（只有 `filePath/errors/warnings`）                 | 行号、列号、规则名、消息、修复建议 |
| 服务端运行期日志           | **完全没有捕获**（`compiler_dev` 返回后无法再透出） | —                                                                    | 全部丢失                           |

Rust 侧的消费同样压平：`bridge_helpers/events.rs` 把每类事件都转成一句
`self.log(level, "command_execute", ..., format!("..."))`。于是：

```rust
// 现状：信息在两层被压平
// 1) TS：{ filePath, line: 12, rule: 'E501', message: '...' } → "ruff file src/a.py: 3 errors"
// 2) Rust：format!("{adaptor} file {path}: {errors} errors, {warnings} warnings")
```

**结论：不需要新增"劫持能力"，需要新增"保真通道"。** 这是本节的立足点。

### 7.2 R1：劫持模型（统一到一条结构化日志通道）

新增一个**唯一的日志事件**，替代 `event.log` 的语义（保留 `event.log` 作为兼容别名）：

```jsonc
// event.diag  —— 所有"被劫持的终端输出"都走这一条
{
  "method": "event.diag",
  "params": {
    "source": "tool" | "runtime" | "host",      // 谁产生的
    "stream": "stdout" | "stderr" | "syslog",   // 原始终端流
    "tool": "go" | "cargo" | "vite" | "mypy",   // 具体工具（source=tool 时必有）
    "channel": "build" | "dev" | "lint" | "app",// 用途；app = 应用自身日志
    "level": "trace" | "debug" | "info" | "warn" | "error",
    "text": "compiling foo v0.1.0",             // 原始文本（未加工）
    "seq": 42,                                  // 单调递增，保证跨流合并顺序
    "ts": 1730012345678,                        // 毫秒时间戳
    "fields": { "file": "src/main.rs", "line": 10 }  // 可选的已解析字段
  }
}
```

**`seq` 是关键**：`stdout` 与 `stderr` 是两个独立异步流，不合并会乱序。
每个 bridge 维护一个原子计数器，写事件时自增。

**`channel` 是关键**：它让"构建日志"和"服务端接口日志"能分开渲染/过滤。
现有 `event.log` 没有这个概念，导致 `lan dev` 的输出里
`VITE ready` 与 `GET /api/user 200` 混在一起无法区分。

**各 bridge 的劫持实现**（必须一致）：

| 运行时     | 劫持方式                                                                         | 注意                                            |
| ---------- | -------------------------------------------------------------------------------- | ----------------------------------------------- |
| node / bun | `spawn` 时 `stdio: ['ignore','pipe','pipe']`，`readline` 按行切分 → `event.diag` | 已有 `createLineEmitter`，改造成发 `event.diag` |
| deno       | 同 node（`deno lint` / `deno task` 都是子进程），但需 `DENO_NO_UPDATE_CHECK=1`   | 否则 `deno` 首次运行会打印升级提示污染 stdout   |
| Python     | `subprocess.Popen(stdout=PIPE, stderr=PIPE)` + 两线程按行读 → `event.diag`       | 必须 `bufsize=1` + `text=True`，否则攒包        |
| Go         | `cmd.StdoutPipe()` / `cmd.StderrPipe()` + `bufio.Scanner` → `event.diag`         | `Scanner` 默认 64KB 上限，长行要 `Buffer` 放大  |
| Rust       | 复用 `lania-exec` 的 `ExecEvent`（已按行切分）→ `event.diag`                     | 无需改动捕获层                                  |
| Dart       | `Process.start()` + `stdout.transform(utf8.decoder).transform(LineSplitter())`   | Dart 的 `dart run` 输出可能无换行，需超时 flush |

**禁止事项**（写进 code review checklist）：

| #   | 禁止                                                           | 理由                                            |
| --- | -------------------------------------------------------------- | ----------------------------------------------- |
| 1   | 把 stderr 一律映射成 `warning`                                 | 现状缺陷；stderr 里可能是 error，也可能是进度条 |
| 2   | 在 bridge 内 `console.log` / `print` / `fmt.Println` 写 stdout | 污染 jsonl 协议                                 |
| 3   | 未解析出 `level` 时默认 `info`                                 | 应在 `channel=build` 时按工具约定推断           |

### 7.3 R2：统一结果模型（三层）

日志与结果是两件事，必须分开建模：

```text
① 原始流（event.diag）        —— 逐行，保真，可能有噪声
        │  桥内解析
        ▼
② 结构化诊断（Diagnostic）    —— 去重、带位置、带规则
        │  宿主聚合
        ▼
③ 统一结果（RunResult）      —— 供渲染与 JSON 输出
```

**② `Diagnostic`（跨语言统一，所有 bridge 必须产出）**：

```jsonc
{
  "tool": "ruff",
  "id": "E501",                      // 规则号/检查名
  "severity": "error" | "warning" | "info" | "hint",
  "message": "line too long (95 > 88)",
  "file": "src/main.py",             // 相对 cwd
  "range": { "start": {"line":12,"col":5}, "end": {"line":12,"col":20} },
  "url": "https://docs.astral.sh/ruff/rules/line-too-long",
  "fix": { "available": true, "kind": "safe" | "unsafe" | "none" }
}
```

字段可选性约定：`file` / `range` 缺失表示"文件级诊断"（如 `cargo clippy` 的 crate 级告警）；
`id` 缺失表示"无规则号的通用错误"（如编译失败）。

**现有各解析器 → `Diagnostic` 的映射工作量**：

| 工具             | 现状产出                                                         | 需要补的字段                                                              | 难度                |
| ---------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------- | ------------------- |
| ruff（JSON）     | `filename`                                                       | `code` / `location` / `message` / `fix`                                   | 低（JSON 已含全部） |
| golangci（JSON） | `Pos.Filename`                                                   | `FromLinter` / `Text` / `Pos.Line` / `Replacement`                        | 低                  |
| clippy（JSON）   | `spans[0].file_name`                                             | `code.code` / `message` / `spans[0].line_start` / `suggested_replacement` | 低                  |
| mypy（文本）     | `file:line: severity:`                                           | 全部（需正则解析）                                                        | 中                  |
| flake8（文本）   | `file:line:col: code`                                            | `message`                                                                 | 中                  |
| go vet（文本）   | `file.go:line:col:`                                              | `message`                                                                 | 中                  |
| dart analyze     | `severity\|type\|code\|file\|line\|col\|len\|msg`                | `code` / `message`（已含）                                                | 低                  |
| ESLint           | `errorCount` / `warningCount`                                    | 全部（ESLint API 直接给 `messages[]`）                                    | 低                  |
| oxlint / oxfmt   | 文本                                                             | 全部                                                                      | 中                  |
| **deno lint**    | `--json`（v2+）：`{diagnostics:[{filename,range,code,message}]}` | v2 全部可得；**v1 无 `--json`，需正则**                                   | 低（v2）/ 中（v1）  |
| **deno fmt**     | `--check` 仅退出码 + 文件名列表                                  | `file` / `fix.available`（无行列）                                        | 低                  |
| **bun build**    | 文本 + 退出码                                                    | 全部（需解析 bun 的构建错误格式）                                         | 中                  |

**③ `RunResult`（宿主侧统一）**：

```rust
// lania-host 新增（不改现有 CommandExecution 枚举）
pub struct RunResult {
    pub kind: RunKind,                  // Lint | Build | Dev | App
    pub status: RunStatus,              // Success | Failed | Partial
    pub started_at: SystemTime,
    pub duration: Duration,
    pub diagnostics: Vec<Diagnostic>,   // 去重 + 排序后
    pub counts: DiagCounts,             // {error, warning, info, hint}
    pub logs: Vec<DiagLine>,            // 原始流（受 --verbose 控制是否输出）
    pub artifacts: Vec<Artifact>,       // 产物文件（build）
}
```

**`diagnostics` 的去重与排序规则**（必须确定，否则多工具叠加时输出不稳定）：

1. 去重键：`(tool, file, range.start.line, range.start.col, id)`。
2. 排序：`file` 升序 → `line` 升序 → `col` 升序 → `severity`（error 优先）→ `id`。
3. 同一位置多个工具报警时**全部保留**（不合并），因为在渲染层用不同 `tool` 标签区分。

### 7.4 R3：lint 结果统一格式与高亮

**渲染分两种 profile**，由现有 `OutputProfile`（`lania-cli/src/profile.rs`）决定：

#### 7.4.1 Human profile（TTY）

```text
src/main.py
  ✖ 12:5   error    E501   Line too long (95 > 88)              ruff  [fix]
  ⚠ 20:1   warning  W291   Trailing whitespace                  ruff  [fix]
  ✖  5:10  error    E0602  Undefined name `foo`                 mypy

src/api.go
  ✖ 33:2   error    errcheck  Error return value not checked    golangci-lint

Summary: 3 errors, 1 warning in 4 files (2.1s)
  ruff 2 · mypy 1 · golangci-lint 1
```

**高亮方案**（不引入新依赖，复用现有 `lania-presentation` 的 `TextStyle` / `TextColor`）：

| 元素             | 样式                                   |
| ---------------- | -------------------------------------- |
| 文件路径         | `bold`                                 |
| 行列号           | `dim`                                  |
| `error`          | `Red` + `bold`                         |
| `warning`        | `Yellow`                               |
| `info` / `hint`  | `Blue` / `dim`                         |
| 规则号 `id`      | `Cyan`                                 |
| 消息正文         | `Default`                              |
| 工具名           | `Magenta` + `dim`                      |
| `[fix]` 标记     | `Green`                                |
| 源码片段（可选） | `dim` + 行号 `dim`，命中区间 `inverse` |

**关键约束**：颜色只在 `stdout.is_terminal()` 时启用，且遵守现有 `NO_COLOR` / `--color=never` 约定
（`lania-presentation` 已有 `IsTerminal` 判断，直接用）。

#### 7.4.2 JSON profile（CI / 管道）

```jsonc
{
  "kind": "lint",
  "status": "failed",
  "counts": { "error": 3, "warning": 1, "info": 0, "hint": 0 },
  "diagnostics": [
    /* Diagnostic[]，已去重排序 */
  ],
  "durationMs": 2140,
}
```

**这是对现有输出的破坏性变更**：当前 `lan lint` 的 JSON 只有
`summaryByAdaptor` / `resultsByAdaptor`。迁移策略见 §7.6。

#### 7.4.3 分组成员：为什么 lint 需要一个专门的渲染器

现有 `LogRenderer` 只有 `render_entry(&LogEntry) -> String`，**一次一行字符串**，
无法表达"按文件分组 + 对齐表格 + 组间空行"。

因此新增独立渲染器（不改 `LogRenderer` trait）：

```rust
// lania-presentation 新增
pub struct LintReportRenderer { /* locale, color, max_width */ }
impl LintReportRenderer {
    /// 按 file 分组 → 组内按 (line, col, severity) 排序 → 对齐成列
    pub fn render(&self, result: &RunResult) -> Vec<String>;
}
```

`build` / `dev` 复用同一 `RunResult` 但用不同渲染器：
`BuildReportRenderer`（产物 + 错误摘要 + 耗时）。
**共用 `Diagnostic`，不共用布局**——这是分层的价值。

### 7.5 R4：项目运行期日志（服务端接口日志）

这是现状**完全缺失**的能力，也是最容易误解的一环。

#### 7.5.1 问题本质

`lan dev` 启动服务后，`compiler_dev` 请求返回，Rust 侧的 `apply_bridge_events`
只在**请求处理期间**消费事件。服务端之后打印的 `GET /api/user 200 12ms` 属于
**应用自身 stdout**，它不在工具链 stdout 里，而在**被启动的应用进程**的 stdout 里。

分三种情况：

| 应用形态                                 | 日志来源                                               | 劫持方式                                                          |
| ---------------------------------------- | ------------------------------------------------------ | ----------------------------------------------------------------- |
| node/bun/deno（vite/express/nest/next）  | 应用与 dev server 同进程 → 其 stdout 就是工具链 stdout | 现有 `attachStreamCapture` 已覆盖，标记 `channel: "app"` 即可     |
| Go / Rust / Python / Dart（`exec` 启动） | 独立子进程                                             | bridge 内 `exec` 时同样 pipe stdout/stderr，标记 `channel: "app"` |
| 容器 / 外部服务                          | 不在我们进程树里                                       | **不做**（明确不接管）                                            |

**关键设计：区分 `channel: "build"` 与 `channel: "app"`，但共用 `event.diag` 通道。**

#### 7.5.2 长驻订阅：不让 `compiler_dev` 返回就断流

现状 `compiler_dev` 返回后，`activeEmit` 仍保留（`compiler.ts` 有注释说明），
但 Rust 侧 `apply_bridge_events` 只在 exchange 上跑一次。需要：

```
方案：dev 命令进入"订阅模式"
  1. Rust 发 compiler_dev → 拿到 response（含 url）
  2. Rust 侧开始循环 call.next_event()，不设总超时（只设 idle 心跳超时）
  3. 后续所有 event.diag（channel=app）/ compiler_* 都在这个循环里实时渲染
  4. Ctrl-C 时发 compiler_stop，等 done 后退出
```

**已在代码中具备基础**：`BridgeActiveCall::next_event()` 已存在，
`commands/lint` 等已用边收边渲染。**不需要新协议**，只需要 dev handler 用 `open_call` 而非 `call_async`。

#### 7.5.3 渲染策略：`app` 日志要与构建日志视觉分离

```text
[dev] building...  ← channel=build
[dev] ✔ built in 1.2s
[dev] ➜ http://localhost:5173

[app] GET  /api/user      200  12ms   ← channel=app
[app] POST /api/order     201  34ms
[app] WARN slow query: 1.2s
```

- `app` 前缀用 `dim`，`build` 前缀用 `Cyan`。
- `app` 日志**默认开启**（否则用户看不到），但可 `--quiet-app` 关闭。
- **不做结构化解析**：HTTP 访问日志格式因框架而异（Express morgan / Koa / Go chi / axum），
  强行解析会做成"半成品"。v1 只做**逐行透传 + 前缀标记**，
  结构化留给用户配置 `devLogParser`（见 §7.5.4）。

#### 7.5.4 可选扩展点（v1 不实现，仅预留）

```typescript
// lan.config.ts
export default defineConfig({
  dev: { logParser: './scripts/parse-log.mjs' },
});
```

parser 是一个 `(line: string) => Diagnostic | null` 的纯函数，由 Node bridge 加载。
**v1 只留配置位，不实现**——先验证"透传 + 前缀"够不够用。

#### 7.5.5 落地说明（B5）

- **长驻订阅**：Rust 侧 `maybe_wait_for_shutdown()` 在 `compiler.dev` 应答后用
  `BridgeActiveCall::next_event()` 持续消费事件，直到 Ctrl-C 或应用退出；
  期间 stderr 行（`pump_process_stderr` 广播）一并进入同一渲染路径。
- **实时性修复**：`compiler-worker-client` 之前把 worker stdout/stderr 只放进
  `bufferedEvents`（要等 stop 才排出），dev 期间的应用日志因此延迟。
  现在有 `emit` 时逐行立即发出，并且逐行改发 `event.diag`（`channel: "build"`）。
- **视觉分离**：`event.diag` 渲染为 `[app] …` / `[build] …` / `[dev] …` / `[lint] …` 前缀。
- **开关**：`ui.dev.quietApp: true` 或 `lan dev --quiet-app` 关闭 `channel: "app"` 行。

### 7.6 迁移与兼容

| 变更                                 | 兼容策略                                                                           |
| ------------------------------------ | ---------------------------------------------------------------------------------- |
| 新增 `event.diag`                    | 新增，不影响旧事件                                                                 |
| `event.log`                          | 保留；bridge 内部改为优先发 `event.diag`，`event.log` 仅用于 host 级消息           |
| `event.compiler_issue` 的 `severity` | 不再把 stderr 一律填 `warning`；按内容推断，无法推断时填 `info`                    |
| `lan lint --json` 输出结构           | **破坏性**：新增 `diagnostics` 数组；保留旧字段一版（deprecated），下个 major 移除 |
| `lan build --json` 输出结构          | 同上                                                                               |
| `LogRenderer` trait                  | **不改**。新渲染器独立实现，`bridge_helpers/events.rs` 分流调用                    |

**为什么保留旧 JSON 字段一版**：`lan lint --json` 可能已被 CI 脚本消费。
changeset 标 `minor`（新增）而非 `major`，同时把旧字段标 `deprecated` 并在文档写明移除时间。

### 7.7 验收标准（问题 5）

- [ ] `lan lint` 在 TTY 下按文件分组、列对齐、按 severity 高亮；`NO_COLOR=1` 时无 ANSI 转义。
- [ ] `lan lint --json` 的 `diagnostics[]` 含 `tool` / `id` / `severity` / `message` / `file` / `range`。
- [ ] `lan build` 的 stderr 不再一律显示为 warning；真实 error 显示为 error。
- [ ] `lan dev` 启动后，服务端新产生的 stdout 行**实时**出现在终端，带 `[app]` 前缀。
      （已落地为 `event.diag` 实时转发 + `[app]` / `[build]` / `[dev]` 前缀渲染，见 §7.5.5；
      `event.diag.seq` 由单测保证严格递增。）
- [ ] `lan dev` 输出的 `event.diag` 的 `seq` 严格递增（stdout/stderr 合并无乱序）。
- [ ] 四种 bridge（Node/Python/Go/Rust）对同一份 ruff 输出产出**完全相同**的 `Diagnostic`。
- [ ] `lan lint --json | jq '.diagnostics | length'` 在 CI 无 TTY 环境下与 TTY 一致。

## 8. 包结构与发行

### 8.1 npm 包结构（问题："仍要发一个 npm 包"）

```text
@lania-cli-v2/cli
├── bin/lan.mjs                          Node 入口（已有）
├── lib/node-bridge/                     TS bridge（已有）
├── lib/py-bridge/lan_py_bridge.py       ★新增：纯文本，零平台包
└── optionalDependencies:
    ├── @lania-cli-v2/cli-win32-x64      Rust CLI（已有）
    ├── @lania-cli-v2/cli-linux-x64      Rust CLI（已有）
    ├── @lania-cli-v2/cli-darwin-arm64   Rust CLI（已有）
    ├── @lania-cli-v2/go-bridge-win32-x64   ★新增
    ├── @lania-cli-v2/go-bridge-linux-x64   ★新增
    └── @lania-cli-v2/go-bridge-darwin-arm64 ★新增
```

**平台包数量变化（七运行时全量）**：

| 项           | 现状      | + Python（脚本） | + Go  | + Rust | + Dart |
| ------------ | --------- | ---------------- | ----- | ------ | ------ |
| 平台包数     | 3         | **3（不变）**    | 6     | 9      | 12     |
| 总体积（估） | ~56MB     | ~56MB            | ~75MB | ~110MB | ~130MB |
| 需交叉编译   | Rust only | Rust only        | +Go   | +Rust  | +Rust  |

**各 bridge 的产物形态**：

| bridge                  | 产物形态                               | 平台包                    |
| ----------------------- | -------------------------------------- | ------------------------- |
| `node`（node/bun/deno） | `@lania-cli-v2/node-bridge`（TS dist） | 不需要（随 CLI lib 分发） |
| `python`                | 单文件 `.py` 脚本                      | **不需要**                |
| `go`                    | 编译成二进制                           | 3 个                      |
| `rust`                  | 编译成二进制                           | 3 个                      |
| `dart`                  | 编译成二进制（Dart 源码构建期编译）    | 3 个                      |

**因此全量落地的 npm 平台包是 12 个**。这个数字决定了 §10 里的"必须支持回落"不是可选项：
用户装 CLI 时只装了 3 个（Rust CLI），其余 9 个可选依赖可能因网络/平台缺失，
回落路径是可用性兜底。

> 💡 建议的发布节奏：先只发 `node` + `python`（3 个平台包不变），
> 观察实际需求再决定是否为 Go/Rust/Dart 增加平台包。这是 §11 里程碑的排期依据。

### 8.2 `pack.mjs` 改动

`stageNodeBridgePayload()` 需增加：

```text
npm/cli/lib/py-bridge/lan_py_bridge.py     ← 从 rust/crates/lania-py-bridge/assets 复制
```

Go / Rust / Dart bridge 二进制通过 `optionalDependencies` 分发，`pack.mjs` 的
`stagePlatformBinaries()` 需要泛化（当前只处理 `lania-cli` 一个二进制）。

**必须新增的校验**（现有 `assertVersionSync` 与 `assertStagedBinaryVersion` 已建立模式）：

- 12 个平台包的 version 与 Rust workspace 版本一致；
- 宿主平台的每个 bridge 二进制都能 `--version` 自报版本且匹配；
- 缺失的 bridge 平台包**不报错**，只在 pack 日志里记录（因为它是 optional）。

## 9. 构建与 CI

### 9.1 交叉编译矩阵

| 产物              | win32-x64 | linux-x64 | darwin-arm64 | 工具链要求                           |
| ----------------- | --------- | --------- | ------------ | ------------------------------------ |
| `lania-cli`       | ✅        | ✅        | ✅           | 已有（`release-binaries.yml`）       |
| `lan-go-bridge`   | 需加      | 需加      | 需加         | `GOOS`/`GOARCH` 交叉编译，简单       |
| `lan-rs-bridge`   | 需加      | 需加      | 需加         | 同 Rust CLI                          |
| `lan-dart-bridge` | 需加      | 需加      | 需加         | `dart compile exe`（需宿主平台 SDK） |

**三个额外约束**：

| #   | 约束                                                              | 原因                                      |
| --- | ----------------------------------------------------------------- | ----------------------------------------- |
| 1   | `lan-go-bridge` **不得依赖 CGO**                                  | 否则交叉编译需要各平台 C 工具链           |
| 2   | `lan-dart-bridge` **无法真正交叉编译**（Dart AOT 需目标平台 SDK） | 只能在各平台自建；CI 需 3 个 runner       |
| 3   | 所有 bridge 必须能响应 `bridge.ping` **且不启动任何工具链**       | 否则 health check 会因缺少 go/dart 而失败 |

> ⚠️ 约束 2 是 Dart bridge 的主要成本。如果 CI 只有 linux runner，
> Dart bridge 只能先发 linux 版本，其余平台走回落。

### 9.2 必须新增的 CI 检查

| 检查项                                                           | 目的                  |
| ---------------------------------------------------------------- | --------------------- |
| 各 bridge 在 3 平台能编译并响应 `bridge.ping`                    | 交叉编译可用性        |
| 移除某运行时 bridge 后 `lan build` / `lint` 仍通过（回落路径）   | §5.3 回落矩阵真实可用 |
| `activate()` 单测：幂等 + 前置顺序 + shims 不存在时跳过          | §6.3 约束             |
| `lan env use` 后同 shell 内 `lan build` 使用 managed 版本        | 问题 1 的端到端验收   |
| `lan create --template go-service` 在无 bridge 环境可用          | 问题 3 的端到端验收   |
| **deno 项目判定**：同时有 `deno.json` + `package.json` 时选 deno | §5.1 判定顺序         |
| **bun 项目判定**：有 `bun.lockb` 时选 bun                        | §5.1 判定顺序         |
| **配置候选文件名两侧一致**：Rust 与 TS 都能发现 `.mts` / `.cts`  | §6.5.1 断层 1         |
| **`lan.config` 声明 `runtime` 时优先于特征文件**                 | §5.1 优先级           |
| **`lan add resource users`** 在 nestjs 项目生成 7 文件 + 1 修改  | §4.5.8 验收           |
| **`lan add` 在无框架项目只列通用生成器**                         | §4.5.8 验收           |

## 10. 风险

| 风险                                           | 影响 | 处理                                                                              |
| ---------------------------------------------- | ---- | --------------------------------------------------------------------------------- |
| **多 bridge 生命周期管理**（N 套崩溃恢复）     | 高   | 复用 `NodeBridgeClient` 的 `fail_fast` / `reconnect`，不重写                      |
| **跨层进程组 kill**（`Rust→node→worker→tool`） | 高   | 每层 POSIX 设 `detached`；Windows 用 Job Object（现有未做，需补）                 |
| **stdio 污染**（Go `log` / Python `print`）    | 中   | 各 bridge 启动时重定向 stdout/stderr，协议写原始 fd                               |
| **npm 平台包膨胀**（3 → 9）                    | 中   | 语言 bridge 用 `optionalDependencies`；缺失时回落                                 |
| **Rust bridge 零收益**                         | 中   | 允许砍掉，见 §3.4 备注                                                            |
| **诊断结构扩展破坏消费者**                     | 低   | 只新增字段，changeset 标 `minor`                                                  |
| **shim 自引用**                                | 高   | §6.3 约束 1 写成代码注释 + 单测                                                   |
| **`event.diag` 的 `seq` 跨流乱序**             | 中   | 每 bridge 单计数器；双流读线程共享同一 writer 并加锁                              |
| **stderr 被误判为 warning**（现状继承）        | 中   | 按内容启发式推断；无法推断时降为 `info`，不再冒充 warning                         |
| **`app` 日志量过大淹没构建日志**               | 中   | 默认全量；`--quiet-app` 关闭；不做采样，避免误导                                  |
| **长驻 dev 事件订阅泄漏**                      | 中   | `open_call` 绑定 `CancellationToken`；Ctrl-C / shutdown 必关                      |
| **`lan lint --json` 旧字段被 CI 依赖**         | 中   | 保留一版并标 deprecated；文档写明移除版本                                         |
| **行缓冲导致日志延迟**（Python `bufsize`）     | 低   | 各 bridge 显式设行缓冲 / `PYTHONUNBUFFERED=1`                                     |
| **`lan.config` 候选列表两侧不一致**            | 高   | §6.5.3：Rust 不再硬编码，改为 bridge 单点权威                                     |
| **`lan.config.ts` 加载失败被静默吞掉**         | 高   | §6.5.8：解析/import 失败写入 `validation_errors` 并显示 warning                   |
| **`config set` 改写 `.ts` 破坏用户代码**       | 中   | §6.5.8：检测到函数/import 时**不写入**，只打印建议片段                            |
| **函数求值上下文扩展导致配置行为变化**         | 中   | §6.5.7：仅 `({env})=>…` 的配置受影响（原本 `env` 恒为 `undefined`，本质是坏配置） |
| **模板选项三处维护继续漂移**                   | 中   | 先加 CI 检查：`language-options.ts` 的运行时 id 必须存在于 `RuntimeToolchain`     |

## 11. 里程碑

| 阶段 | 内容                                                                                                                      | 验收                                                      |
| ---- | ------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| B0   | `activate()` + 入口调用 + `doctor` 调整（§6）                                                                             | `lan env use` 后 `lan build` 用 managed 版本              |
| B1   | `bridge_launch_spec` 参数化 + `BridgeRegistry` 骨架（§2.2）                                                               | 可启动任意 `program` 说 jsonl                             |
| B2   | `toolchain.describe` 单点 + JS 簇 bridge 改读（§5）                                                                       | 删除 `RUNTIME_FEATURE_FILES` / `DEFAULT_COMMANDS`         |
| B2.5 | **配置统一**：Rust 候选去重 + `runtime`/`runtimes` 强类型 + 加载失败可见 + **函数求值上下文 `{env,cwd,command}`**（§6.5） | 两侧行为一致；`({env}) => …` 能分出 prod 分支             |
| B3   | `event.diag` 协议 + 劫持实现 + `RunResult`/`Diagnostic`（§7.2、§7.3）                                                     | 同一工具在两个 bridge 产出相同 `Diagnostic`               |
| B4   | `LintReportRenderer` + 高亮 + `--json` 新结构（§7.4）                                                                     | TTY 分组对齐高亮；`NO_COLOR` 下无转义                     |
| B5   | dev 长驻订阅 + `channel: "app"` 前缀渲染（§7.5）                                                                          | 服务端新日志实时出现且带 `[app]`                          |
| B6   | **`add` 生成器重构**：框架探测 + 命名推导 + 多文件产物 + NestJS 首个落地（§4.5）                                          | `lan add resource users` 生成 7 文件并注册进 `app.module` |
| B7   | `add` 扩展：nextjs / gin / fastapi / axum / flutter 生成器                                                                | 每个框架至少一个多文件生成器                              |
| B8   | Python bridge（`lan-py-bridge`）：`lint.mypy` + 回落                                                                      | mypy 走 API；移除 bridge 后仍可用                         |
| B9   | Dart bridge（`lan-dart-bridge`）：`lint.analyze` + `build.dart`（**去掉 Node 中转**）                                     | Dart 项目进程层数从 3 降到 2                              |
| B10  | Go bridge：`lint.govet` 进程内 + `build.go` 内 exec                                                                       | 进程层数从 3 降到 2                                       |
| B11  | Rust bridge 骨架：`lint.rust` 委托 clippy                                                                                 | 七运行时同构                                              |
| B12  | 诊断字段补全（行号/规则/修复）+ `create` 补 4 个 Rust fallback                                                            | 问题 3、4 验收                                            |
| B13  | **`lan config` 子命令族**：`init` / `doctor` / `set`（受限）/ `sync`（受限）（§6.5.7）                                    | `init` 产出 `defineConfig` 模板；`.ts` 含函数时不写入     |
| B14  | 模板选项统一到 `RuntimeToolchain.presentation`                                                                            | 新增模板只改 Rust 一处                                    |

**可独立发布的阶段**（不依赖其他阶段的产物）：

| 阶段  | 理由                                                      |
| ----- | --------------------------------------------------------- |
| B0    | 纯缺陷修复                                                |
| B3+B4 | 只改 JS 簇 bridge，Node/Bun/Deno 项目的 lint 体验立刻提升 |
| B6    | 纯 Rust + 模板，与 bridge 完全无关                        |

**建议的落地顺序**：`B0 → B1 → B2+B2.5 → B3+B4 → B6 → B8 → B9 → B5 → B10 → B11 → B7 → B12 → B13 → B14`。
理由：

1. 先修"立即可感知"的缺陷（B0）；
2. 再把架构收口（B1 多 bridge 骨架、B2/B2.5 知识与配置单点）——**这两步是后续一切的前提**；
3. 然后拿体验提升（B3/B4 日志渲染、B6 生成器）；
4. 最后是长尾运行时与配置写入（B8-B14）。

**为什么 `add` 排在 B6 而非最后**：它是唯一不依赖任何 bridge 的重构，
可以并行推进，且**没有回落负担**（不需要考虑 bridge 缺失）。

**为什么 B2.5（配置统一）必须紧跟 B2**：`toolchain.describe` 需要读 `runtime` 字段，
而该字段目前只在 `snapshot.raw` 里。不先做 B2.5，B2 只能继续"从 raw 摸"，
等于把缺陷固化进新接口。

## 12. 验收标准（汇总）

- [ ] `lan env use <runtime> <ver>` 后，同一 shell 内 `lan build` / `lan lint` / `lan create` 使用 managed 版本。
- [ ] 删除任一运行时 bridge 的二进制/脚本后，对应命令通过回落路径正常工作。
- [ ] **运行时判定顺序固定**：`deno.json` > `bun.lockb` > `package.json`；判定结果有提示。
- [ ] 特性文件、默认命令、lint 工具集在仓库内**只有一处**定义，且所有 bridge 均从该处读取。
- [ ] `BridgeEventMethod` 枚举未新增运行时相关变体（保持中立）。
- [ ] **每个 `Diagnostic` 含 `tool` / `id` / `severity` / `message` / `file` / `range` / `fix`（可选字段可缺）。**
- [ ] **`lan lint` TTY 输出按文件分组、列对齐、severity 高亮；`NO_COLOR=1` 时无 ANSI 转义。**
- [ ] **`lan dev` 期间服务端新日志实时输出，带 `[app]` 前缀；`event.diag.seq` 严格递增。**
- [ ] **同一份 ruff 输出经 Python bridge 与 Rust 回落路径解析，`Diagnostic` 深度相等。**
- [ ] **`lan add resource users` 在 NestJS 项目生成 ≥7 个文件并更新 `app.module.ts`。**
- [ ] **`lan add --list` 按框架分组；无框架项目只列 `component` / `config`。**
- [ ] **配置文件候选列表在仓库内只有一处（`node-bridge/src/core/runtime.ts`），Rust 不再硬编码。**
- [ ] **`lan.config.ts` 导出函数时正常求值；函数能拿到 `{ env, cwd, command }`。**
- [ ] **函数型 hook（`hooks.onBuild`）正常工作。**
- [ ] **`.ts` 配置加载失败（语法错误 / import 失败）时 `validation_errors` 有条目且 CLI 显示 warning。**
- [ ] **`LanConfigSnapshot.runtime` / `.runtimes` 是强类型字段。**
- [ ] **`lan config set` 检测到函数/import 时只打印建议，不写入文件。**
- [ ] **`lan config init` 生成的模板使用 `defineConfig` 且有类型提示。**
- [ ] `lan create --template {go-service,rust-lib,python-cli,dart-cli}` 在无 JS 簇 bridge 环境可用。
- [ ] `cargo test --workspace` 与 `pnpm -C ts test` 全绿。
