# 多运行时 bridge 与 dev/build/create/add/lint 重构

> 状态：**B0–B14 已全部落地**。> 目标读者：能直接据此开工的 builder。
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
> - **B7** `add` 扩展至 6 个框架（nestjs / nextjs / gin / fastapi / axum / flutter）；
>   每个框架至少一个多文件生成器，`--list` 按框架分组
> - **B8** Python bridge（`lan-py-bridge`）：`lint.mypy` 走 `mypy.api`；
>   bridge 缺失时回落到 JS 簇 spawn，两条路径产出同一份 `Diagnostic`
> - **B9** Dart bridge（`lan-dart-bridge`）：`lint.dart-analyze` + `build.dart`；
>   `lint` / `build` 都按运行时路由，去掉 Node 中转（详见 §3.6）
> - **B10** Go bridge（`lan-go-bridge`）：`lint.go-vet` / `lint.golangci-lint` + `build.go`；
>   同批把 `build` 的运行时分流补齐（含 Python），详见 §3.4
> - **B11** Rust bridge（`lan-rs-bridge`）：由 `lan` 二进制自身的 bridge 模式承担
>   `lint.clippy` / `build.rust`，零新增产物（详见 §3.5）
> - **B12** 诊断字段补全（行号/规则/修复）+ `create` 补 4 个 Rust fallback
>   （`go-service` / `rust-lib` / `python-cli` / `dart-cli`）；`--template` 候选改为由
>   `lania_workflows::declared_template_names` 单点派生，不再手写第二份清单。
>   §7.3 映射表 12 行已全部收口：新增 `deno-lint`（`--json`/文本双形态）、`deno-fmt`
>   （文件级）、`oxfmt`（文件级，此前只数文件不产诊断）、`bun build`（编译器 stderr →
>   `Diagnostic` + `event.diag`，`channel: "build"`）。
> - **B13** `lan config` 子命令族：`init` / `doctor` / `sync` + `set` 双作用域派发，
>   纯数据配置才允许写盘（§6.5.8 / §6.5.10）
> - **B14** 模板选项统一到 `RuntimeToolchain.presentation`（字面量在
>   `@lania-cli-v2/templates/presentation.ts`，`toolchain.ts` 负责组合并对外暴露；
>   7 个运行时 + 模板额外工具都从这里派生）
>
> 七个运行时现在都走同构路径：`lint` / `build` 由 Rust 侧按运行时选 bridge，
> 失败回落到 JS 簇；`dev` 与 `build --watch` 仍由 compiler-worker 承担（§4.2）。

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

| 知识                     | 位置 A                                | 位置 B                                            | 位置 C                | 位置 D          | 一致？         |
| ------------------------ | ------------------------------------- | ------------------------------------------------- | --------------------- | --------------- | -------------- |
| 特征文件 → 运行时        | `compiler-shared.ts`（5 项）          | `runex-core/language.rs`（版本文件）   | —                     | —               | ❌             |
| 运行时 → 包管理器        | `lania-pm`（`managers_for_language`） | `templates/language-options.ts`                   | —                     | —               | ❌             |
| 运行时 → lint 工具集     | `language-runners.ts`（8 个 adaptor） | `runex-core/language.rs`（companions） | —                     | —               | ❌             |
| 运行时 → 可执行文件位置  | `lania-exec::resolve_executable`      | `language-runners.ts::resolveTool`                | `spawn()`（交给系统） | `lan env which` | ❌             |
| 运行时 → 默认 build 命令 | `runtime-adapters/commands.ts`        | —                                                 | —                     | —               | ✅（唯一单点） |

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

**B8 落地说明**（`rust/crates/lania-py-bridge/`）：

| 项                               | 落地方式                                                                                                                      |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| 分发                             | 单个 Rust crate + `assets/lan_py_bridge.py`（`include_str!`），落盘到 `LANIA_BRIDGE_HOME`（默认用户缓存目录），内容变化才重写 |
| 启动                             | `python -u <script>`；`-u` 不可省，否则事件块缓冲（§10 行缓冲风险）                                                           |
| 解释器定位                       | `.venv`/`venv` 的 `Scripts`/`bin` 优先，其次 PATH；`LANIA_PYTHON` 可覆盖（§6.4 顺序不变）                                     |
| `lint.mypy`                      | 每次请求起一个干净解释器跑 `mypy.api.run()`，**解析留在 bridge 主进程** ⇒ API 与 spawn 两条路径同一份 `Diagnostic`            |
| `lint.ruff` / `black` / `flake8` | 有 API 的走 API；`ruff` 的 Python 包只是二进制壳，退化为 `spawn`                                                              |
| 工具缺失                         | 返回 `implementation: "fallback"` + `message`（不算错误），与 JS 簇的 `fallbackMissingAdaptor` 同语义                         |
| 回落                             | `main.rs` 登记失败 / bridge 调用失败 ⇒ `lint` 继续走 JS 簇路径；e2e 用 `LANIA_PYTHON=/不存在` 锁住                            |
| 诊断归一化                       | `normalize_diagnostics` / `files_from_diagnostics` 与 TS `diagnostics.ts` 逐字段对齐（含去重键与排序）                        |

**已知边界**：`build.python` 已实现（`build.ProjectBuilder` 优先，缺包回落 `python -m build`），
B10 已把 `build` 命令按运行时路由到 Python / Go / Dart bridge（`build.<runtime>`），
因此 Python 项目现在也走 bridge；`dev` 仍保留在 JS 簇（长驻订阅与增量编译见 §4.2）。

**B10 落地说明（`build` 分流的公共部分）**：

| 项         | 落地方式                                                                                       |
| ---------- | ---------------------------------------------------------------------------------------------- |
| 运行时判定 | `lania_host::detect_runtime`（Rust 唯一入口，§5.1 优先级：CLI > 配置 > 特征文件 > node）       |
| 命令来源   | 配置覆盖 `runtimes.<id>.build` 优先，否则 `toolchain.describe` 的 `buildCommand`（单点，§5.2） |
| 占位符     | Rust 侧替换 `{output}`（`--output-dir`，缺省 `bin/app`），bridge 只负责 exec                   |
| watch      | `build --watch` 不走语言 bridge（保留 compiler-worker 的增量语义），继续 node bridge           |
| 失败语义   | bridge 调用失败 ⇒ warn + 回落 JS 簇；bridge 返回 `success:false` ⇒ 退出码 2                    |

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

**Go bridge 的净收益（本节原设想）**：`go vet` 与自定义 Analyzer 可进程内；
`go build` / `golangci-lint` 仍是子进程（bridge 内 `exec`）。
落地时的实际选择见下方「有意偏离」——净收益改为「去掉 Node 中转 + 统一 `Diagnostic`」。

**B10 落地说明**（`rust/crates/lania-go-bridge/`）：

| 项                   | 落地方式                                                                                                                   |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 分发                 | Rust crate + `assets/lan_go_bridge.go`（`include_str!`，只用标准库）；落盘到 `LANIA_BRIDGE_HOME` 后编译                    |
| 编译                 | 优先 `LANIA_GO_BRIDGE` / 与 CLI 同目录的 `lan-go-bridge[.exe]`（CI 交叉编译产物）；都没有时用本机 `go build` 编一次并缓存  |
| 缓存键               | 源码内容 + crate 版本（FNV-1a）；升级 CLI 自动失效；先编译到 `*.staging-<pid>-<nanos>` 再 rename（并发安全）               |
| 懒加载               | `BridgeRegistry::register_deferred`：登记不编译，首次真正需要 bridge 时才解析（`lan env` 等命令零开销）                    |
| `lint.go-vet`        | bridge 内 `exec go vet ./...`，`file.go:line:col: msg` → 全字段 `Diagnostic`（别名 `lint.govet` 同义）                     |
| `lint.golangci-lint` | bridge 内 `exec golangci-lint run --out-format json`；旧版数组 / 新版 `{Issues:[…]}` 双形态都认                            |
| `build.go`           | bridge 内 `exec` 由 Rust 解析好的 argv（`{output}` 已替换），stdout/stderr 逐行走 `event.diag`（`channel: build`）         |
| 诊断归一化           | 去重键 `(tool,file,line,col,id)` + 排序与 Rust / TS 逐字段对齐                                                             |
| 回落                 | 没有 `go`、编译失败、调用失败 ⇒ `lint` / `build` 继续走 JS 簇路径；e2e 用 `LANIA_GO=/不存在` + 空 `LANIA_BRIDGE_HOME` 锁住 |

**有意偏离 §3.4 的一处**：`go vet` 没有做成「进程内 `go/analysis`」。
`go/analysis` 的 analyzer 全在 `golang.org/x/tools`，引入它会让 bridge 的**构建**依赖
模块下载（§9.1 的离线交叉编译约束会失效）。因此 B10 的收益定为
「去掉 Node 中转：`Rust → go bridge → go vet/go build`，2 层 + 统一 `Diagnostic`」。
协议层已经预留位置：将来若把 analyzer 内联，只需在 bridge 里换实现，Rust 侧不改。

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

**B11 落地说明**（`rust/crates/lania-rs-bridge/`）：

| 项            | 落地方式                                                                                                                               |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| 产物形态      | **不新增任何产物**：`lan` 二进制以隐藏参数 `__lania_rs_bridge` 启动就是 Rust bridge                                                    |
| 为什么这样做  | §3.5 的结论是零 API 收益，独立二进制只会多一份平台包；复用同一二进制还能满足 §8.2 的 `--version` 自报版本                              |
| 启动          | `launch_spec` = `current_exe + [__lania_rs_bridge]`；`LANIA_RS_BRIDGE` 可覆盖（预编译 / 测试制造不可用）                               |
| 入口短路      | `main.rs` 在任何宿主装配之前判断该参数：只跑协议循环，不加载插件、不读配置、不打印额外输出（§3.1 约定 1）                              |
| `lint.clippy` | bridge 内 exec `cargo clippy --message-format=json`，JSONL 解析 `reason: "compiler-message"` → 全字段 `Diagnostic`（别名 `lint.rust`） |
| `build.rust`  | bridge 内 exec Rust 侧解析好的 argv（`toolchain.describe` 的 `cargo build --release`），stdout/stderr 逐行 `event.diag`                |
| 失败可见      | `cargo` 不可用 / 退出码非 0 且解析不出诊断 ⇒ 产出一条 `severity: error` 的 `Diagnostic`（与 Go/Dart 同语义，不再静默 0 错误）          |
| 回落          | 调用失败 ⇒ `lint` / `build` 继续走 JS 簇路径；e2e 用 `LANIA_RS_BRIDGE=/不存在` 锁住                                                    |

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

**B9 落地说明**（`rust/crates/lania-dart-bridge/`）：

| 项                  | 落地方式                                                                                                                                                                           |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 分发                | Rust crate + `assets/lan_dart_bridge.dart`（`include_str!`，只用 `dart:io`），落盘后 `dart run <script>`                                                                           |
| 为什么不是 AOT      | `dart compile exe` 无法交叉编译（§9.1 约束 2），为它加 3 个平台包不划算；脚本形态与 `lan-py-bridge` 同构、零平台包                                                                 |
| SDK 定位            | `.fvm/flutter_sdk` / `.fvm/dart_sdk` 优先，其次 PATH；`dart.bat` 旁边若有 `cache/dart-sdk/bin/dart.exe` 直接用后者                                                                 |
| `.bat` 包装         | 只找到 `dart.bat` / `dart.cmd` 时用 `cmd /d /s /c <脚本> run <bridge>` 拉起（Windows 不能直接 CreateProcess 批处理）；**路径不预加引号**（Rust 会把内层引号转义成 `\"`，cmd 不认） |
| 子进程              | 一律用 `Platform.resolvedExecutable`（当前 VM 的绝对路径）启动 `dart analyze` / `dart compile`                                                                                     |
| `lint.dart-analyze` | `dart analyze --format machine` → `SEVERITY\|TYPE\|CODE\|file\|line\|col\|len\|message` 全字段解析                                                                                 |
| `build.dart`        | exec 由 Rust 解析好的 argv（剥掉 `dart` 自身、补 `-o <outputDir>`），stdout/stderr 逐行走 `event.diag`                                                                             |
| 输出串行化          | Dart 的 `IOSink` 在 `flush()` 未完成时会拒绝写入（`StreamSink is bound to a stream`），因此写+flush 串成 FIFO 队列                                                                 |
| 回落                | 没有 dart SDK / 调用失败 ⇒ `lint` / `build` 继续走 JS 簇路径                                                                                                                       |

**验收**：Dart 项目的 `lan lint` / `lan build` 进程链都是 `Rust → dart bridge → dart tool`（2 层），
`runtime` 字段为 `dart`，`bridge` 为 `lan-dart-bridge`（e2e 见
`rust/crates/lania-cli/tests/phase3_e2e/build_and_lint.rs`）。

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
   `runex-core` 定义一次。
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
它们本质是「Node 替其他语言 spawn」。**B9 / B10 之后它们只作为回落路径保留**
（对应语言 bridge 可用时不再经过这里），Rust bridge 落地（B11）后 `rust.ts` 同样降级为回落。

**B9 / B10 落地状态**：`build` 已按运行时分流。

```text
Go 项目：     Rust → go bridge（内 exec go build）        2 层（无 worker）
Dart 项目：   Rust → dart bridge（内 exec dart compile） 2 层（无 worker）
Python 项目： Rust → python bridge（内 spawn python -m build）2 层
JS 项目：     Rust → node bridge → compiler-worker       （不变）
```

- 分流点是 `lania-plugins-command-build/src/handlers.rs::run_runtime_build`：
  判定运行时 → `ctx.has_bridge(kind)` → 用 `toolchain.describe`（或配置覆盖）解析命令 →
  `build.<runtime>` → `CommandExecution::RuntimeBridge`；
- **`build --watch` 不走语言 bridge**：增量/长驻语义仍由 compiler-worker 承担（§4.2 的保留理由）；
- **`dev` 暂不迁移**：需要 `open_call` 长驻订阅 + 进程组 kill（§7.5.2），留待后续批次；
  Dart / Go 的 dev 现仍由 JS 簇 spawn（回落路径，行为与改动前一致）。

### 4.3 `lint`

现状：`runAdaptor` 先试 `runLanguageAdaptor`（spawn），失败再走 JS adaptor（API）。

重构后按项目语言分流：

```text
JS 项目     → node bridge（eslint/stylelint/textlint 走 API；oxlint/oxfmt 走 spawn）
Python 项目 → python bridge（mypy 走 API；ruff/black/flake8 走 spawn）
Go 项目     → go bridge（go vet 走 API；golangci-lint 走 spawn）
Rust 项目   → rust bridge（clippy 走 spawn）
```

**B8 落地状态**：Python 项目已按上表分流 —— `lint` 检测到 `pyproject.toml` /
`setup.py` / `requirements*.txt` / 裸 `.py` 文件后，改为逐个调用
`lan-py-bridge` 的 `lint.<tool>`，再把结果聚合成与 `lint.run` 完全相同的形状。
聚合层新增了 `CommandExecution::RuntimeBridge` 变体，输出层只序列化 `exchange`，
不感知是哪种语言。

**B9 / B10 落地状态**：Go 与 Dart 也已分流（`lan-go-bridge` / `lan-dart-bridge`），
Rust 仍走 JS 簇的 spawn 适配器（B11 未开工）。实现要点：

| 项           | 落地方式                                                                                               |
| ------------ | ------------------------------------------------------------------------------------------------------ |
| adaptor 映射 | `RUNTIME_LINT_TOOLS`（`lania-plugins-command-lint`）是「adaptor → 语言 bridge」的唯一映射点            |
| 运行时判定   | `lania_host::detect_runtime`（§5.1 唯一入口，不再各命令自己数特征文件）                                |
| 工具选择     | `--linters` > `lan.config.lintTools`（快照强类型字段）> 内置默认（`go-vet` / `dart-analyze` / `mypy`） |
| 默认工具     | 选「随工具链自带」的那个，避免新项目 lint 一上来就因为缺第三方 linter 变成噪声                         |
| 聚合         | `runtime_lint_payload(mode, bridge, results)`：结果形状与 `lint.run` 完全一致，`bridge` 字段标明来源   |
| 回落         | 未登记 / 解析失败 / 调用失败 ⇒ `Ok(None)`，继续走 JS 簇路径                                            |

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

**B12 落地说明**（`rust/crates/lania-workflows/src/create/templates.rs`）：

- 文件集合与条件分支按各语言的 `templates/<name>/config.ts` 逐条对齐，
  `DeclaredFileRule::when` 支持 `key:value`（列表包含）/ `key=value` / `key!=value`。
- 派生字段（`packageName` / `modulePath` / `goVersion` / `lintToolsJson` /
  `devRequirementList` / `dartDevDependencies`）在 `with_defaults()` 里补齐，
  与 TS 的 `options.ts` / `template-runtime.ts` 同值。
- `python-cli` 的 dev 依赖由 `computed_dependencies` 按选项渲染 —— 否则兜底路径的
  `requirements-dev.txt` 会是空文件。
- 问题项带 `default`（JSON 字面量），保证非交互场景下 `lintTools` / `testTool`
  不会因为拿不到答案而退化成「没有任何工具配置」。
- 两处**有意偏离** TS 模板，目的是让产物可运行：`rust-lib` 的 `lan.config.js` 只留
  clippy（`rustfmt` 不是 `cargo` 子命令），`go-service` 去掉 `gofmt`（独立二进制）。

**`--template` 候选单点化**：候选由 `BRIDGE_TEMPLATES`（只有 bridge 能渲染的 4 个）
加上 `lania_workflows::declared_template_names()` 合并而成。此前候选是手写列表、
比兜底集合大 4 项，等于给用户 4 个必然失败的选项；`every_declared_fallback_template_is_selectable`
测试锁住这个不变量。

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
>
> **B7 落地说明**（同一批文件，仍是仓库内唯一的生成器定义）：
>
> | 框架      | 多文件生成器 | 产物                                            | 注册锚点            |
> | --------- | ------------ | ----------------------------------------------- | ------------------- |
> | `nextjs`  | `segment`    | `app/<seg>/{page,layout,loading}.tsx`           | —                   |
> | `gin`     | `resource`   | `internal/{model,repository,service,handler}/…` | `// <lania:routes>` |
> | `fastapi` | `resource`   | `app/{schemas,models,crud,routers}/…`           | —                   |
> | `axum`    | `resource`   | `src/{model,repository,service,handler}/…`      | `// <lania:mods>`   |
> | `flutter` | `feature`    | `lib/{models,services,controllers,pages}/…`     | —                   |
>
> - **框架探测**（`detect.rs`）两级：先看框架标志文件（`nest-cli.json` / `next.config.*`），
>   再看依赖声明里的框架名（`go.mod`→gin、`pyproject.toml`→fastapi、`Cargo.toml`→axum、
>   `pubspec.yaml`→flutter）。**有 `go.mod` 但不是 gin 仍然回退到通用生成器**，不猜框架。
> - **默认目标目录**按框架推导：nestjs/nextjs → `src/<name>`；gin/axum/fastapi → `.`
>   （产物自带 `internal/…`、`app/…` 完整分层，套一层会变成 `internal/internal/…`）；
>   flutter → `lib`。
> - **`--list` 按框架分组**：新增 `groups: [{framework, generators}]`，`generators` 保持平铺兼容旧消费者。
> - **产物可运行性已用真实工具链验证**：`go build ./...`、`cargo check`、`dart analyze`、
>   `python -c "from app.routers.users import router"` + `TestClient` 全部通过。
>   为此 `axum` 的 repository/service 改为 `Clone` + `Arc<RwLock<_>>`（否则不能作 axum `State`），
>   `router` 生成器只保留能编译的 `GET` 路由（`POST` 由 `handler` 自身暴露）。
> - 渲染只做 `{key}` 替换、没有转义，模板里出现 `{{`/`}}` 会原样进产物；已加
>   `templates_do_not_contain_escape_style_doubled_braces` 锁住这个坑。
> - `create` 的 nestjs 模板 `app.module.ts` 现在带 `// <lania:imports>`，
>   所以「新建的 Nest 项目立刻 `lan add resource`」也能自动注册。

## 5. 运行时知识单点（解决问题 2）

### 5.1 数据模型

在 `runex-core` 新增（`language.rs` 扩展）：

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
pyproject.toml / setup.py / requirements*.txt → python
go.mod                       → go
Cargo.toml                   → rust
pubspec.yaml                 → dart
package.json                 → node       （兜底放最后）
```

> **注意顺序与早期草案的差异**：非 JS 强信号（`go.mod` / `Cargo.toml` / `pyproject.toml` /
> `pubspec.yaml`）排在 `package.json` **之前**。Go / Rust / Python / Dart 项目里带一个
> `package.json`（前端资源、脚本工具链）很常见，若让 `package.json` 先命中，
> 这些项目会被判成 web 项目、`build` 会走错工具链。TS 侧的 `toolchain.ts`
> （`DETECTION_PRIORITY`）用的是同一份顺序。

**Rust 侧唯一入口（B9/B10 落地）**：`lania_host/src/runtime/detect.rs`

```rust
pub fn detect_runtime(
    cwd: &Path,
    config: Option<&LanConfigSnapshot>,  // 读强类型 runtime 字段（§6.5.5）
    cli_runtime: Option<&str>,           // `--runtime`
) -> RuntimeKind;
pub const FEATURE_PRIORITY: &[(&str, RuntimeKind)];  // 上表的数组形式
```

`lint` / `build` 都从这里拿运行时（不再各自数特征文件），不再有第二份判定表。

判定结果**必须**通过 `diagnostic` 输出告知用户（"检测到 deno.json 与 package.json，
已选择 deno；可用 `--runtime node` 覆盖"），避免用户困惑。
**该提示尚未实现**（见 §12 未勾选项）：当前只保证判定顺序固定。

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
| Rust `create` 工作流             | 直接 `use runex_core::toolchain`                   |
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

| 命令                                     | 首选                                  | 回落               | 用户可见               |
| ---------------------------------------- | ------------------------------------- | ------------------ | ---------------------- |
| `lint` / `build` / `dev` (node/bun/deno) | JS 簇 bridge（必有，Node 就是运行时） | —                  | —                      |
| `lint` / `build` (Python)                | `lan-py-bridge`                       | JS 簇 spawn 适配器 | 无差异（事件模型相同） |
| `lint` / `build` (Go)                    | `lan-go-bridge`                       | 同上               | 无差异                 |
| `lint` / `build` (Rust)                  | `lan-rs-bridge`（B11）                | 同上               | 无差异                 |
| `lint` / `build` (Dart)                  | `lan-dart-bridge`                     | 同上               | 无差异                 |
| `build --watch` / `dev`（所有运行时）    | JS 簇（compiler-worker）              | —                  | —                      |
| `add`（任意框架）                        | Rust 侧生成器（不经 bridge）          | —                  | —                      |

> **回落目标写清楚**：`lint` / `build` 的回落是**同进程里的 JS 簇 bridge**（`lint.run` /
> `compiler.build`，再由 `runtime-adapters/*` spawn 工具），不是 Rust 侧 `lania-exec`
> —— 因为参数解析、配置读取、诊断归一化都已在 JS 簇实现，复用它能保证两条路径产出同一份
> `Diagnostic`（§5.3 的「回落实现要求」）。

**为什么必须回落**：用户可能只装了 Node（`npm i -g`），没装其他 bridge 二进制。
回落路径保证「装了就更好，不装也能用」，这是分阶段发布的必要条件。

**回落判定必须廉价**（B9/B10 落地细节）：语言 bridge 用
`BridgeRegistry::register_deferred` 登记 —— 登记只记 `cwd + 解析函数`，
**首次真正需要时才解析**（`lan-go-bridge` 此时可能编译一次并缓存）。
`is_registered` 对延迟登记的 bridge 返回 `true`，因此：
「登记成功但解析失败」也走回落（`ctx.bridge(kind)` 返回 `Err`），不会卡住命令。

**`add` 不经过任何 bridge**：生成器表与模板渲染都在 Rust 侧与 TS 模板包内完成，
与运行时 bridge 完全解耦。这是 `add` 相比其他命令的架构优势，必须保持。

## 6. 工具链血缘（解决问题 1）

### 6.1 缺陷

`lan env use go 1.22` 只做两件事：改 `~/.runex/state.json` 的 `current`、重写 shim 文件。
但 `build` / `lint` / `create` 的子进程**继承父进程 PATH**，而 `~/.runex/shims` 从不在其中。
结果：managed 版本永远不生效（除非用户手改 shell profile）。

### 6.2 修法：入口统一前置，而非逐处注入

```rust
// runex-core/src/home.rs（新增）
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

**B13 落地说明**（`rust/crates/lania-plugins-command-locale/src/project_config.rs`）：

| 项              | 落地方式                                                                                                                                                                                |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `config init`   | 写 `cwd/lan.config.ts`；已有任何 `lan.config.*` 时默认**不覆盖**（`written: false` + 完整建议内容），`--force` 才写                                                                     |
| `defineConfig`  | **只有项目能解析到 `lania` 包时**（`node_modules/lania` 或 package.json 依赖里有）才 import 它并包一层；否则写普通 `export default {…}` —— 新项目直接 import 会让配置一生成就加载失败   |
| `config doctor` | 只读：`configPath` / `exists` / `runtime{declared,detected,effective,source}` / `runtimes` / `lintTools` / `pureData` / `writable` / `validationErrors` / `issues[]`；问题同时走 logger |
| `config set`    | 双作用域派发：`locale` / `log.timestamps` / `output.mode` 仍是全局偏好；其余按项目配置路径处理（顶层字段用 `lania_config::known_config_fields` 校验）                                   |
| `config sync`   | 默认**只给建议**（`snippet` + `suggestions[]`），`--write` 才落盘；纯数据配置才允许写                                                                                                   |
| 纯数据判定      | `is_pure_data()`：源文件不含 `import ` / `require(` / `=>` / `function`；命中即拒绝改写，只打印建议                                                                                     |
| 写回策略        | 不保留原格式：按原模块风格（ESM / CJS / JSON）把求值后的 `raw` 重新序列化 —— 对纯数据配置等价，函数配置本来就被挡住                                                                     |
| 单点            | 顶层字段白名单只有一份：`lania_config::known_config_fields()`（validate 与写入共用）                                                                                                    |

**模板产物也是 `.ts`**（§4.4 的收口）：10 个 `create` 模板（ejs + Rust 兜底）统一产出
`lan.config.ts`（`export default {…}`）。**不写 `defineConfig` import**：这些模板生成的项目
（Go/Rust/Python/Dart 尤其）没有 `node_modules/lania`，import 会让配置加载失败；类型提示留给
`lan config init` 的可解析分支。

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

- [x] Rust 侧不再硬编码 `lan.config.*` 候选列表；`searchFrom` 之外的列表来自 bridge。
      （`lania-host/src/runtime/config/loading.rs` 只从 bridge 的 `config.get` 取
      `searchFrom` / `candidates`；候选权威仍是 `node-bridge/src/core/runtime.ts`。）
- [x] `lan.config.mts` / `.cts` 在 Rust 与 bridge 两条路径下都能被找到（行为一致）。
      （Rust 不传 `candidates` ⇒ 用 bridge 的 7 项默认表；`config doctor` 输出
      `supportedExtensions` 供核对。）
- [x] `lan.config.ts` 导出函数时，返回值被正确求值并成为配置。
- [x] **函数式配置能拿到 `{ env, cwd, command }`**；`LANIA_ENV=prod` 时 `({env}) => ...` 返回 prod 分支。
      （`node-bridge/src/core/runtime.test.ts` 的
      `loadLanConfig evaluates function configs with { env, cwd, command }`。）
- [x] 函数型 hook（`hooks.onBuild: (ctx) => ...`）在 `.ts` 配置下正常工作。
      （`sanitizeConfigHooks` → inline 引用，B2.5 落地并有 TS 测试。）
- [x] `LanConfigSnapshot.runtime` / `.runtimes` 是强类型字段，可直接读取。
      （`lania-config/src/models.rs`；`lan config doctor` 直接序列化 `runtimes`。）
- [x] `.ts` 配置加载失败（语法错误 / import 失败）时，`validation_errors` 有明确条目，CLI 显示 warning。
      （`broken_lan_config_ts_surfaces_a_warning_e2e`：语法错误的 `lan.config.ts` 下
      `config doctor` 仍退出 0，stderr 有 `lan.config …` warning，`validationErrors` 非空。）
- [x] `lan config set` 在检测到函数/import 时**不写入**，只打印建议片段。
      （`lan_config_set_refuses_function_configs_e2e`：文件内容逐字节不变。
      硬约束，`--force` 也不绕过。）
- [x] `lan config init` 生成的模板使用 `defineConfig` 且有类型提示。
      （**两段式**：项目能解析到 `lania` 包时写 `import { defineConfig } from 'lania'` + `export default defineConfig({…})`；否则写普通 `export default {…}` 并附注释。
      这样「新项目 config 一生成就加载失败」不会发生 —— 见 §6.5.8 的落地表。）

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

> **状态：12 行全部已落地**（B12）。下表保留原始工作量评估，便于回溯为什么某些工具
> 只能给到文件级诊断。

| 工具             | 现状产出                                                         | 需要补的字段                                                              | 难度                | 落地                                           |
| ---------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------- | ------------------- | ---------------------------------------------- |
| ruff（JSON）     | `filename`                                                       | `code` / `location` / `message` / `fix`                                   | 低（JSON 已含全部） | ✅ 全字段                                      |
| golangci（JSON） | `Pos.Filename`                                                   | `FromLinter` / `Text` / `Pos.Line` / `Replacement`                        | 低                  | ✅ 全字段                                      |
| clippy（JSON）   | `spans[0].file_name`                                             | `code.code` / `message` / `spans[0].line_start` / `suggested_replacement` | 低                  | ✅ 全字段                                      |
| mypy（文本）     | `file:line: severity:`                                           | 全部（需正则解析）                                                        | 中                  | ✅ 含规则号（末尾 `[code]`）                   |
| flake8（文本）   | `file:line:col: code`                                            | `message`                                                                 | 中                  | ✅ 全字段                                      |
| go vet（文本）   | `file.go:line:col:`                                              | `message`                                                                 | 中                  | ✅ 全字段                                      |
| dart analyze     | `severity\|type\|code\|file\|line\|col\|len\|msg`                | `code` / `message`（已含）                                                | 低                  | ✅ 全字段                                      |
| ESLint           | `errorCount` / `warningCount`                                    | 全部（ESLint API 直接给 `messages[]`）                                    | 低                  | ✅ 读 `messages[]`                             |
| oxlint / oxfmt   | 文本                                                             | 全部                                                                      | 中                  | ✅ oxlint 全字段；**oxfmt 仅文件级**（无行列） |
| **deno lint**    | `--json`（v2+）：`{diagnostics:[{filename,range,code,message}]}` | v2 全部可得；**v1 无 `--json`，需正则**                                   | 低（v2）/ 中（v1）  | ✅ 双形态：JSON 优先，失败回落文本             |
| **deno fmt**     | `--check` 仅退出码 + 文件名列表                                  | `file` / `fix.available`（无行列）                                        | 低                  | ✅ 文件级诊断，`fix.available = true`          |
| **bun build**    | 文本 + 退出码                                                    | 全部（需解析 bun 的构建错误格式）                                         | 中                  | ✅ `error:` + `at file:line:col`，多错误全收   |

**三处只能做到文件级 / 需要注意的地方**：

- `oxfmt --list-different` 与 `deno fmt --check` 只输出文件路径，拿不到行列，
  因此产出 `file` 有值、`range` 缺省的文件级诊断（符合 §7.3 的可选性约定）。
- `bun build` 的错误落在 **stderr**，且一个 `error:` 后跟一行 `at <file>:<line>:<col>`；
  解析按「段」切分而不是按行，避免多错误时后一个覆盖前一个。
- `bun` 目前**没有注册 spawn 运行时适配器**（`SpawnRuntime` 不含 `bun`/`deno`），
  所以 `bun build` 的诊断解析器已就绪并单独测试，但要等 bun 的 build 适配器接上
  才会在真实 `lan build` 路径里生效。

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

- [x] `lan lint` 在 TTY 下按文件分组、列对齐、按 severity 高亮；`NO_COLOR=1` 时无 ANSI 转义。
      （`lint_report.rs` 的分组/对齐/ANSI 单测；`for_terminal(is_terminal && !no_color)`。）
- [x] `lan lint --json` 的 `diagnostics[]` 含 `tool` / `id` / `severity` / `message` / `file` / `range`。
      （四个语言 bridge 的 e2e 都断言 `result.diagnostics[*]` 的这些字段。）
- [x] `lan build` 的 stderr 不再一律显示为 warning；真实 error 显示为 error。
      （Go / Rust bridge 的 `looksLikeError` 按内容推断 level；`event.diag.level` 不再是纯 `warning`。）
- [x] `lan dev` 启动后，服务端新产生的 stdout 行**实时**出现在终端，带 `[app]` 前缀。
      （`event.diag` 实时转发 + 四个通道前缀渲染；e2e
      `lan_dev_streams_app_logs_with_prefix_and_quiet_app_suppresses_e2e`。）
- [x] `lan dev` 输出的 `event.diag` 的 `seq` 严格递增（stdout/stderr 合并无乱序）。
      （`diag.test.ts` 的 `seq is strictly increasing across stdout and stderr`。）
- [x] 四种 bridge（Node/Python/Go/Rust）对同一份 ruff 输出产出**完全相同**的 `Diagnostic`。
      （Python 与 Node 用共享 fixture + golden 双向断言（§12 最后一条）；Go/Rust 的解析器
      走同一套 `Diagnostic` 字段约定与归一化（去重键 + 排序），由各自 crate 单测覆盖。）
- [x] `lan lint --json | jq '.diagnostics | length'` 在 CI 无 TTY 环境下与 TTY 一致。
      （`--json` 与 TTY 共用同一份 `RunResult.result`；渲染器只影响人读输出。）

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
| `go`                    | 单文件 `.go` 源码（内嵌）+ 可选预编译  | **不需要**（B10 决定）    |
| `rust`                  | 编译成二进制                           | 3 个                      |
| `dart`                  | 单文件 `.dart` 脚本（内嵌）            | **不需要**（B9 决定）     |

> **B9 / B10 的实际产物形态与本节早期设想不同**（这是有意的简化）：
>
> - `lan-go-bridge` 内嵌**单文件 Go 源码**（只用标准库），优先使用
>   `LANIA_GO_BRIDGE` / 与 CLI 同目录的预编译二进制；都没有时用本机 `go build` 编一次并缓存。
>   因此**不需要为它新增平台包**，也不依赖 CGO（§9.1 约束 1 天然满足）。
> - `lan-dart-bridge` 内嵌**单文件 Dart 脚本**，`dart run` 直接解释执行（与 `lan-py-bridge` 同构），
>   彻底绕开「Dart AOT 无法交叉编译」（§9.1 约束 2）。
>
> 结果：B9 + B10 之后 npm 平台包数量**仍是 3 个**，只有 Rust bridge（B11）可能需要新增。

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

| 阶段 | 内容                                                                                                                           | 验收                                                            |
| ---- | ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| B0   | `activate()` + 入口调用 + `doctor` 调整（§6）                                                                                  | `lan env use` 后 `lan build` 用 managed 版本                    |
| B1   | `bridge_launch_spec` 参数化 + `BridgeRegistry` 骨架（§2.2）                                                                    | 可启动任意 `program` 说 jsonl                                   |
| B2   | `toolchain.describe` 单点 + JS 簇 bridge 改读（§5）                                                                            | 删除 `RUNTIME_FEATURE_FILES` / `DEFAULT_COMMANDS`               |
| B2.5 | **配置统一**：Rust 候选去重 + `runtime`/`runtimes` 强类型 + 加载失败可见 + **函数求值上下文 `{env,cwd,command}`**（§6.5）      | 两侧行为一致；`({env}) => …` 能分出 prod 分支                   |
| B3   | `event.diag` 协议 + 劫持实现 + `RunResult`/`Diagnostic`（§7.2、§7.3）                                                          | 同一工具在两个 bridge 产出相同 `Diagnostic`                     |
| B4   | `LintReportRenderer` + 高亮 + `--json` 新结构（§7.4）                                                                          | TTY 分组对齐高亮；`NO_COLOR` 下无转义                           |
| B5   | dev 长驻订阅 + `channel: "app"` 前缀渲染（§7.5）                                                                               | 服务端新日志实时出现且带 `[app]`                                |
| B6   | **`add` 生成器重构**：框架探测 + 命名推导 + 多文件产物 + NestJS 首个落地（§4.5）                                               | `lan add resource users` 生成 7 文件并注册进 `app.module`       |
| B7   | `add` 扩展：nextjs / gin / fastapi / axum / flutter 生成器                                                                     | 每个框架至少一个多文件生成器                                    |
| B8   | Python bridge（`lan-py-bridge`）：`lint.mypy` + 回落                                                                           | mypy 走 API；移除 bridge 后仍可用                               |
| B9   | ✅ **Dart bridge（`lan-dart-bridge`）**：`lint.dart-analyze` + `build.dart`（**去掉 Node 中转**）                              | Dart 项目进程层数从 3 降到 2；e2e 断言 `bridge=lan-dart-bridge` |
| B10  | ✅ **Go bridge（`lan-go-bridge`）**：`lint.go-vet` / `lint.golangci-lint` + `build.go`；同批补 `build` 运行时分流（含 Python） | 进程层数从 3 降到 2；移除 bridge 后仍可用                       |
| B11  | ✅ **Rust bridge（`lan-rs-bridge`）**：`lan` 二进制以 `__lania_rs_bridge` 模式承担 `lint.clippy` + `build.rust`                | 七运行时同构；`lan lint`/`build` 的 `bridge=lan-rs-bridge`      |
| B12  | 诊断字段补全（行号/规则/修复）+ `create` 补 4 个 Rust fallback                                                                 | 问题 3、4 验收                                                  |
| B13  | ✅ **`lan config` 子命令族**：`init` / `doctor` / `sync` + `set` 双作用域（受限）（§6.5.8）                                    | `init` 产出 `defineConfig` 模板；`.ts` 含函数时不写入           |
| B14  | ✅ 模板选项统一到 `RuntimeToolchain.presentation`（字面量在 `@lania-cli-v2/templates/presentation.ts`，`toolchain.ts` 组合）   | 7 个运行时 + 模板额外工具都从 presentation 派生，测试锁漂移     |

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

> **实际顺序**：B6 之后直接做了 **B8**（Python bridge），再补 **B7**（其余框架生成器），
> 然后按 §11 的建议顺序做完 **B9**（Dart）/ **B10**（Go + `build` 分流）/ **B11**（Rust）/
> **B12** / **B13**（config 子命令族）/ **B14**（presentation）。
> 四个 bridge 共用一份「说话方式」（jsonl 协议 + `Diagnostic` 归一化）与一份回落策略，
> 因此可以各自独立验证：`LANIA_PYTHON` / `LANIA_GO` / `LANIA_DART` / `LANIA_RS_BRIDGE`
> 指向不存在的路径就是「移除该 bridge」的等价场景（e2e 已锁住 Python / Go / Rust）。

**为什么 `add` 排在 B6 而非最后**：它是唯一不依赖任何 bridge 的重构，
可以并行推进，且**没有回落负担**（不需要考虑 bridge 缺失）。

**为什么 B2.5（配置统一）必须紧跟 B2**：`toolchain.describe` 需要读 `runtime` 字段，
而该字段目前只在 `snapshot.raw` 里。不先做 B2.5，B2 只能继续"从 raw 摸"，
等于把缺陷固化进新接口。

## 12. 验收标准（汇总）

- [x] `lan env use <runtime> <ver>` 后，同一 shell 内 `lan build` / `lan lint` / `lan create` 使用 managed 版本。
      （`RuntimeHome::activate()` 在 `main.rs` 最前面执行，子进程继承 PATH；
      `runex-core/src/home.rs` 有「幂等 / shims 不存在时跳过 / 前置顺序」三条约束的单测。
      **未做**需要真实运行时下载的跨进程 e2e —— 它依赖网络与目标平台二进制。）
- [x] 删除任一运行时 bridge 的二进制/脚本后，对应命令通过回落路径正常工作。
      （Python：`LANIA_PYTHON=/不存在`；Go：`LANIA_GO=/不存在` + 空 `LANIA_BRIDGE_HOME`；
      Rust：`LANIA_RS_BRIDGE=/不存在`。三条 e2e 各自锁住“不认领 bridge + 仍能跑”。）
- [x] **运行时判定顺序固定**：`deno.json` > `bun.lockb` > `package.json`；判定结果有提示。
      （顺序：`lania_host::detect::FEATURE_PRIORITY` + `deno_and_bun_win_over_package_json`；
      提示：`decide_runtime` 在多信号项目（如 `deno.json` + `package.json`）返回一句
      「detected …; using `deno` — pass `--runtime <id>` to override」，由
      `lint` / `build` / `dev` 的 `log_runtime_decision` 以 warning 输出。）
- [x] 特性文件、默认命令、lint 工具集在仓库内**只有一处**定义，且所有 bridge 均从该处读取。
      （默认命令：`toolchain.describe` 由 Rust 读取后传给 bridge；lint 工具：TS
      `toolchain.ts` 管参数表、Rust `RUNTIME_LINT_TOOLS` 管“归哪个进程”（职责不同）；
      特征文件顺序：TS `DETECTION_PRIORITY` 是权威，Rust `FEATURE_PRIORITY` 与之靠
      `rust_detection_order_matches_the_ts_toolchain_table` 单测逐条比对，漂移即红。）
- [x] `BridgeEventMethod` 枚举未新增运行时相关变体（保持中立）。
      （Go / Dart bridge 只发既有事件：`event.ready` / `event.diag` / `event.lint_start` /
      `event.lint_result` / `event.shutdown` / `event.heartbeat`。）
- [x] **每个 `Diagnostic` 含 `tool` / `id` / `severity` / `message` / `file` / `range` / `fix`（可选字段可缺）。**
      §7.3 映射表 12 行全部收口；`oxfmt` / `deno fmt` 受工具输出限制只给文件级诊断。
      Go / Dart bridge 侧解析器与 TS `parsers.ts` 逐字段对齐（`go-vet` / `golangci-lint` / `dart-analyze`）。
- [x] **`lan lint` TTY 输出按文件分组、列对齐、severity 高亮；`NO_COLOR=1` 时无 ANSI 转义。**
      （`lania-presentation/src/lint_report.rs`：`groups_by_file_and_sorts_within_group` /
      `aligns_columns_across_diagnostics` / `no_ansi_when_terminal_colors_disabled` /
      `ansi_is_emitted_when_terminal_colors_enabled`。）
- [x] **`lan dev` 期间服务端新日志实时输出，带 `[app]` 前缀；`event.diag.seq` 严格递增。**
      （`lan_dev_streams_app_logs_with_prefix_and_quiet_app_suppresses_e2e`：应用每 200ms
      打一行 → stderr 出现 `[app] GET /api/user 200`，`--quiet-app` 后不再出现；
      `format_diag_line` 单测固定四个通道前缀；`diag.test.ts` 锁 `seq` 跨流严格递增。）
- [x] **同一份 ruff 输出经 Python bridge 与 Rust 回落路径解析，`Diagnostic` 深度相等。**
      （共享 fixture：`rust/crates/lania-py-bridge/assets/fixtures/ruff-{sample,diagnostics}.json`。
      Python 侧 `ruff_parser_matches_the_shared_golden`（真跑 bridge 脚本的 `parse_ruff_json`），
      TS 侧 `plugins/lint/ruff-fixture.test.ts` 用 `parseRuffDiagnostics` 断言同一份 golden；
      两边任一字段漂移都会各自失败。解析器语义已对齐：ruff 恒为 `error`，
      `fix.kind` 取 `applicability`。）
- [x] **B9：Dart 项目的 `lan lint` / `lan build` 走 `lan-dart-bridge`，进程链 2 层。**
      （`lan_lint_uses_the_dart_bridge_for_dart_projects_e2e` /
      `lan_build_uses_the_dart_bridge_e2e`：`result.bridge == "lan-dart-bridge"`、
      `runtime == "dart"`、产物落在 `--output-dir`。）
- [x] **B10：Go 项目的 `lan lint` / `lan build` 走 `lan-go-bridge`，进程链 2 层。**
      （`lan_lint_uses_the_go_bridge_for_go_projects_e2e` /
      `lan_build_uses_the_go_bridge_e2e`：`go vet` 的 printf 诊断带 `file` / `line`，
      `build` 产物落到 `bin/app`。）
- [x] **`lan add resource users` 在 NestJS 项目生成 ≥7 个文件并更新 `app.module.ts`。**
- [x] **`lan add --list` 按框架分组；无框架项目只列 `component` / `config`。**
      （`groups` 字段；B7 把分组扩到 6 个框架。）
- [x] **配置文件候选列表在仓库内只有一处（`node-bridge/src/core/runtime.ts`），Rust 不再硬编码。**
      （`runtime/config/loading.rs` 只透传 `searchFrom` / bridge 给的 `candidates`。）
- [x] **`lan.config.ts` 导出函数时正常求值；函数能拿到 `{ env, cwd, command }`。**
      （`node-bridge/src/core/runtime.test.ts` 的
      `loadLanConfig evaluates function configs with { env, cwd, command }`，含 `LANIA_ENV=prod` 分支。）
- [x] **函数型 hook（`hooks.onBuild`）正常工作。**
      （`sanitizeConfigHooks` → inline 引用；B2.5 落地，TS 侧有测试。）
- [x] **`.ts` 配置加载失败（语法错误 / import 失败）时 `validation_errors` 有条目且 CLI 显示 warning。**
      （`broken_lan_config_ts_surfaces_a_warning_e2e`。）
- [x] **`LanConfigSnapshot.runtime` / `.runtimes` 是强类型字段。**
      （`RuntimeCommandOverride` + `runtime: Option<String>`，`lan config doctor` 直接输出。）
- [x] **`lan config set` 检测到函数/import 时只打印建议，不写入文件。**
      （`lan_config_set_refuses_function_configs_e2e`，文件逐字节不变。）
- [x] **`lan config init` 生成的模板使用 `defineConfig` 且有类型提示。**
      （能解析到 `lania` 包时用 `defineConfig`，否则普通对象 + 注释；两段式见 §6.5.8。）
- [x] **B11：Rust 项目的 `lan lint` / `lan build` 走 `lan-rs-bridge`（`lan` 二进制自身）。**
      （`lan_lint_uses_the_rust_bridge_for_rust_projects_e2e`：clippy 的
      `clippy::needless_range_loop` 带 `file` / `line`；`lan_build_uses_the_rust_bridge_e2e`：
      command 来自 `toolchain.describe`、产物在 `target/release`。）
- [x] **B13：`lan config init/doctor/sync` 与 `set` 双作用域。**
      （`lan_config_init_creates_lan_config_ts_e2e` / `..._refuses_to_overwrite_without_force_e2e` /
      `lan_config_set_project_key_writes_pure_data_e2e` / `lan_config_doctor_reports_runtime_mismatch_e2e` /
      `lan_config_sync_suggests_then_writes_when_asked_e2e`。）
- [x] **B14：模板选项只有一处定义（presentation），模板从它派生。**
      （`templates/src/presentation.ts` 是唯一字面量来源；`toolchain.ts` 组合成
      `RuntimeToolchain.presentation` 并由 `toolchain.describe` 暴露；
      `template.test.ts` / `toolchain.test.ts` 锁住 7 运行时覆盖与「模板选项 == presentation」。）
- [x] **`create` 产出的项目配置是 `lan.config.ts`**（10 个模板 + 4 个 Rust 兜底一致）。
      （`create_workflow_falls_back_to_declared_language_templates` 与 create e2e 断言 `.ts`。）
- [x] `lan create --template {go-service,rust-lib,python-cli,dart-cli}` 在无 JS 簇 bridge 环境可用
      （`create_workflow_falls_back_to_declared_language_templates` 以不可达 bridge 验证）。
- [x] **B7：`lan add <generator> <name>` 在 nextjs / gin / fastapi / axum / flutter 项目各生成 ≥2 个文件，
      产物可被对应工具链接受**（`go build` / `cargo check` / `dart analyze` / `fastapi` 实际启动）。
- [x] `cargo test --workspace` 与 `pnpm -C ts test` 全绿。
