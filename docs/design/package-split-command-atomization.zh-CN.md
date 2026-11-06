# 按包拆分与命令原子化设计方案

> 状态：**方案定稿 / 代码未实施**（2026-11-06）
>
> 本文只固化设计决策与施工依据，**仓库中当前不含任何 `packages/` 目录、命令子包、core 调度器或 manifest 驱动机制**。
> 现状代码基线：`rust/crates/*` + `ts/packages/*` + `npm/*` 三套目录、单一 `lania-cli` 二进制。
>
> 前置文档：
>
> - `self-managed-runtime-manager.zh-CN.md`（runex 能力层 / 产品 API / `ShimHost` / 冻结导出面）
> - `agent-architecture.zh-CN.md`（agent 三层拆包 / §3.2 A4 待办 / §11.4 待办）
> - `bridge-layer.zh-CN.md`（Bridge Hub 定稿但代码暂停；本文收敛其 §6 的 Hub 部署形态未决项）
> - `multi-runtime-bridge.zh-CN.md` §8（npm 包结构与平台包矩阵）
> - `../architecture/框架化与产品化总览.zh-CN.md` §6 / §14、`../architecture/模块设计与通信总览.zh-CN.md` §2 / §14
>
> 本文的产出边界：**只做设计，不改代码**。施工阶段（§8 阶段 1 起）另行排期。

---

## 0. 决策记录

| #   | 决策           | 结论                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| --- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | 目录形态       | 根目录新建 `packages/*` 统一容器；每个包内含 `ts/`、`rust/`、`npm/` 子目录归置不同语言代码，**不再按语言顶层分类**。现有顶层 `ts/`、`rust/`、`npm/` 逐步迁入后删除，最终只剩一套 pnpm workspace + 一套 Cargo workspace。通用骨架、功能独立模块、可直接成包的模块都成为 `packages/` 下独立包。                                                                                                                                                                                                                                                                                                                                                     |
| 2   | 三个主包       | `runex`（多运行时管理器）、`agent`（agent 核心）、`bridge-hub`（语言中立 Hub）各自独立成主包，统一「Rust lib API + 独立 bin + 可发 npm 包」双形态。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 3   | 命令原子化     | 12 个命令各拆成一个子包（`template` 并入 `create` 作为二级命令），目标是「只装 env 命令的包就能用 `lan env`，不必安装全量体量」。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 4   | 子包双形态     | 每个命令子包**同时提供 API 与 CLI 两种调用方式**：单独安装使用时走 CLI；作为总包依赖时，总包通过 **API 静态链接**调用子包。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 5   | 总包           | 保留 `@lania-cli-v2/cli`，依赖全部命令子包、静态链接成单一 `lania-cli` 二进制，**行为与现在逐字节不变、不经过调度器、不新增进程跳转**；子包行为 = 单条命令的行为。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| 6   | 入口归属       | 极小 `@lania-cli-v2/core` 提供**唯一** `lan` 入口与调度器，按构建期已知命令表探测安装状态，路由到子包自带的 `lan-<cmd>` 二进制。装 1 个或 N 个子包都不冲突，未安装命令给 `npm i @lania-cli-v2/cmd-<name>` 提示。                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 7   | 总包安装拓扑   | `@lania-cli-v2/cli` **移除 `bin.lan`**，改为 `dependencies` 带 `@lania-cli-v2/core`；launcher 检测到全量二进制就直接启动，不进入调度。装 cli 仍自动得到 `lan`。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 8   | 首批产物范围   | 12 个命令**全部**首发平台二进制包（12 × 3 = 36 个产物）；CI 用动态 matrix、单平台单 job 内一次编译 + 多次链接。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 9   | Hub 部署形态   | **进程内嵌**：`lania-bridge-host` 静态链接进每个 `lan-<cmd>`。不采用 core 持有 sidecar Hub；独立 `lan-hub` bin 只作调试与未来长驻场景。此结论收敛 `bridge-layer.zh-CN.md` §6 未决项。                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 10  | 兼容约束       | 12 个 builtin 命令（template 并入 create）行为不变（golden 基线 + `scripts/smoke-pi.mjs` 8/8 + `lan env` 13 条 e2e）；沿用 `os`/`cpu` + `optionalDependencies` + 无 postinstall + 安装期不联网；沿用 `框架化与产品化总览.zh-CN.md` §6 的四个分发边界。                                                                                                                                                                                                                                                                                                                                                                                            |
| 11  | config 归属    | **`config` 命令不独立成子包**（修订本文 §3.2/§3.3/§5.3/§5.4 原列的 `commands/config`）：保留在总包（crate `lania-plugins-command-locale` 不迁出 `rust/crates/`），角色 = 全局偏好 + 项目配置 + 跨包视角管理子包偏好。**每个命令子包自带 `config` 二级命令**管理本包包级偏好，配置引擎下沉为 foundation crate（§10.2 修订），存储 `~/.lania/config/<pkg>.json` 按包隔离。命令子包 12 → **11**，平台二进制 11 × 3 = 33；总包 builtin 命令仍为 12 条。                                                                                                                                                                                               |
| 12  | 插座唯一事实源 | **bridge-runtime 保留「电器」，但插座必须经由 bridgehub，协议逻辑不允许写两遍**：`bridge-runtime/ts` 的帧编解码、stdio 循环、boot、心跳、错误包装一律复用 bridge-hub 的 `protocol/schema.json` + `gen/ts/runtime.ts`（gen.mjs 生成，单一事实源），不自研帧层。据此退役两处重复实现：① `node-bridge` 自带的 `src/protocol/*` + `src/entry/stdio.ts` 帧层（阶段 4a）；② `adaptors/ts/lania-node-bridge` client 的自有帧编码（宿主侧统一经 `bridge-host` → Hub → gen 协议调用端点）。`adaptors/ts/endpoint.ts`（echo 演示端点）由 bridge-runtime 取代——它是 gen runtime 的第一个真实端点。方法面（capability 清单）按命令切片在 schema/golden 登记。 |

### 0.1 一句话看懂

```text
                       ┌── 总包模式（行为不变）──────────────────────────┐
npm i @lania-cli-v2/cli │  core 的 lan → 直接 spawn lania-cli（全量单二进制，静态链接 13 个命令 lib）│
                       └───────────────────────────────────────────────┘
                       ┌── 按需模式（原子化）────────────────────────────┐
npm i core cmd-env     │  core 的 lan → 查表 → spawn lan-env（只链接 env + host-core + runex lib）│
npm i core cmd-env cmd-build │  同上，两个 lan-<cmd> 并存，bin 不冲突                    │
                       └───────────────────────────────────────────────┘
```

两种模式共用**同一个 `lan` 入口**；区别只在 launcher 检测到全量二进制时走直启分支。

---

## 1. 现状结论与「二进制误解」澄清

### 1.1 结构与 workspace

| 项                  | 事实                                                                                   | 证据                                                                           |
| ------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| 顶层按语言分目录    | `rust/`（Cargo workspace）、`ts/`（TS 源码 + 嵌套 pnpm workspace）、`npm/`（发布包装） | `README.md` Architecture；根 `pnpm-workspace.yaml` = `npm/*` + `ts/packages/*` |
| 三套 workspace 并存 | 根 pnpm、`ts/pnpm-workspace.yaml`、`rust/Cargo.toml`                                   | 三个文件                                                                       |
| Cargo 全局版本锁    | `[workspace.package] version = 0.0.3`，42 个手写 members；`runex` 例外 `0.1.0`         | `rust/Cargo.toml:2-52`                                                         |
| TS 构建             | 纯 `tsc` → `dist/`，**无 bundler**；无 postinstall                                     | `ts/tsconfig.base.json`、各 `ts/packages/*/package.json`                       |

### 1.2 单二进制装配

- `rust/crates/lania-cli/Cargo.toml:8-38` 静态依赖全部 13 个 `lania-plugins-command-*` + `lania-host` + `lania-command` + `runex` + `lania-agent` + `lania-node-bridge` + `lania-rs-bridge` + `lania-workflows` + `lania-preferences` 等，**没有 feature gate、没有可选依赖**。
- `rust/crates/lania-cli/src/main.rs:62-75` 硬编码 `host.register_plugin(...)` 13 次。`:59-61` 明确注释：注册顺序影响「CLI 命令树展示顺序」与「hooks/capabilities/handlers 注册时机」。
- `main.rs:47` 无条件 `activate_managed_runtimes()`（`:290` 调 `runex::RuntimeHome::resolve().activate()`，把 `~/.runex/shims` 前置 PATH，见 `self-managed-runtime-manager.zh-CN.md` §11.4）。
- `main.rs:53-56` 同一二进制兼作 Rust 运行时 bridge（`lan __lania_rs_bridge`），在任何宿主装配之前短路。
- `main.rs:86` `load_lan_config_snapshot_from_cwd_async` 与 `:110` `bootstrap_project_extensions_from_cwd_async` 会触发 `config.loadLan` 走 Node bridge（`rust/crates/lania-host/src/runtime/config/loading.rs:117`）；失败被 `.ok()` 吞掉，但**事实上每条命令启动都会尝试拉起 Node bridge 子进程**。

### 1.3 骨架与 payload 的重量

- 13/13 命令 crate 都依赖 `lania-command` + `lania-host`。`rust/crates/lania-host/Cargo.toml:8-35` 依赖 5 个语言 bridge（`lania-node-bridge` / `lania-py-bridge` / `lania-go-bridge` / `lania-dart-bridge` / `lania-rs-bridge`）+ 20 余能力 crate；`src/runtime.rs` 的 `HostRuntimeServices` 硬持有 `node_bridge` 与 `bridges`；`src/runtime/bridges.rs` 手写 `RuntimeKind` 7 变体与 4 个 bridge 可执行名。
- `rust/crates/lania-command/Cargo.toml:8-11` 运行时只依赖 `clap` / `serde` / `serde_json`，**是干净 SDK**；但 `:13-29` 的 `[dev-dependencies]` 反向引用 12 个命令 crate + `lania-host` + `lania-agent`（本地化覆盖测试组装真实命令树），形成 dev-only 反向闭环。
- `scripts/pack.mjs:118-194` 把 `ts/packages/{node-bridge,templates,agent}/dist` 全量拷进 `npm/cli/lib/node-bridge/`，合成 `@lania-cli-v2/node-bridge-payload` 并 `npm install --omit=dev` 安装 `ejs` / `tsx` / `yaml` / `@earendil-works/pi-agent-core` / `pi-ai`。
- `ts/packages/node-bridge/src/core/plugin-registry.ts:15` 顶层静态 `import { agentPlugin } from '@lania-cli-v2/agent'`；`:59-74` `builtinPlugins` 是 12 项静态数组（含 `agentBridgePlugin`）。`agent-architecture.zh-CN.md` §3.2 / A4 已把「改动态 import」列为待办，**当前未实施**。
- `LANIA_NODE_BRIDGE_DIR` 由 `npm/cli/bin/lan.mjs:149` 唯一注入。

结论：一条 `lan env list` 也隐式背上了 agent + templates + ejs 的全量 payload，以及 5 个语言 bridge 的 Rust 依赖。

### 1.4 分发与硬编码链

| 项           | 事实                                                                                                                                                                                                                               |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 现有 npm 包  | `@lania-cli-v2/cli`（`bin.lan` → `bin/lan.mjs`，`files: ["bin","lib"]`）+ 3 个 `@lania-cli-v2/cli-<platform>`（`os`/`cpu`、`files:["bin"]`、**无 `bin` 字段**，内含 `bin/lania-cli[.exe]`）                                        |
| 定位方式     | `lan.mjs:25-47` 按 `process.platform`/`arch` 分 3 个分支，`require.resolve('<pkg>/package.json')` 定位二进制                                                                                                                       |
| 环境注入     | `lan.mjs:145-153` `spawn(..., { stdio:'inherit' })` + `LANIA_NODE_BRIDGE_DIR` / `LANIA_PRODUCT_ROOT` / `LANIA_RUNTIME_MODE`；`:163-170` 退出码与信号转发                                                                           |
| 无安装期下载 | 全仓 `package.json` 只有根 `prepare: husky`，无 pre/post/install 脚本；跨平台产物靠构建期 `scripts/fetch-platform-binaries.mjs` 从 GitHub Releases 拉取                                                                            |
| pack 硬编码  | `pack.mjs:119/123-125/127-129` 写死 payload 与 3 个 dist；`:323` 写死 `rust/target/release/lania-cli`；`:361` 只有一条 `cargo build --release -p lania-cli`；`:90-116` `assertVersionSync`；`:297-319` `assertStagedBinaryVersion` |
| 平台包派生   | `pack.mjs:208-234` / `release.mjs:21-39` / `clean.mjs` 都靠 `npm/cli/package.json` 的 `optionalDependencies` 前缀 `@lania-cli-v2/cli-` 字符串过滤                                                                                  |
| 发布硬编码   | `release.mjs:21-39` `discoverAllPackages()` 写死 4 个包路径；`publish-all.mjs:25-28` 写死 4 行顺序；`.changeset/config.json` `fixed` 组锁 6 包同版本                                                                               |
| CI           | `.github/workflows/release-binaries.yml` 3 平台 matrix 写死 `-p lania-cli`；`release-runex.yml` 5 平台、tag `runex-v*`，Windows `-C target-feature=+crt-static`                                                                    |

### 1.5 没有的能力（不要假设存在）

- **没有**内建命令的动态发现或 manifest 驱动注册：`rust/crates/lania-host/src/runtime/catalog.rs:78-80` 的 `discover_builtin_plugins()` 只回显已注册 registry 的 meta。项目级动态命令（`lan.config.*` / `.lania/schemas` 目录扫描）只服务用户项目，`../architecture/代码链路与数据结构补充.zh-CN.md:666` 明确「不做完全的热插拔」。
- **没有** bundler、**没有** postinstall、**没有** dylib 热插拔（`bridge-layer.zh-CN.md` §6 已定：Rust ABI 不稳定）。
- **没有** `packages/` 目录。

### 1.6 已核实的可复用支点

| 事实                                                                               | 位置                                                           | 用途                                                                                |
| ---------------------------------------------------------------------------------- | -------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `CommandSpec` 已 `Serialize/Deserialize`                                           | `rust/crates/lania-command/src/spec.rs:8-9`                    | `--describe` JSON 与 help 聚合可直接序列化，无需新造协议                            |
| 13 个命令 crate 均已有 `pub fn spec() -> CommandSpec`                              | 各 `lania-plugins-command-*/src/lib.rs` / `spec*.rs`           | 提升为 API 契约                                                                     |
| `lania-plugins-command-locale` 暴露的命令名是 **`config`** 不是 `locale`           | `.../src/lib.rs:49-50` `CommandSpec::new("config", ...)`       | 命令名必须以 `spec().name` 为唯一事实源                                             |
| `runex` 已是 lib + bin，且 `pub use runex_core::{..., RuntimeHome, ShimHost, ...}` | `rust/crates/runex/Cargo.toml:11-13`、`src/api.rs:20-25,81-83` | `ShimHost::lan()` / `::binary("runex", exe)` 已存在，新增第三种模式是扩展而非新机制 |
| `runex-core` 冻结导出面 + 编译期守卫 `tests::frozen_export_surface_is_intact`      | `self-managed-runtime-manager.zh-CN.md` §11.3                  | 命令包 API 守卫照抄此模式                                                           |
| `agent` 三层拆包已完成、`lania-agent*` 零 lania 依赖                               | `agent-architecture.zh-CN.md` §3.1 / §11                       | 主包迁移时保持该边界                                                                |

### 1.7 关键澄清：不需要「把二进制打进去」

需求讨论中提出的「env 命令可能需要把 runex 的二进制打进去，其他命令也一样，也可能需要把 rust 宿主层的二进制打包进去」——**这个前提不成立**：

| 担忧                         | 真相                                                                                                                                                                                                              | 真正要做的事                                                   |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| env 包要嵌 `runex` 二进制    | `lania-plugins-command-env/Cargo.toml:13` 依赖的是 `runex` **lib**；所有动作调 `runex::api::Runex`（`self-managed-runtime-manager.zh-CN.md` §13）。`runex` 的 `[[bin]]` 只是同一 crate 的第二个产物，二者互不嵌套 | `lan-env` 静态链接 `runex` lib 即可                            |
| 其它命令要嵌 Rust 宿主二进制 | 命令 crate 依赖的是 `lania-host` **lib**（`Plugin` / `CommandHandler` / `CommandExecutionContext`），不是可执行文件                                                                                               | 把 `lania-host` 拆薄，让纯 Rust 命令不再被拖进 5 个语言 bridge |

`lania-cli` 与 13 个命令 crate 之间、命令与主包之间共享的都是 **Cargo 静态链接的 rlib**。**因此本方案的核心不是设计二进制嵌套策略，而是「拆依赖」**：让每个 `lan-<cmd>` 只链接它需要的那几个 lib，让不需要 JS 的命令完全不带 TS payload。这也是「只装 env 不装全量体量」能成立的唯一路径。

---

## 2. 目标、原则与不变量

**目标**：在保持 `@lania-cli-v2/cli` 全量总包行为逐字节不变的前提下，把仓库重构为 `packages/*` 单一容器；11 个命令各自成为可独立安装（CLI）、可被总包静态链接（API）的原子包（`template` 作为 `create` 二级命令不单独成包；`config` 按决策 #11 保留在总包，不独立成包，每个命令子包自带 `config` 二级命令管理本包偏好）；`runex` / `agent` / `bridge-hub` 成为独立双形态主包。

**原则（按优先级）**：

1. **总包零行为变化**：`lania-cli` 仍是一个进程、静态链接 12 个命令 lib、不经过调度器、不新增进程跳转。`main.rs` 的 `output/` 模块与 `profile.rs` 中的渲染逻辑、以及 12 条命令输出是硬基线。
2. **事实源单一**：`packages/*/lania.package.json` 描述「命令名 / API 包 / CLI 二进制 / 平台包 / payload 切片 / 版本族」；workspace、脚本、CI 全部由它派生。
3. **API 与 CLI 同源**：同一份 `spec()` + 同一份 handler 代码，既被 `lania-cli` 通过 `factory()` 静态链接，也被 `lan-<cmd>` 薄壳调用；输出渲染统一由 `lania-cli-kit` 提供。
4. **不做运行时命令动态发现**：调度器只按构建期生成的**已知命令表**探测安装状态（`require.resolve`，与 `lan.mjs:51-64` 同构）；不做目录扫描式发现、不做 dylib 热插拔。
5. **依赖单向无环**：主包零 CLI/宿主耦合；命令包 → foundation + 所需主包；总包 → 全部命令 lib；core → 命令 bin（仅 npm 层 spawn，无 Rust 编译期依赖）。
6. **沿用既有分发契约**：`os`/`cpu`/`optionalDependencies`/无 postinstall/安装期不联网/平台包无 `bin` 字段/`require.resolve` 定位，全部保留。
7. **四个分发边界稳定**（`框架化与产品化总览.zh-CN.md` §6）：npm wrapper 暴露命令名 / Rust binary 负责命令执行与宿主治理 / node-bridge payload 负责 JS runtime 能力 / templates payload 负责模板资产。
8. **Rust 保持命令树主控权**（同上 §14）：命令树由 `CommandSpec` 编译期生成，不由 npm 层拼装。

**全程不变量**：任何施工阶段结束，都必须能 `pnpm pack && pnpm publish:all` 产出与现状等价的全量 `@lania-cli-v2/cli`；golden 回归失败即停在该阶段回滚。

---

## 3. 目录与包清单

### 3.1 顶层收敛

```text
<repo root>/
├── Cargo.toml                     # 唯一 Cargo workspace（上移，glob members）
├── pnpm-workspace.yaml            # 唯一 pnpm workspace
├── packages/                      # 新增：所有包的统一容器
├── scripts/                       # manifest 驱动的 pack / release / publish / gen-*
├── docs/design/                   # 本文
└── （迁入后删除）ts/  rust/  npm/  ts/pnpm-workspace.yaml
```

```yaml
# pnpm-workspace.yaml
packages:
  - 'packages/*/npm/*'
  - 'packages/*/ts/*'
```

```toml
# Cargo.toml（根）
[workspace]
members = ["packages/*/rust/*"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
authors = ["Lania CLI Team"]
# 删除 version —— 各 crate 显式声明所属版本族版本

[workspace.dependencies]   # 仅第三方依赖版本集中，沿用现有 rust/Cargo.toml:54-78
```

> 兜底：若单层 glob 不足以表达某主包 `rust/` 下再分目录，由 `scripts/gen-cargo-workspace.mjs` 从 manifest 的 `rust.cratePath` 生成 `members` 数组，CI 用 `cargo metadata --no-deps` 校验生成物与磁盘一致（仿 §11.3 冻结守卫思路）。**优先 glob，生成脚本用于校验与兜底**。

### 3.2 完整目录树（四类包）

```text
packages/
├── schemas/
│   └── lania.package.schema.json        # manifest JSON Schema（构建期校验）
│
├── foundation/                          # ① 通用骨架（不发布或仅内部 crate）
│   ├── command/rust/                    # crate lania-command【干净 SDK + 新 API 契约】
│   ├── cli-kit/rust/                    # crate lania-cli-kit【从 lania-cli 抽出 output/ + profile.rs】
│   ├── host-core/rust/                  # crate lania-host-core【零语言 bridge / 零 bridge-hub 的 HostRuntime】
│   ├── runex-home/rust/                 # crate runex-home【RuntimeHome::resolve + activate，极小】
│   ├── command-conformance/rust/        # 测试专用 crate【承接 lania-command dev-deps 的本地化覆盖测试】
│   ├── bridge-runtime/ts/ + npm/        # @lania-cli-v2/bridge-runtime【最小 stdio 协议核 + 动态 registry】
│   ├── bridge-slices/*/ts/ + npm/       # 12 个 @lania-cli-v2/bridge-slice-<name>
│   └── capabilities/rust/*              # 现有能力 crate 逐一迁移（1:1 目录，crate 名不变）：
│        #   13 个宿主直接依赖的 leaf：lania-config / hooks / exec / fs / git / logger / pm /
│        #     progress / prompt / runtime / task / workflows / preferences（均 bridge-free）
│        #   其余能力 crate（format / validate / presentation 等）按同一规则归入此处
│        #   注：lania-bridge-support 是语言 bridge 的共享基建，归 products/bridge-hub/（见 §3.2 主包段）
│        #   注：5 个语言 bridge crate（node/py/go/dart/rs-bridge）整体并入
│        #        products/bridge-hub/adaptors/<lang>/（bridge-layer §4 规定）
│        #   注：非 ts 配置读取直接复用 lania-config（纯 Rust，零 bridge），不另起 config-native crate
│
├── products/                            # ② 3 个主包（Rust lib API + 独立 bin + 可发 npm）
│   ├── runex/
│   │   ├── rust/runex-core/             # crate runex-core（零 lania 依赖，§11 冻结导出面）
│   │   ├── rust/runex/                  # crate runex（lib + [[bin]] runex，独立版本 0.1.0）
│   │   └── npm/                         # @lania-cli-v2/runex wrapper + 5 平台包
│   ├── agent/
│   │   ├── rust/lania-agent-core/       # 三层第 1 层，零 lania 依赖
│   │   ├── rust/lania-agent/            # 三层第 2 层，run(AgentOptions, Box<dyn AgentSessionClient>, ...)
│   │   ├── ts/agent/                    # @lania-cli-v2/agent（pi 引擎）
│   │   └── npm/agent/                   # 可选独立 bin 分发
│   └── bridge-hub/
│       ├── protocol/                    # schema.json + gen.mjs（旧 bridge/protocol 迁入）
│       ├── rust/lania-bridge-{protocol,bus,core,host}/   # host = 宿主侧集成（含 SharedBridgeRegistry 装配）
│       ├── adaptors/{ts,py,go,dart,rust}/   # 5 个内置端点 adaptor（原 lania-{node,py,go,dart,rs}-bridge 并入此处，bridge-layer §4）
│       ├── ts/                          # TS 端点 adaptor（endpoint.ts，由 npm payload 引用）
│       └── npm/bridge-hub/              # @lania-cli-v2/bridge-hub + 可选 lan-hub bin
│
├── commands/                            # ③ 11 个命令包（template 并入 create；config 按决策 #11 留总包不成包）
│   ├── dev/ build/ lint/ create/ add/ generate/ release/ git/
│   ├── tools/ env/ pi/             # 每包自带 config 二级命令（包级偏好，§10.2），crate 内自有 config.rs
│   │   ├── lania.package.json           # 含 configSchema（本包自有偏好 key + 类型，构建期校验）
│   │   ├── rust/                        # lib crate + [[bin]] lan-<cmd>
│   │   ├── ts/                          # 仅确需自有 TS 资产时存在
│   │   ├── npm/cmd-<cmd>/               # @lania-cli-v2/cmd-<cmd>：bin/lan-<cmd>.mjs + lib/bridge/
│   │   ├── npm/cmd-<cmd>-{darwin-arm64,linux-x64,win32-x64}/   # os/cpu + files:["bin"]
│   │   └── describes/<cmd>.json         # 构建期由 spec() 序列化产出，随包发布
│   # 注：`config` 命令（crate lania-plugins-command-locale）按决策 #11 不成子包：
│   #     过渡期保留在 rust/crates/，阶段 5c 随总包 Rust 侧一并迁入 distribution/cli/rust/；
│   #     不产生 lan-config 独立二进制与 cmd-config npm 包。
│
├── runtime/                             # ④ 可扩展宿主设施（动态运行时命令 + 脚手架封装命令的执行宿主）
│   ├── lania.package.json               # kind: "command-runtime"（非 builtin 命令，不被 known-commands 收录）
│   ├── rust/lania-command-runtime/      # crate：链接 全量 host-core + bridge-host + node-bridge
│   │   └── bin/lan-runtime.rs           # [[bin]] lan-runtime：project 级 runtimeCommands 的执行宿主
│   ├── ts/                              # 复用 node-bridge 的 dynamic-commands + scaffold + templates 切片
│   └── npm/runtime/                     # @lania-cli-v2/runtime：bin/lan-runtime.mjs + lib/bridge/(动态+脚手架切片)
│       └── npm/runtime-{darwin-arm64,linux-x64,win32-x64}/
│
└── distribution/                        # ⑤ 分发包装包
    ├── core/npm/core/                   # @lania-cli-v2/core：唯一 lan bin + launcher + known-commands.json
    ├── cli/npm/cli/                     # @lania-cli-v2/cli（总包：全量 payload + 3 平台 optionalDependencies）
    │   └── rust/                        # 总包 Rust 侧最终落点（阶段 5c 从顶层 rust/ 迁入，§8 计划 1.5）：
    │       #   lania-cli（总包硬基线，12 命令逐字节不变）
    │       #   lania-host（facade 聚合别名，A.5(6)，长期保留）
    │       #   lania-plugins-command-locale（config 命令，决策 #11，只随总包发布）
    ├── cli/npm/cli-{darwin-arm64,linux-x64,win32-x64}/
    └── rs-bridge/npm/rs-bridge/         # @lania-cli-v2/rs-bridge：独立 lan-rs-bridge bin
```

### 3.3 11 个命令包对照表（template 并入 create；config 留总包，见决策 #11）

> 「初始切片」是设计起点，施工时须按各命令实际 bridge 方法调用逐条核对（`rust/crates/lania-plugins-command-*/src/**`）。

| 包目录              | 命令名 `spec().name` | Cargo crate（lib / bin）        | npm 主包       | 平台包前缀      | Rust 仓内依赖                     | 初始 TS 切片                                       |
| ------------------- | -------------------- | ------------------------------- | -------------- | --------------- | --------------------------------- | -------------------------------------------------- |
| `commands/dev`      | `dev`                | `...-dev` / `lan-dev`           | `cmd-dev`      | `cmd-dev-`      | node-bridge                       | compiler, config, lifecycle, system, toolchain     |
| `commands/build`    | `build`              | `...-build` / `lan-build`       | `cmd-build`    | `cmd-build-`    | node-bridge, logger, presentation | compiler, config, lint, product, system, toolchain |
| `commands/lint`     | `lint`               | `...-lint` / `lan-lint`         | `cmd-lint`     | `cmd-lint-`     | node-bridge, logger, presentation | config, lint, toolchain                            |
| `commands/create`   | `create`             | `...-create` / `lan-create`     | `cmd-create`   | `cmd-create-`   | workflows, logger                 | config, template, product, commitizen, commitlint  |
| `commands/add`      | `add`                | `...-add` / `lan-add`           | `cmd-add`      | `cmd-add-`      | workflows                         | config, template, product                          |
| `commands/generate` | `generate`           | `...-generate` / `lan-generate` | `cmd-generate` | `cmd-generate-` | node-bridge, prompt, workflows    | config, template, dynamic-commands                 |
| `commands/release`  | `release`            | `...-release` / `lan-release`   | `cmd-release`  | `cmd-release-`  | workflows                         | config, product, commitizen, commitlint, lifecycle |
| `commands/git`      | `git`                | `...-git` / `lan-git`           | `cmd-git`      | `cmd-git-`      | workflows                         | config, commitizen, commitlint, lifecycle          |
| `commands/tools`    | `tools`              | `...-tools` / `lan-tools`       | `cmd-tools`    | `cmd-tools-`    | node-bridge, exec, runtime        | config, system, toolchain                          |
| **`commands/env`**  | **`env`**            | `...-env` / `lan-env`           | **`cmd-env`**  | `cmd-env-`      | **`runex`（lib）**                | **无**（`nodeBridge: false`）                      |
| `commands/pi`       | `pi`                 | `...-pi` / `lan-pi`             | `cmd-pi`       | `cmd-pi-`       | agent, agent-core, node-bridge    | config, agent, system                              |

> **`config` 命令不在上表**（决策 #11）：crate `lania-plugins-command-locale` 保留在 `rust/crates/`，只随总包发布，不产生 `lan-config` 独立二进制、`cmd-config` npm 包或 `describes/config.json`。每个命令子包自带 `config` 二级命令管理本包偏好（机制见 §10.2 修订版）。

**命名分叉警示**：**`manifest.command` 是唯一事实源，所有 `gen-*` 脚本禁止从目录名或 crate 名推断命令名**；CI 加断言 `manifest.command == describes/<cmd>.json 里的 spec().name`。（历史背景：原 `commands/config` 方案中目录/npm 名用 `config` 而 Cargo crate 沿用 `...-locale`，该分叉随 config 不成子包而消除，规则保留适用于其余包。）

> **`template` 命令已去除，能力并入 `create` 二级命令（评审决定）**：顶级 `template` 命令不再独立存在，也不再作为子包 `commands/template` 发布；其全部能力（基于 ejs 的 `templatePlugin` 渲染，对应 `bridge-slice-template`，见 §5.2 B.4）直接**迁移进 `create`**，以 `lan create template ...` 二级命令提供。因此：
>
> 1. 顶层 builtin 命令由 **13 降为 12**，`known-commands.json` 与 `BUILTIN` 数组相应为 **12 项**（§5.3 C.7 / §5.4 D.1）。
> 2. `template` 命令原有的切片需求由 `create` 承接：`create` 的 `template` 切片保留，服务于 `create` 与其 `create template` 子命令（§5.2 B.4 映射表中 `create` 行已含 `template`）。
> 3. 历史 `lan template <args>` 的 golden 输出需重录为 `lan create template <args>`，纳入 §7 不变量（"12 命令输出逐字节不变"含此迁移）。
> 4. **`templates` 脚手架资产包（`@lania-cli-v2/templates`，payload 切片名 `templates`）不变**，仍被 `create` / `add` / `generate` 复用；本文中凡出现 `templates`（复数、资产/切片）均指该包，与已删除的 `template` 命令无关。
> 5. **迁移落点**：`create` 的 `CommandSpec.subcommands` 增加 `template` 一项（describe/help 自然呈现为 `lan create template`），`CommandPlugin::execute` 按 `CommandContext.handler_id` 分派到原 `commands/template` 的 handler 逻辑（handler 代码零改动，仅注册点从独立 crate 迁到 `create` 内部）；`create` 的 `bridgeSlices` 已含 `template`（ejs），无需新增切片，并复用 §5.7 G 的 `ctx.config` / `ctx.script`。

### 3.4 主包 / 分发包对照表

| 类别 | 目录                             | npm 名                                                      | Cargo crate                                                                | 产物形态                                                                                          | 版本族       |
| ---- | -------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ------------ |
| 主包 | `products/runex`                 | `@lania-cli-v2/runex` + 5 平台包                            | `runex-core`(lib) / `runex`(lib + `[[bin]] runex`)                         | 独立 `runex` 二进制 5 平台，tag `runex-v*`                                                        | `runex`      |
| 主包 | `products/agent`                 | `@lania-cli-v2/agent`（TS）/ 可选 `@lania-cli-v2/agent-cli` | `lania-agent-core` / `lania-agent`                                         | Rust lib API + 可选独立 bin                                                                       | `agent`      |
| 主包 | `products/bridge-hub`            | `@lania-cli-v2/bridge-hub`                                  | 4 个 `lania-bridge-*`（+ 内置 `adaptors/{ts,py,go,dart,rust}` 5 语言端点） | Rust lib API + 可选 `lan-hub` bin（默认不启用）                                                   | `bridge-hub` |
| 分发 | `distribution/core`              | `@lania-cli-v2/core`                                        | 无（纯 Node launcher）                                                     | 唯一 `lan` bin + `lib/known-commands.json` + `lib/compat.json`                                    | `lan`        |
| 分发 | `distribution/cli`               | `@lania-cli-v2/cli` + 3 `cli-*`                             | `lania-cli`（全量 bin）                                                    | 单一 `lania-cli[.exe]`，行为不变                                                                  | `lan`        |
| 分发 | `distribution/rs-bridge`（弃用） | —                                                           | —                                                                          | 已被 bridge-hub 的 Rust 进程内端点取代，`lan __lania_rs_bridge` 不再需要独立二进制（见 §5.1 A.3） | —            |

### 3.5 构建期硬约束

`lania-command/Cargo.toml` 的 `[dev-dependencies]` 必须归零。现有本地化覆盖测试（组装 13 命令真实树）上移到 `packages/foundation/command-conformance/rust/`（测试专用、不参与任何产物），或改为读取构建期生成的 `specs.json`。**推荐前者**以保留真实树覆盖度（见 §9.5）。

---

## 4. 依赖方向

```text
        ┌──────────────────────────────────────────────────────────────┐
        │ ④ distribution（仅 npm 层，运行时 spawn）                       │
        │   core ─────► cmd-*/bin（lan-<cmd>）      cli ─────► lania-cli │
        └──────────────────────▲───────────────────────────────────────┘
                               │ npm spawn / require.resolve（无 Rust 编译期依赖）
   ┌──────────────────────────┴───────────────────────────────────────┐
   │ ③ 12 × commands/<cmd>                                              │
   │   rust: lib(lania-plugins-command-*) + [[bin]] lan-<cmd>           │
   │   依赖 → foundation + 所需主包（+ 需语言运行时则依赖 bridge-hub 的 bridge-host）│
   └───────▲───────────────────────────▲───────────────────────────────┘
           │ API 静态链接（factory()）  │ Rust lib 依赖
   ┌───────┴────────────┐     ┌────────┴───────────────────────────────┐
   │ lania-cli（总包）   │     │ ② products（3 主包，零 CLI/宿主耦合）    │
   │ 依赖全部 13 lib     │     │  runex ← runex-core ← runex-home        │
   │ host-core + bridge-host 全 adaptor │ │  agent ← agent-core                     │
   │                     │     │  bridge-hub ← protocol/bus/core         │
   └───────▲─────────────┘     └────────▲───────────────────────────────┘
           │                            │
   ┌───────┴────────────────────────────┴───────────────────────────────┐
│ ① foundation（单向向下，禁止反向）                                   │
│  lania-command（clap/serde/serde_json）· lania-cli-kit              │
│  lania-host-core（零语言 bridge / 零 bridge-hub）                    │
│  runex-home · lania-config（纯 Rust，零 bridge，复用现有 crate）· capabilities/*  │
│  bridge-runtime + bridge-slices（TS，仅被 npm payload 引用）         │
│  命令需桥接时依赖 lania-bridge-host（bridge-hub 已内聚 5 语言 adaptor）│
   └────────────────────────────────────────────────────────────────────┘
```

**硬规则**：

- 箭头恒为「上层 → 下层」。`lania-command` / `lania-cli-kit` / `runex-home` / `runex-core` / `lania-agent-core` / `lania-bridge-core` **不得**反向依赖任何命令 crate、总包或 core。
- 主包零 CLI/宿主耦合：`runex` / `runex-core` / `lania-agent*` / `lania-bridge-*` 不依赖 `@lania-cli-v2/core`、`lania-cli`、任何 `lania-plugins-command-*`（`agent-architecture.zh-CN.md` §3.1 的三层边界保持不变）。
- `lania-plugins-command-pi` 是**唯一**认识宿主的 agent 适配层；`lania-agent` 不认识它。
- 总包 → 全部 12 命令 lib（编译期 `factory()` 静态链接）；core → 命令 bin（**仅 npm 运行时 spawn，无 Rust 依赖**）。
- 打破 dev-dep 闭环后，`cargo tree -p lania-command -e normal` 只出现 clap/serde/serde_json。

---

## 5. 核心机制

### 5.1 A. 拆骨架，让命令二进制变轻

#### A.1 `lania-host` → `host-core`，5 语言 bridge **内聚进 bridge-hub**

现状硬耦合点：`rust/crates/lania-host/src/runtime.rs:16,63-66` 硬持有 `NodeBridgeClient` / `SharedBridgeRegistry`；`src/runtime/bridges.rs:22` 直接 `use lania_node_bridge::*`；`src/runtime/config/loading.rs:117` 通过 `services.node_bridge` 发 `config.loadLan`；`main.rs:121-143` 4 次 `register_{python,go,dart,rust}_bridge(...)`；`lania-host/Cargo.toml:24-31` 硬依赖 5 个 `lania-*-bridge` crate。

**关键修正（bridge-layer §4 / §5 过渡第 3 步）**：5 个语言 bridge **不是**宿主层的 feature，而是 **bridge-hub 包内的内置端点 adaptor**（`packages/products/bridge-hub/adaptors/{ts,py,go,dart,rust}/`，对应原 `lania-node-bridge` / `lania-py-bridge` / `lania-go-bridge` / `lania-dart-bridge` / `lania-rs-bridge` 五个 crate 整体并入）。宿主删掉 5 条语言硬依赖，改为依赖 `lania-bridge-host`（bridge-hub 的宿主侧集成 crate）；命令包需要桥接语言运行时时，依赖 `bridge-host` 即可，**不再 import 任何具体语言 bridge crate**（bridge-layer §5）。Rust 端点走进程内 channel，TS/Py/Go/Dart 端点走 stdio 子进程，由 `lania-bridge-host` 统一装配。

```text
host-core（零语言 bridge / 零 bridge-hub）
  ├─ Plugin / PluginSetupContext / PluginMeta
  ├─ CommandRegistry / CommandHandlerRegistry / HookBus / CapabilityContainer
  ├─ 纯 Rust services：exec / fs / git / logger / pm / progress / prompt /
  │                    runtime / task / workflows / preferences / config(Rust 部分)
  ├─ HostRuntime：bridge 字段为 Option<Arc<dyn BridgeProvider>>
  │               （trait 由 lania-bridge-host 实现并注入，host-core 只认 trait）
  └─ 实现 lania_command::HostApi

packages/products/bridge-hub/（桥接层主包，内聚 5 adaptor）
  ├─ rust/lania-bridge-{protocol,bus,core,host}
  │     host = Hub 进程装配 + SharedBridgeRegistry（原 RuntimeKind 7 变体迁入此处）
  │            + register_python/go/dart/rust（main.rs:121-143 实现迁入）
  │            + config.loadLan / bootstrap_project_extensions 的 bridge 实现
  └─ adaptors/{ts,py,go,dart,rust}/   # 5 个内置端点 adaptor
                                     # ts→npm payload；py/go/dart include_str! 落盘；rust→进程内 channel
```

`config.loadLan` 的处理：`host-core` 把 `load_lan_config_snapshot_from_cwd_async` 抽象为 `ConfigSource::load(cwd) -> Option<LanConfigSnapshot>`；需要 `.ts` 配置的项目，由 `bridge-host` 经 node adaptor 求值（保持现状语义），纯 `.yaml/.json` 由现有 `lania-config`（纯 Rust、零 bridge，不另起 `config-native` crate）实现。`main.rs:86-89` 的 `.ok()` 吞错语义保持不变。

初始依赖矩阵（写进 manifest 的 `rust.dependencies`，**不再有 bridge feature**）：

| 命令                            | 额外宿主依赖                                             | 说明                             |
| ------------------------------- | -------------------------------------------------------- | -------------------------------- |
| `env` / `config`                | 无 `bridge-host`                                         | 只依赖 `host-core`，零语言运行时 |
| `dev/build/lint/generate/tools` | `lania-bridge-host`（node adaptor 经 stdio）             | 需 Node 运行时                   |
| `create/add/release/git`        | `lania-bridge-host` + `lania-workflows`                  | 需 Node 运行时（workflows 侧）   |
| `pi`                            | `lania-bridge-host` + `lania-agent` + `lania-agent-core` | 需 agent 端点（经 Hub）          |
| 总包 `lania-cli`                | `host-core` 全量 + `bridge-host` 全 adaptor              | 行为不变                         |

#### A.2 `runex-home`：PATH 前置不拉下载引擎

现状 `main.rs:290` 直接 `runex::RuntimeHome::resolve()`，把整个 `runex`（`runex-core` 下载/校验/解压 + `ratatui` / `crossterm` + `ureq` + 压缩库）拉进**每一个**命令二进制。

```text
新 crate runex-home（零第三方依赖或仅 std/libc）
  pub struct RuntimeHome { ... }
  impl RuntimeHome { pub fn resolve() -> Result<Self>; pub fn activate(&self); }
  pub fn activate_managed_runtimes() { if let Ok(h) = RuntimeHome::resolve() { h.activate(); } }
```

- `runex-core` 依赖 `runex-home` 并 `pub use runex_home::RuntimeHome`，**保持 `runex::api` 的 `pub use runex_core::{..., RuntimeHome, ...}` 导出面不变**（`self-managed-runtime-manager.zh-CN.md` §11.3 冻结清单）。
- 所有 `lan-<cmd>` 与 `lania-cli` 只依赖 `runex-home` 做 PATH 前置；只有 `env` 额外依赖 `runex` lib。
- `main.rs:47` 改为 `runex_home::activate_managed_runtimes();`。§11.4 的「所有子进程继承 PATH」「`which` 不得回退 PATH 扫描」约束因此对所有 `lan-<cmd>` 成立。

#### A.3 Rust 端点并入 bridge-hub，去掉独立 `lan-rs-bridge` 二进制

bridge-layer §4 规定 Rust 端点是 `lania-bridge-bus` 内的**进程内 channel**，直接连 Hub，无需独立子进程。因此 `rust/crates/lania-rs-bridge` 这个独立 crate **整体并入 `packages/products/bridge-hub/adaptors/rust/`**，宿主通过 `hub.attach("rust")` 进程内注册 Rust 端点。

- **按需模式**：命令二进制链接 `lania-bridge-host`，Rust 端点经进程内 channel 接入，不再有 `lan-rs-bridge[.exe]` 二进制，也不再复用 `std::env::current_exe()`。
- **总包模式**：`lania-cli` 同样经 `bridge-host` 进程内接入 Rust 端点，行为逐字节不变。

「剥离」的实质不是删掉协议循环，而是**不再让「Rust bridge 复用主二进制」成为所有命令必须携带完整 `lania-cli` 的理由**；代价是 `lan __lania_rs_bridge` 这个历史调用入口被桥接层标准 `attach` 取代。

#### A.4 `lania-cli-kit`：行为不变的支点

把 `rust/crates/lania-cli/src/output/` 目录与 `src/profile.rs` 整体抽成 crate `lania-cli-kit`：

```rust
pub fn normalize_os_args(...) -> Vec<String>;
pub fn resolve_effective_locale(...) -> String;
pub fn output_profile_from_sources(...) -> OutputProfile;
pub fn render_builtin_command(...) -> Option<BuiltinOutput>;
pub fn render_output_value(...) -> Result<String>;
pub fn execution_json_value(...) / error_json_value(...) / should_suppress_output(...);
pub fn cli_version() -> &'static str;

/// 子包薄壳唯一入口，与 lania-cli::main 共用同一套渲染
pub async fn run_single_command(
    spec: CommandSpec,
    factory: CommandPluginFactory,
    args: Vec<OsString>,
) -> i32;
```

`lania-cli/src/main.rs` 改为调用 kit，**渲染代码零复制**。这是「总包 12 命令输出逐字节不变」与「子包行为 = 单条命令行为」能同时成立的技术保证。

#### A.5 `lania-host` 模块级拆法细则（落地落点）

> A.1 给出的是「`host-core` + 5 bridge 内聚进 bridge-hub」的方向。本节给出**文件级落点、接缝设计与迁移顺序**，可直接作为施工清单。所有路径已对照 `rust/crates/lania-host/src/` 实际结构核实。

**（1）宿主层真实的耦合面（已核实）**

`lania-host` 不是单体，它只是一层聚合 crate。其 `Cargo.toml` 的 18 个依赖分两类：

- **13 个 bridge-free 能力 leaf**：`lania-config` / `lania-hooks` / `lania-exec` / `lania-fs` / `lania-git` / `lania-logger` / `lania-pm` / `lania-progress` / `lania-prompt` / `lania-runtime` / `lania-task` / `lania-workflows` / `lania-preferences`（均已验证不传递依赖任何 bridge）。
- **5 个语言 bridge**：`lania-node-bridge` / `lania-py-bridge` / `lania-go-bridge` / `lania-dart-bridge` / `lania-rs-bridge` —— 这 5 个 crate 整体并入 `packages/products/bridge-hub/adaptors/<lang>/`（见 §3.2 与 bridge-layer §4）。

真正的硬耦合只有 4 个点，全部集中在「桥接」上：

1. `src/runtime.rs:16` `use lania_node_bridge::NodeBridgeClient;` 与 `:64-66` `HostRuntimeServices` 硬持 `node_bridge: NodeBridgeClient` + `bridges: SharedBridgeRegistry`。
2. `src/lib.rs:33` 复用 `runtime::bridges` 的 `BridgeProgram, BridgeRegistry, RuntimeKind`。
3. `src/runtime/bridges.rs:22,26` 定义 `RuntimeKind`（7 变体 Node/Bun/Deno/Python/Go/Rust/Dart）+ `register_{python,go,dart,rust}_bridge`，并 `use lania_node_bridge::*`。
4. `src/runtime/config/loading.rs:117-120` 经 `services.node_bridge.call_async("config.loadLan", ...)` 求值 `.ts` 配置。

拆宿主层 = **把这 4 个点从 `host-core` 抽走，改走 trait 接缝**，其余 50+ 文件原样迁入 `host-core`。

**（2）模块 → 目标 crate 落点表**

| 源文件（`rust/crates/lania-host/src/...`）                                                                                     | 目标 crate        | 说明                                                                                                       |
| ------------------------------------------------------------------------------------------------------------------------------ | ----------------- | ---------------------------------------------------------------------------------------------------------- |
| `lib.rs`（除 `:33` 那一条 re-export）                                                                                          | `host-core`       | 其余 `pub use` 全保留                                                                                      |
| `runtime.rs`（去 `:16` import 与 `:64-66` 两字段）                                                                             | `host-core`       | 两字段替换为 `bridge: Option<Arc<dyn BridgeProvider>>`                                                     |
| `runtime/core.rs` `runtime/detect.rs` `runtime/execute.rs` `runtime/lifecycle.rs`                                              | `host-core`       | 纯宿主逻辑，零 bridge                                                                                      |
| `runtime/config.rs` `runtime/config/summary.rs` `runtime/config/project_extensions*.rs`                                        | `host-core`       | 配置逻辑归 host-core；仅 `loading.rs` 的调用点改                                                           |
| `runtime/config/loading.rs`                                                                                                    | `host-core`       | `:117` 改为走 `BridgeProvider::call`（见 (3)）                                                             |
| `runtime/dynamic/*`（command/hook_invokers/hook_registration/hooks/prompt/types）                                              | `host-core`       | 动态命令/项目插件，属宿主                                                                                  |
| `runtime/host_rpc/*`（agent_tools/exec/fs_domain/git/interaction/log/pm/runtime/task_progress）                                | `host-core`       | RPC 服务端骨架，归 host-core；具体 bridge 调用走 trait                                                     |
| `registry.rs` `plugin.rs` `capability.rs` `approval.rs` `guard.rs`                                                             | `host-core`       | 注册表/插件/能力/审批/守护                                                                                 |
| `execution.rs` `execution/context.rs` `execution/run_result.rs` `execution/types.rs` `execution/utils.rs` `execution/tests.rs` | `host-core`       | 执行框架                                                                                                   |
| `execution/bridge_api.rs` `execution/bridge_helpers.rs` `execution/bridge_helpers/*`                                           | `host-core`       | **改为参数化于 `&dyn BridgeProvider`**，不再 `use lania_node_bridge`                                       |
| `*.tests.rs` `runtime/tests.rs`                                                                                                | `host-core`       |                                                                                                            |
| **`runtime/bridges.rs`**（全文件）                                                                                             | **`bridge-host`** | `RuntimeKind` / `BridgeProgram` / `BridgeRegistry` / `register_*_bridge` / `SharedBridgeRegistry` 整体迁入 |
| **`lib.rs:33` 那条 `pub use runtime::bridges::{...}`**                                                                         | **`bridge-host`** | 由 bridge-host 重新导出，host-core 不再持有                                                                |

> 迁移口诀：**凡是名字带 `bridge` 且不带 `host_rpc` 的文件/符号 → `bridge-host`；其余 → `host-core`**。`host_rpc/*` 虽名为 rpc 但属于宿主侧的服务端骨架，且只通过 trait 调桥，故留 host-core。

**（3）接缝设计：`BridgeProvider` trait（host-core 定义，bridge-host 实现）**

```rust
// host-core/src/bridge.rs
pub trait BridgeProvider: Send + Sync + 'static {
    /// 取代 NodeBridgeClient::call_async + RuntimeKind 路由。
    /// host-core 永远只发 method 名 + JSON 参数，不感知具体语言。
    fn call(&self, method: &str, params: serde_json::Value)
        -> impl std::future::Future<Output = anyhow::Result<serde_json::Value>> + Send;

    /// 取代 main.rs:121-143 的 register_{python,go,dart,rust}_bridge。
    /// 语言 id 用字符串（"python"/"go"/...），host-core 不引用 RuntimeKind。
    fn register_language(&self, id: &str, spec: BridgeLaunchSpec) -> anyhow::Result<()>;
}

// HostRuntimeServices 的桥接字段由「两个具体类型」变为「一个可选 trait 对象」：
pub(super) struct HostRuntimeServices {
    pub(super) logger: LoggerService,
    pub(super) exec: ExecService,
    /* ... 13 个 leaf service 不变 ... */
    /// 桥接层（可选）：命令无需语言运行时时为 None。
    pub(super) bridge: Option<Arc<dyn BridgeProvider>>,
}
```

`loading.rs:117` 的调用点改为：

```rust
// 原：self.services.node_bridge.call_async(self.services.node_bridge.request("config.loadLan", params))
// 新：
match &self.services.bridge {
    Some(b) => b.call("config.loadLan", params).await?,
    None    => return Ok(None),   // 无桥接 → 走 lania-config 纯 Rust 回退（见 (5)）
}
```

`RuntimeKind` 与 `register_*_bridge` 在 bridge-host 内仍按现状实现（JS 簇共享一个 client、各语言懒 spawn），但**只对 bridge-host 可见**，host-core 与命令子包均不再 import 它们——这彻底斩断「命令二进制必须拉全量 bridge」的链路。

**（4）能力 leaf crate 的归宿**

- 13 个宿主直接依赖的 leaf（上 (1) 列出）**1:1 迁移**到 `packages/foundation/capabilities/<name>/`，crate 名不变、内部代码不动。迁移校验门：`grep -rn "lania_.*_bridge" rust/crates/lania-*/src` 在这 13 个 crate 内应为空（预期通过，因 leaf 不引 bridge）。
- `lania-bridge-support` **不是宿主能力**，它是语言 bridge 的共享基建（被 `lania-node-bridge` 等依赖），按 §3.2 修正并入 `packages/products/bridge-hub/`。
- 其余能力 crate（`lania-format` / `lania-validate` / `lania-presentation` 等，若存在）按同一规则归入 `foundation/capabilities/`，crate 名不变。

**（5）`config.loadLan` 的桥接路由（澄清 `config-native` 误解）**

`lania-config` 本身**已是纯 Rust、零 bridge 依赖**的 crate，它负责读 `.yaml/.json` 的 `lan.config.*`。所谓「非 ts 配置读取」**不需要新建 `config-native` crate**——直接复用 `lania-config` 即可：

- `host-core` 抽象一个薄 `ConfigSource`：`load(cwd) -> Option<LanConfigSnapshot>`，默认实现直接调 `lania-config`（纯 Rust，无 node）。
- 仅当项目配置是 `.ts`（需 `config.loadLan` 求值）或命令显式需要 bridge 时，host-core 才经 `BridgeProvider::call("config.loadLan", ...)` 走 bridge-host。
- `main.rs:86-89` 的 `.ok()` 吞错语义不变：桥接返回 `None`/Err 时回落到纯 Rust 快照，仍与现状行为一致。

→ **结论**：§3.2 目录树中的 `foundation/config-native` 应删除，非 ts 配置路径即 `lania-config` 本身。目录树已据此修正。

**（6）过渡兼容层：`lania-host` 降为 facade crate（关键去环手段）**

保留 `lania-host` 这个 crate 名，但把它从「聚合实现」降为「聚合 facade」，避免一次性改动所有 `use lania_host::*` 引发的雪崩：

```toml
# rust/crates/lania-host/Cargo.toml（重构后）
[dependencies]
lania-host-core  = { path = "../host-core" }
lania-bridge-host = { path = "../bridge-host" }
# 不再直接依赖 5 个语言 bridge 与 13 个 leaf（改由 host-core / bridge-host 传递）
```

```rust
// lania-host/src/lib.rs（重构后）
pub use lania_host_core::*;                       // Host / HostRuntime / registry / plugin / ...
pub use lania_bridge_host::{                       // 补回原 lib.rs:33 的 re-export
    BridgeProgram, BridgeRegistry, RuntimeKind,
    register_python_bridge, register_go_bridge,
    register_dart_bridge, register_rust_bridge,
};
/// 装配「带全量桥接」的 HostRuntime，等价旧 lania-host::HostRuntime::new()。
pub fn host_runtime_with_bridges() -> HostRuntime { /* host-core + bridge-host 注入 */ }
```

这样 **`lania-cli` 总包与所有历史调用点零改动即可继续编译**，而命令子包则**新代码直接依赖 `host-core` +（按需）`bridge-host`，不再 `use lania_host`**。facade 仅作为「聚合别名」长期保留，不会造成循环依赖（它只 re-export 下游 crate，下游不反向依赖它）。facade 与总包 `lania-cli`、`lania-plugins-command-locale` 同属「总包 Rust 侧」，阶段 5c 一并迁入 `packages/distribution/cli/rust/`（§3.2 / §8 计划 1.5）。

**（7）迁移顺序与每步验收门（建议每步一个独立 commit）**

| 步  | 动作                                                                                                                                                                                                 | 验收门（CI 必须绿）                                                                                                                          |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | 新建 `host-core`（迁入全部非 bridge 模块，`bridge: Option<Arc<dyn BridgeProvider>>`），`lania-host` 改为 facade 依赖 `host-core` + 临时 `bridge-host-stub`（trait 返回 unsupported）                 | 全量 `lania-cli` golden 不变；`cargo build -p lania-host` 通过                                                                               |
| 2   | 新建 `bridge-host`（迁入 `runtime/bridges.rs` + 具体 `BridgeProvider` impl + `register_*_bridge`），facade 替换 stub → `bridge-host`                                                                 | `lania-cli` golden 不变；`cargo tree -p lania-host` 仍只是 facade（不暴露具体 bridge 给命令层）                                              |
| 3   | `commands/env` 改依赖 `host-core` 且仅此（不用 `lania-host`、不用 `bridge-host`），产出 `lan-env`；`config` 命令按决策 #11 留总包，无独立 `lan-config`                                               | `cargo tree -p lan-env` **不得出现** `bridge-host` / `lania-node-bridge` / `lania-py/go/dart/rs-bridge`；`lan env` 在干净环境可用            |
| 4   | 6 个需 Node 的命令（`dev/build/lint/generate/tools` + `pi`；`create/add/release/git` 经 workflows 也需）改依赖 `host-core` + `bridge-host`                                                           | `cargo tree -p lan-build` 出现 `bridge-host` 但**不得出现** `lania-py/go/dart-bridge`（它们被桥接层封装在内部）；TS payload 切片按 §5.2 生效 |
| 5   | 打破 `lania-command` 的 dev-dependencies 环：把 localization coverage 集成测试从 `lania-command` 迁到 `lania-cli`（或新建 `lania-cli-tests`）。`lania-command` 由此成为干净 leaf，命令子包可无环依赖 | `cargo build -p lania-command` 无 dev-dep 反向引用；`cargo test -p lania-cli` 该集成测试仍过                                                 |

**（8）最终每命令二进制的最小链接集（施工后的 `cargo tree` 基线）**

| 命令包                          | 必链                                                           | 按需                                               | 显式不链                         |
| ------------------------------- | -------------------------------------------------------------- | -------------------------------------------------- | -------------------------------- |
| `env`                           | `host-core` + `runex-home` + `lania-cli-kit` + `lania-command` | `runex`（lib）                                     | `bridge-host`、全部 node payload |
| （config 留总包，无独立二进制） | —（`lania-plugins-command-locale` 只随总包链接，见决策 #11）   | —                                                  | —                                |
| `dev/build/lint/tools`          | `host-core` + `runex-home` + `lania-cli-kit` + `lania-command` | `bridge-host`（node adaptor 经 stdio）             | `lania-py/go/dart-bridge`、agent |
| `generate`                      | 同上                                                           | `bridge-host` + `lania-workflows`                  | —                                |
| `create/add/release/git`        | `host-core` + `runex-home` + `lania-cli-kit` + `lania-command` | `bridge-host` + `lania-workflows`                  | agent                            |
| `pi`                            | 同上                                                           | `bridge-host` + `lania-agent` + `lania-agent-core` | —                                |

验收硬门：`env` 等命令二进制的 `cargo tree` 中**搜索不到 `bridge-host` 与任一 `lania-*-bridge`**；`cargo bloat` 对比 `lan-env` 体积应显著小于当前 `lania-cli` 单二进制（因为它根本没编进 5 个语言运行时）。`config` 命令不产出独立二进制（决策 #11），不参与此验收。

### 5.2 B. TS payload 按命令切片

#### B.1 问题链

```text
main.rs:86 load_lan_config_snapshot_from_cwd_async（无条件）
  └─> loading.rs:117 → node_bridge.call_async("config.loadLan")
        └─> 启动 Node bridge（LANIA_NODE_BRIDGE_DIR 由 lan.mjs:149 注入）
              └─> plugin-registry.ts:15 静态 import @lania-cli-v2/agent
                    :59-74 builtinPlugins 12 项静态数组
                      └─> pack.mjs:118-194 必须全量拷 3 个 dist 并装 ejs/tsx/yaml/pi-*
```

#### B.2 最小核 + 动态装配

`packages/foundation/bridge-runtime/ts/` 只保留「电器」：`BridgePlugin` 类型、动态 registry、最小 metrics/runtime、`ctx.tools` 能力面。**stdio 帧循环不自研**——帧编解码、boot、心跳、错误包装一律复用 bridgehub 的 `protocol/gen/ts/runtime.ts`（决策 #12：插座唯一事实源）。

**落地 `agent-architecture.zh-CN.md` A4 待办**（本方案把它作为 B 的前置，不发明新机制）：

```ts
// 由环境变量 LANIA_BRIDGE_SLICES 驱动，值是构建期由 lania.package.json 决定的静态列表
const sliceNames = (process.env.LANIA_BRIDGE_SLICES ?? '').split(',').filter(Boolean);
const builtinPlugins: BridgePlugin[] = [];
for (const name of sliceNames) {
  const mod = await import(`@lania-cli-v2/bridge-slice-${name}`); // 原生 ESM 动态 import
  builtinPlugins.push(mod.plugin);
}
// 空 slices 时 registry 仍可握手，methods 为空 —— 不加载 agent / ejs / templates
```

> 这不是「命令动态发现」：切片列表在构建期确定，运行时只做 ESM `import()`（tsc 输出 ESM，无 bundler 依赖）。

`builtinPlugins` 现有 12 项拆为 12 个独立 npm 包，装到 payload 的 `node_modules/@lania-cli-v2/bridge-slice-*` 下（沿用 `pack.mjs:174-190` 已验证的「手动放置到 node_modules」做法，不依赖 postinstall）：

| slice 包                                                                                   | 原插件                                      | 关键依赖                  | 主要消费命令                                           |
| ------------------------------------------------------------------------------------------ | ------------------------------------------- | ------------------------- | ------------------------------------------------------ |
| `bridge-slice-config`                                                                      | `configPlugin`                              | yaml                      | 所有需 `config.loadLan` 的命令                         |
| `bridge-slice-template`                                                                    | `templatePlugin`                            | **ejs**                   | create / add / generate（含 `create template` 子命令） |
| `bridge-slice-compiler`                                                                    | `compilerPlugin`                            | —                         | dev / build                                            |
| `bridge-slice-lint`                                                                        | `lintPlugin`                                | —                         | build / lint                                           |
| `bridge-slice-agent`                                                                       | `agentBridgePlugin` → `@lania-cli-v2/agent` | **pi-agent-core / pi-ai** | **仅 pi**                                              |
| `bridge-slice-{product,system,lifecycle,toolchain,commitizen,commitlint,dynamic-commands}` | 同名                                        | —                         | 按 §3.3 表                                             |

#### B.3 两个命令完全不带 TS payload

`env` 与 `config`：

1. manifest 声明 `payload.slices: []`、`payload.nodeBridge: false`；命令包内**不存在** `lib/bridge/`；wrapper 与 core dispatcher **不注入** `LANIA_NODE_BRIDGE_DIR`。
2. `lan-env` 二进制**不链接** `lania-bridge-host`（bridge-hub 内聚 5 adaptor，但 env 不需要任何语言运行时，故零桥接依赖；config 留总包，见决策 #11）。
3. `config.loadLan` 的缺省值由 `lania-config` 补齐（纯 Rust、零 bridge，复用现有 crate，不另起 `config-native`）：纯 Rust 读 `lan.config.yaml/json` 的 `locale` / `output` / `validation_errors` 子集，构造与 `LanConfigSnapshot` 同构的值，喂给 `resolve_effective_locale` / `output_profile_from_sources`，使 envelope 与总包一致且不拉起 Node。
   - **已知边界**：`lan.config.ts` 仍需 tsx/Node（现状即如此，见 `pack.mjs:147-149`）。无 JS 命令遇到 `.ts` 配置时回落到 `lania_preferences` + 环境变量，并输出一条与现有 `validation_errors` 渲染同风格的 warning。现状 `.ok()` 本就吞掉加载失败，因此不破坏「行为不变」，登记为 §9.8 未决项。
4. 总包 `payload.slices: ["*"]`，`stageNodeBridgePayload()` 的组装结果与迁移前**逐文件一致**（12 插件全在、ejs/tsx/yaml/pi-\* 全装）。

`LANIA_NODE_BRIDGE_DIR` 注入责任：

| 场景           | 注入方                                                                               | 指向                                               |
| -------------- | ------------------------------------------------------------------------------------ | -------------------------------------------------- |
| 总包           | `distribution/cli/npm/cli/bin/lan.mjs`（迁移自 `npm/cli/bin/lan.mjs:149`，逻辑不变） | cli 包 `lib/node-bridge/`                          |
| 命令包单独安装 | 命令包自己的 `bin/lan-<cmd>.mjs`                                                     | 该包 `lib/bridge/`                                 |
| 调度模式       | `@lania-cli-v2/core` launcher，仅当 `payload.nodeBridge === true`                    | 该命令包自己的 `lib/bridge/`（各命令切片互不污染） |

#### B.4 切片机制具体落地（动态 import + 切片映射）

把 `plugin-registry.ts:59-74` 的静态 `builtinPlugins` 数组改成「**核心常驻 + 切片按需 import**」两分结构：

- **`bridge-runtime` 核心（常驻，永不被切片门控）**：帧层（经 bridgehub `protocol/gen/ts/runtime.ts`，决策 #12）、`BridgePlugin` 类型、`createPluginRegistry()` 动态装配、`metrics`、`plugin-policy`、`loadLanConfig`、`resolveModuleFromCwd`。这些在 `LANIA_BRIDGE_SLICES` 为何值都存在——空切片时 registry 仍能握手、methods 为空、不加载 agent/ejs。
- **切片（按需）**：12 个插件 payload 改为 `import()` 懒加载，由 slice 名索引：

```ts
// packages/foundation/bridge-runtime/ts/src/plugins/index.ts
export const SLICE_LOADERS: Record<string, () => Promise<{ plugin: BridgePlugin }>> = {
  config: () => import('../plugins/config.js'),
  template: () => import('../plugins/template.js'),
  compiler: () => import('../plugins/compiler.js'),
  lint: () => import('../plugins/lint.js'),
  product: () => import('../plugins/product.js'),
  system: () => import('../plugins/system.js'),
  lifecycle: () => import('../plugins/lifecycle.js'),
  toolchain: () => import('../plugins/toolchain-plugin.js'),
  commitizen: () => import('../plugins/commitizen.js'),
  commitlint: () => import('../plugins/commitlint.js'),
  'dynamic-commands': () => import('../plugins/dynamic-commands.js'),
  agent: () => import('@lania-cli-v2/agent'), // 经 agentBridgePlugin 适配（见 :43-57）
};

export async function loadSlices(spec: string): Promise<BridgePlugin[]> {
  const names =
    spec === '*'
      ? Object.keys(SLICE_LOADERS)
      : spec
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean);
  const out: BridgePlugin[] = [];
  for (const name of names) {
    const loader = SLICE_LOADERS[name];
    if (!loader) continue; // 未知切片静默跳过（metrics 可记）
    const mod = await loader();
    out.push(name === 'agent' ? adaptAgentBridge(mod) : mod.plugin);
  }
  return out;
}
```

`createPluginRegistry().load(cwd)` 把原 `builtinPlugins` 替换为 `await loadSlices(process.env.LANIA_BRIDGE_SLICES ?? '')`。

**切片 → 命令映射**（直接来自 §3.3「初始 TS 切片」列，是唯一事实源）：

| 命令       | `payload.slices`                                 | 拉入的切片            |
| ---------- | ------------------------------------------------ | --------------------- |
| `env`      | `[]`                                             | 无（零 node，见 B.3） |
| `config`   | `[]`                                             | 无（零 node，见 B.3） |
| `dev`      | `compiler,config,lifecycle,system,toolchain`     | 5                     |
| `build`    | `compiler,config,lint,product,system,toolchain`  | 6                     |
| `lint`     | `config,lint,toolchain`                          | 3                     |
| `create`   | `config,template,product,commitizen,commitlint`  | 5                     |
| `add`      | `config,template,product`                        | 3                     |
| `generate` | `config,template,dynamic-commands`               | 3                     |
| `release`  | `config,product,commitizen,commitlint,lifecycle` | 5                     |
| `git`      | `config,commitizen,commitlint,lifecycle`         | 4                     |
| `tools`    | `config,system,toolchain`                        | 3                     |
| `pi`       | `config,agent,system`                            | 3（含 agent，重依赖） |

> 打包时，切片模块随各自的 `bridge-slice-*` npm 包落到命令包 `lib/bridge/`（沿用 `pack.mjs:174-190` 的「手动放置到 node_modules」做法，无 postinstall）。tsc 输出 ESM，`import('../plugins/config.js')` 在运行时按相对路径解析；**不依赖任何 bundler**。

#### B.5 现状 → 目标落点映射（ts/ 侧施工清单，2026-11-07 核对）

现状 `ts/packages/` 共 4 包（node-bridge 176 源文件 / templates 294 / agent 8 / lania 4），逐一映射（按 §3.3 提醒，施工时须对照 `node-bridge/src/plugins/**` 逐文件核对 bridge 方法归属）：

| 现状                                                                                                                                                                                                                         | 目标落点                                       | 说明                                                                                                                                                                                                                                           |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `node-bridge/src/core/*`（bridge-plugin / plugin-registry / plugin-policy / metrics / runtime / host-rpc / inline-hooks / schema-tools）+ `src/entry/{builtin-handlers,plugin-dispatch,request-context,protocol-catalog}.ts` | `packages/foundation/bridge-runtime/ts/`       | **核常驻**：帧层经 bridgehub `protocol/gen/ts/runtime.ts`（决策 #12，不自研）、`BridgePlugin` 类型、`createPluginRegistry` 改造为 `loadSlices` 动态装配、metrics、plugin-policy、schema-tools（`ctx.tools` 能力面，仅重导出宿主 leaf，无业务） |
| `node-bridge/src/plugins/config.ts`                                                                                                                                                                                          | `bridge-slices/config`                         | 依赖 yaml                                                                                                                                                                                                                                      |
| `node-bridge/src/plugins/template.ts`（+ `plugins/template/`）                                                                                                                                                               | `bridge-slice-template`                        | 依赖 **ejs**                                                                                                                                                                                                                                   |
| `node-bridge/src/plugins/compiler*.ts` + `src/workers/compiler-worker.ts`                                                                                                                                                    | `bridge-slice-compiler`                        | compiler-shared / worker-client / worker-protocol / worker 随切片走                                                                                                                                                                            |
| `node-bridge/src/plugins/lint.ts`（+ `plugins/lint/`）                                                                                                                                                                       | `bridge-slice-lint`                            |                                                                                                                                                                                                                                                |
| `node-bridge/src/plugins/product.ts`（+ `plugins/product/`）                                                                                                                                                                 | `bridge-slice-product`                         |                                                                                                                                                                                                                                                |
| `node-bridge/src/plugins/system.ts`（+ `plugins/system/`）+ `diag.ts` + `diagnostics.ts`                                                                                                                                     | `bridge-slice-system`                          | diag/diagnostics 归属施工时核对（也可能入核作为诊断面）                                                                                                                                                                                        |
| `node-bridge/src/plugins/lifecycle.ts`                                                                                                                                                                                       | `bridge-slice-lifecycle`                       |                                                                                                                                                                                                                                                |
| `node-bridge/src/plugins/toolchain.ts` + `toolchain-plugin.ts`（`plugins/runtime-adapters/` 归属待核）                                                                                                                       | `bridge-slice-toolchain`                       |                                                                                                                                                                                                                                                |
| `node-bridge/src/plugins/commitizen.ts`（+ commit-tools 共享代码）                                                                                                                                                           | `bridge-slice-commitizen`                      | commit-tools 为 commitizen/commitlint 共享：随 commitizen 切片发布，commitlint 以 npm 依赖引用（切片间允许依赖）                                                                                                                               |
| `node-bridge/src/plugins/commitlint.ts`                                                                                                                                                                                      | `bridge-slice-commitlint`                      | 依赖 `bridge-slice-commitizen`（仅 commit-tools）                                                                                                                                                                                              |
| `node-bridge/src/plugins/dynamic-commands.ts`（+ `plugins/dynamic-commands/`）                                                                                                                                               | `bridge-slice-dynamic-commands`                | 同时被 `packages/runtime/` 复用（§5.6 F.1）；`dynamic-commands-test/` 随切片走                                                                                                                                                                 |
| node-bridge 内的 agent 适配层（`plugin-registry.ts` 的 agentBridgePlugin）                                                                                                                                                   | `bridge-slice-agent`                           | 依赖 `@lania-cli-v2/agent`（pi-agent-core / pi-ai 重依赖只进 pi）                                                                                                                                                                              |
| `ts/packages/agent`                                                                                                                                                                                                          | `packages/products/agent/ts/agent/`            | 主包，不切片（§3.2 主包段）                                                                                                                                                                                                                    |
| `ts/packages/templates`                                                                                                                                                                                                      | `packages/foundation/templates/`（npm 名不变） | 脚手架资产包，被 create/add/generate 复用（§3.3 注 4）                                                                                                                                                                                         |
| `ts/packages/lania`                                                                                                                                                                                                          | `packages/foundation/lania/`（npm 名不变）     | 作者侧 DSL（definePlugin / defineProduct / defineTemplate / defineWorkflow），作者项目直接 import                                                                                                                                              |
| `ts/packages/node-bridge`（npm 包本身）                                                                                                                                                                                      | **消亡**                                       | 被 bridge-runtime + 12 切片取代；总包 `payload.slices:["*"]` 组装结果与迁移前逐文件一致（B.3.4 验收）                                                                                                                                          |
| `node-bridge/src/protocol/*` + `src/entry/stdio.ts`（自带帧层）                                                                                                                                                              | **消亡**                                       | 插座唯一事实源为 bridgehub `protocol/schema.json` + `gen/ts/runtime.ts`，禁止第二套协议实现（决策 #12）                                                                                                                                        |
| `adaptors/ts/lania-node-bridge/src/protocol.rs`（client 自有帧编码）                                                                                                                                                         | **退役**                                       | 宿主侧统一经 `bridge-host` → Hub → gen 协议调用端点（决策 #12，阶段 4a 同步）；client 的 spawn/transport 骨架保留                                                                                                                              |
| `adaptors/ts/endpoint.ts`（echo 演示端点）                                                                                                                                                                                   | 被 bridge-runtime 取代                         | bridge-runtime 是 gen runtime 的第一个真实端点；echo demo 退役前留作协议 conformance golden                                                                                                                                                    |

### 5.3 C. 双组装模式与 launcher

#### C.1 两种模式对照

|            | 总包模式（monolith）                                                          | 按需模式（atomic）                                 |
| ---------- | ----------------------------------------------------------------------------- | -------------------------------------------------- |
| 安装包     | `cli` + 3 `cli-*` + `core`（依赖带入）                                        | `core` + 1~N 个 `cmd-*` + 各自 3 平台包            |
| `lan` 来源 | core launcher 检测到全量二进制 → **直接 spawn `lania-cli[.exe]`**             | core launcher → dispatch → spawn `lan-<cmd>[.exe]` |
| 进程层数   | Node launcher → 全量二进制（**与现有 `lan.mjs:145` 完全等价，仍一层 spawn**） | Node launcher → 单命令二进制（同样一层）           |
| 调度器     | **不经过**                                                                    | 经过（仅命令名路由 + 环境注入）                    |
| 命令树     | 全量 12 命令                                                                  | 已安装的 N 个                                      |
| 行为       | **逐字节不变**                                                                | = 单条命令的行为                                   |

#### C.2 `lan` bin 冲突的解法

- **唯一 `lan` 由 `@lania-cli-v2/core` 提供**（`bin: { "lan": "./bin/lan.mjs" }`）。
- `@lania-cli-v2/cli` **移除 `bin.lan`**，改为 `dependencies: { "@lania-cli-v2/core": "^0.0.x" }`；全量二进制仍叫 `lania-cli[.exe]`、仍放在 3 个 `cli-*` 平台包的 `bin/` 下，`os`/`cpu`/`files` 不变。
- 「装总包」= 自动装 core（得到 `lan`）+ 全量二进制；launcher 检测到全量二进制即直启，**零调度、零额外进程跳转**。
- launcher 检测**不是**动态发现：用 `require.resolve('@lania-cli-v2/cli/package.json')` 探测一个**已知包名**，与 `lan.mjs:51-64` 的 `resolveCliPackageDir()` 同构。

#### C.3 launcher 判定逻辑（伪代码）

```js
// packages/distribution/core/npm/core/bin/lan.mjs —— 唯一 lan 入口
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
const require = createRequire(import.meta.url);
const argv = process.argv.slice(2);

// ── 1) 总包直启（等价于现有 lan.mjs 的全部逻辑）──────────────────────
function resolveMonolithBinary() {
  if (process.env.LANIA_MONOLITH_BINARY) return process.env.LANIA_MONOLITH_BINARY; // 开发模式
  let cliDir;
  try {
    cliDir = path.dirname(require.resolve('@lania-cli-v2/cli/package.json'));
  } catch {
    return null;
  } // 未装总包
  const platformPkg = resolvePlatformBinaryPackage(); // 沿用 lan.mjs:25-38，前缀 cli-
  if (!platformPkg) return null;
  try {
    const pkgDir = path.dirname(require.resolve(`${platformPkg}/package.json`));
    const name = process.platform === 'win32' ? 'lania-cli.exe' : 'lania-cli';
    const bin = path.join(pkgDir, 'bin', name);
    return fs.existsSync(bin) ? bin : null;
  } catch {
    return null;
  }
}

function execDirect(binary, env, args) {
  // 与 lan.mjs:145-170 逐行等价
  const child = spawn(binary, args, { stdio: 'inherit', env: { ...process.env, ...env } });
  child.on('error', (e) => {
    console.error(`Failed to launch ${binary}: ${e.message}`);
    process.exit(1);
  });
  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });
}

const monolith = resolveMonolithBinary();
if (monolith) {
  const cliDir = path.dirname(require.resolve('@lania-cli-v2/cli/package.json'));
  execDirect(
    monolith,
    {
      LANIA_NODE_BRIDGE_DIR:
        process.env.LANIA_NODE_BRIDGE_DIR ?? path.join(cliDir, 'lib', 'node-bridge'),
      LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? process.cwd(),
      LANIA_RUNTIME_MODE: process.env.LANIA_RUNTIME_MODE ?? 'installed',
    },
    argv,
  );
  return; // ← 不进入 dispatch
}

// ── 2) 调度模式 ─────────────────────────────────────────────────────
const KNOWN = JSON.parse(
  fs.readFileSync(
    // 构建期生成，12 条，非运行时扫描
    path.join(__dirname, '..', 'lib', 'known-commands.json'),
    'utf8',
  ),
);

if (argv.length === 0 || ['--help', '-h', 'help'].includes(argv[0])) {
  printAggregatedHelp(KNOWN);
  return;
}
if (['--version', '-V'].includes(argv[0])) {
  printVersion(KNOWN);
  return;
}
if (argv[0] === '--refresh-help') {
  rebuildHelpCache(KNOWN);
  return;
}

const cmd = argv[0];
let manifest = null;
try {
  manifest = JSON.parse(
    fs.readFileSync(require.resolve(`@lania-cli-v2/cmd-${cmd}/lania.package.json`), 'utf8'),
  );
} catch {
  /* 未安装 */
}

if (!manifest) {
  console.error(
    KNOWN.commands.some((c) => c.command === cmd)
      ? `命令 "${cmd}" 未安装。安装：npm i @lania-cli-v2/cmd-${cmd}`
      : `未知命令 "${cmd}"。运行 lan --help 查看可用命令。`,
  );
  process.exit(127);
}
assertSupportedProtocol(manifest.api.protocol); // 运行时校验，见 §9.11

const bin = resolveCommandBinary(manifest); // bin/lan-<cmd>[.exe]，含 os/cpu 校验
const cmdDir = path.dirname(require.resolve(`@lania-cli-v2/cmd-${cmd}/package.json`));
const env = {
  LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? process.cwd(),
  LANIA_RUNTIME_MODE: process.env.LANIA_RUNTIME_MODE ?? 'installed',
  LANIA_COMMAND_NAME: manifest.command,
  ...(manifest.payload.nodeBridge
    ? {
        LANIA_NODE_BRIDGE_DIR: path.join(cmdDir, 'lib', 'bridge'),
        LANIA_BRIDGE_SLICES: manifest.payload.slices.join(','),
      }
    : {}),
};
// argv 规范：剥掉命令名后转发；全局参数原样透传，不由 launcher 消费
execDirect(bin, env, argv.slice(1)); // ["list", "--json", ...]
```

> `execDirect` 在两种模式下共用同一段代码（spawn / stdio / error / exit / signal），这是 TTY 与信号行为一致的实现保证。

#### C.4 全局参数归属与 argv 契约

- `--json` / `--locale` / `--output-profile` / `--no-progress` **不属于 core，属于每个命令二进制**：由 `lania-cli-kit::run_single_command` 内的 `profile.rs`（`output_profile_from_sources` / `resolve_effective_locale` / `OutputMode` / `ProgressMode`）解析，与总包 `main.rs:90-164` 同一份代码。
- launcher **不改写 argv**（只剥掉命令名）；总包模式下也是原样透传。
- 例外：顶层 `--help` / `--version` / `help` 由 launcher 消费（总包模式下仍由全量二进制的 `render_builtin_command` 处理）。`lan --help` 在两种模式下输出不同是**预期的**：总包显示全部 12 条，调度模式显示已安装 N 条 + 未安装提示。
- `run_single_command(spec, factory, args)` 内部按 `spec().name` **补回命令名**再交给 `lania_command::build_cli`（root `binary_name = "lan"`，唯一 subcommand = spec），使 `CommandContext` 与 `execution_json_value` 里的 `command` 名、envelope 字段与总包一致。

#### C.5 `lan --help` 聚合

- 每个 `lan-<cmd>` 支持 `--describe`，输出 `serde_json::to_string(&spec())`（`CommandSpec` 已 `Serialize`）。
- **构建期** `scripts/gen-describes.mjs` 生成 `packages/commands/<cmd>/describes/<cmd>.json`（静态资产随包发布），避免 `--help` 时 spawn 12 次。
- 同时生成 `packages/distribution/core/npm/core/lib/known-commands.json`：

```json
{
  "protocol": 1,
  "coreVersion": "0.0.3",
  "commands": [
    {
      "command": "env",
      "package": "@lania-cli-v2/cmd-env",
      "bin": "lan-env",
      "describe": "describes/env.json"
    }
  ]
}
```

共 11 条构建期静态表（config 留总包、不在调度表内，见决策 #11）；运行时 launcher 对每条已知名只做 `require.resolve`，**不扫描目录、不发现未知命令**。调度模式下 `lan config` 不在 `known-commands.json`，launcher 提示「config 属于总包，请安装 @lania-cli-v2/cli」。

- 缓存 `lib/help-cache.json`（或用户缓存目录），键 = 各已安装 manifest 的 `version` + `coreVersion` + `protocol`；版本变化即失效，`--refresh-help` 显式重建。
- 渲染：调度模式由 core 的 JS 用与 `render_builtin_command` 同构的模板渲染「已安装命令 + 未安装命令的可安装提示」；总包模式直接交给全量二进制。`lan <cmd> --help` 在调度模式下转发给 `lan-<cmd> --help`。

#### C.6 TTY / stdio / 信号

- `spawn(..., { stdio: 'inherit' })`（沿用 `lan.mjs:145`）：子进程直接继承父进程 stdin/stdout/stderr，launcher **不插在 stdio 中间**。`lan pi` 的 REPL、`lan env ui` 的 ratatui TUI、`lan dev` 的 watch 都拥有真实 TTY。
- 信号：沿用 `lan.mjs:163-170` —— `if (signal) process.kill(process.pid, signal); else process.exit(code ?? 0);`；launcher 在子进程退出前保持存活，避免孤儿。Windows 不设 `CREATE_NEW_PROCESS_GROUP`，靠控制台继承（需 e2e 覆盖）。
- 退出码原样透传；`ensureExecutable()`（Unix 0o755）沿用 `lan.mjs:99-109`。
- **结论：调度模式相对现状只多了一次 Node launcher，而现状 `lan.mjs` 本身也是一次 Node → 二进制 spawn；不新增进程层、不新增 IPC、不新增 TTY 判定差异。**

#### C.7 `known-commands.json` 完整 schema 与派发协议

**构建期生成**：`scripts/gen-known-commands.mjs` 读取 12 份 `lania.package.json`（`kind === "command"`），产出 `packages/distribution/core/npm/core/lib/known-commands.json`，随 `@lania-cli-v2/core` 发布。运行时 launcher 只读此表，**不扫描目录、不发现未知命令**。

完整 schema（每个命令一条，共 12 条）：

```json
{
  "protocol": 1,
  "coreVersion": "0.0.3",
  "commands": [
    {
      "command": "env",
      "package": "@lania-cli-v2/cmd-env",
      "bin": "lan-env",
      "npmBin": "lan-env.mjs",
      "order": 11,
      "describe": "describes/env.json",
      "needsNodeBridge": false,
      "platformPackages": [
        "@lania-cli-v2/cmd-env-darwin-arm64",
        "@lania-cli-v2/cmd-env-linux-x64",
        "@lania-cli-v2/cmd-env-win32-x64"
      ]
    }
  ]
}
```

**派发协议（launcher 处理一条 `lan <cmd> [..args]` 的逐步动作）**：

| 步骤         | 动作                                                                                                                                                                               | 说明                                                                         |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| 1 取命令名   | `cmd = argv[0]`；`args = argv.slice(1)`                                                                                                                                            | 顶层 `--help` / `--version` / `help` / 空 由 launcher 自身消费（C.4/C.5）    |
| 2 查静态表   | `entry = KNOWN.commands.find(c => c.command === cmd)`                                                                                                                              | 无目录扫描；命中 → 跳到 3；未命中 → 进入 **Phase 2 动态回退**（见 §5.6 F.2） |
| 3 已安装判定 | `require.resolve(entry.package + '/package.json')` 成功？                                                                                                                          | 已知但包未装 → 「命令未安装，npm i <pkg>」提示，`exit 127`                   |
| 4 解析二进制 | 用 `entry.platformPackages` 中匹配当前 os/cpu 的那一个 → 其 `bin/` 下 `lan-<cmd>[.exe]`（核心直启 Rust 二进制，**不经过** `npmBin` 的 .mjs wrapper）                               | os/cpu 已被 optionalDependencies 保证匹配                                    |
| 5 注入环境   | 必设 `LANIA_COMMAND_NAME=cmd` + `LANIA_PRODUCT_ROOT` + `LANIA_RUNTIME_MODE`；`needsNodeBridge` 时追加 `LANIA_NODE_BRIDGE_DIR`（指向该命令包 `lib/bridge/`）+ `LANIA_BRIDGE_SLICES` | 各命令切片互不污染（C.3）                                                    |
| 6 转发 argv  | `spawn(bin, args, { stdio: 'inherit' })`                                                                                                                                           | 命令名已剥（C.4）                                                            |
| 7 退出码     | `exit(code ?? 0)`；若子进程被信号杀死 → `process.kill(process.pid, signal)`                                                                                                        | 与 C.6 一致，TUI/REPL 的 Ctrl-C 透传到子进程                                 |
| 8 TTY        | `stdio: 'inherit'` 继承真实 TTY                                                                                                                                                    | `lan pi` REPL / `lan env ui` ratatui TUI 正常                                |

> 第 4 步**直启 Rust 二进制**（`lan-<cmd>.exe`），不走 `npmBin` 的 `lan-<cmd>.mjs` wrapper。`npmBin` 那条路径仅供 `npx lan-env ...` 或用户把 `lan-env` 直接放进 PATH 时调用（D.3 所述第二条路径）。两条路径最终抵达同一份 `run_single_command`，故行为一致。

### 5.4 D. 命令子包 API 契约

#### D.1 稳定 API（对总包暴露）

下沉到 `lania-command`（干净 SDK，仅 clap/serde/serde_json）：把 `CommandHandler` / `CommandExecution` / `ExecutionError` / `EXIT_RUNTIME_ERROR` 从 `lania_host::execution` **类型搬迁**到 `lania-command`，`host-core` 保留 `pub use` 兼容；新增：

```rust
// packages/foundation/command/rust/src/plugin.rs  (crate lania-command)
use crate::{CommandContext, CommandSpec};

/// 宿主能力视图：由 lania-host-core 实现，命令 crate 只依赖此 trait
pub trait HostApi: Send + Sync {
    fn locale(&self) -> &str;
    fn capability(&self, name: &str) -> Option<&dyn std::any::Any>;
}

pub trait CommandPlugin: Send + Sync {
    fn spec(&self) -> CommandSpec;
    /// 按 CommandContext.handler_id 分派到该命令的若干 handler
    fn execute(&self, ctx: &CommandContext, host: &dyn HostApi)
        -> Result<CommandExecution, ExecutionError>;
}
pub type CommandPluginFactory = fn() -> Box<dyn CommandPlugin>;

pub fn describe(spec: &CommandSpec) -> String { serde_json::to_string(spec).unwrap() }
```

**最小改动路径**：现有命令 crate 的 `impl Plugin for XxxCommandPlugin { setup(...) { register_builtin_command(...) } }` 不必重写 handler 逻辑 —— 把 `register_builtin_command` 的签名从 `lania_host::plugin::*` 平移到 `lania_command::*` 同名函数，`setup` 改为 `impl CommandPlugin`（内部注册到 `lania_command::CommandRegistrar`），`host-core` 实现该 registrar 并落到现有 `CommandRegistryImpl` / `HandlerRegistryImpl`。**装配方式变化，handler 代码零变化**，这是 golden 回归能过的关键。

每个命令 crate 的冻结导出面（以 env 为例）：

```rust
// packages/commands/env/rust/src/lib.rs
pub const COMMAND_NAME: &str = "env";            // 必须等于 spec().name
pub const HANDLER_ID:   &str = "command.env";    // 现有常量，保持
pub fn spec() -> lania_command::CommandSpec { /* 原样保留 */ }
pub fn factory() -> Box<dyn lania_command::CommandPlugin> {
    Box::new(EnvCommandPlugin)                    // 现有结构体
}
pub struct EnvCommandPlugin;
// 其余 pub const LIST_HANDLER_ID 等保持，一并写入冻结清单
```

总包装配（**编译期生成，非运行时发现**）：

```rust
// 由 scripts/gen-builtin-commands.mjs 从 12 份 lania.package.json 生成
static BUILTIN: &[(&str, lania_command::CommandPluginFactory)] = &[
    ("dev",     lania_plugins_command_dev::factory),
    // ... 12 行，替代 main.rs:62-75 的 12 次 register_plugin（template 已并入 create）
    ("config",  lania_plugins_command_locale::factory),  // crate 名 locale，命令名 config
];
let mut host = HostRuntime::new();                       // host-core + bridge-host（全 adaptor）
for (name, factory) in BUILTIN {
    host.register_command(*name, factory());            // 内部等价于旧 register_plugin
}
```

> ⚠️ `BUILTIN` 顺序必须与 `main.rs:62-75` **完全一致**（`main.rs:59-61` 注释：顺序影响命令树展示与 hook 时机）。序号写进 manifest 并纳入 golden 对比。

#### D.2 冻结导出面 + 编译期守卫

每个命令包新增 `tests/frozen_export_surface.rs`（同名同思路照抄 `runex` 的 `tests::frozen_export_surface_is_intact`）：

```rust
const EXPECTED: &[&str] = &[
    "COMMAND_NAME", "HANDLER_ID", "spec", "factory", "EnvCommandPlugin",
    "LIST_HANDLER_ID", /* ... */
];
#[test]
fn frozen_export_surface_is_intact() {
    // 构建期 scripts/gen-api-surface.mjs 扫描 pub 项生成 exports.snapshot.json
    let actual: Vec<&str> = serde_json::from_str(include_str!("../exports.snapshot.json")).unwrap();
    assert_eq!(actual, EXPECTED, "导出面变更必须人工 review 并递增 api.protocol");
}
```

不假设仓库已有 `cargo public-api` 等工具；`gen-api-surface.mjs` 为新增的轻量扫描脚本（`cargo rustdoc --output-format json` 或源码 `pub` 项）。manifest 的 `rust.api.surface` 是 `EXPECTED` 的单一事实源，`api.protocol` 初始为 `1`，core 内置 `SUPPORTED_PROTOCOL = 1`。

#### D.3 CLI 薄壳与 argv 规范

```rust
// packages/commands/env/rust/src/bin/lan-env.rs  ([[bin]] name = "lan-env")
#[tokio::main]
async fn main() {
    runex_home::activate_managed_runtimes();                  // A.2，幂等、静默失败
    let args = lania_cli_kit::normalize_os_args(std::env::args_os());
    let code = lania_cli_kit::run_single_command(
        lania_plugins_command_env::spec(),
        lania_plugins_command_env::factory(),
        args,                                                // 已剥掉命令名
    ).await;
    std::process::exit(code);
}
```

`run_single_command` 内部：补回命令名 → `apply_legacy_aliases` → `localize_command_specs` → `render_builtin_command` → `build_cli(binary_name = "lan", ...)` → `command_context_from_matches` → `host_core::execute_command(ctx)` → `execution_json_value` / `should_suppress_output` / `render_output_value` → exit code。**与 `main.rs:43-285` 同一套 kit 函数**。

- **argv 规范**：外部契约 = 调度器剥掉命令名后转发，即 `lan env list --json` → `spawn lan-env ["list","--json"]`；补回动作在 `run_single_command` 内部完成。
- **独立 CLI 的第二条路径**：命令包 `npm/cmd-<cmd>/bin/lan-<cmd>.mjs` 是 thin wrapper（与 `lan.mjs` 同构：`require.resolve` 定位平台包 + 注入切片环境变量 + `stdio:'inherit'` + 信号转发），供 `npx lan-env ...` 或 PATH 直接调用。总包与调度模式**不经过**这个 wrapper。

命名规约（单一事实源 = manifest `command`）：

| 维度           | 规约                                      | env                  | pi                  |
| -------------- | ----------------------------------------- | -------------------- | ------------------- |
| 目录           | `packages/commands/<command>/`            | `commands/env`       | `commands/pi`       |
| Cargo crate    | 沿用 `lania-plugins-command-<crate>`      | `...-env`            | `...-pi`            |
| Cargo bin      | `lan-<command>`                           | `lan-env`            | `lan-pi`            |
| npm 主包       | `@lania-cli-v2/cmd-<command>`             | `cmd-env`            | `cmd-pi`            |
| npm 平台包     | `@lania-cli-v2/cmd-<command>-<os>-<arch>` | `cmd-env-win32-x64`  | `cmd-pi-win32-x64`  |
| 平台包内二进制 | `bin/lan-<command>[.exe]`                 | `bin/lan-env.exe`    | `bin/lan-pi.exe`    |
| describe       | `describes/<command>.json`                | `describes/env.json` | `describes/pi.json` |

（原表第二示例列为 `commands/config`，随决策 #11 改为 `commands/pi`。）

#### D.4 同一份 `CommandPlugin` 服务双形态（monolith static vs atomic binary）

核心结论：**`spec()` + `factory()` 是唯一契约，两种组装方式都用它，命令作者只写一遍。** 区别只在「链接了哪些命令 crate」与「注册了几个命令」，执行路径最终都汇入 `lania-cli-kit::run_single_command`。

```rust
// 总包（monolith）—— scripts/gen-builtin-commands.mjs 从 12 份 manifest 生成，顺序 = main.rs:62-75
#[tokio::main] async fn main() -> Result<()> {
    activate_managed_runtimes();
    let mut host = HostRuntime::new();                       // host-core + bridge-host（全 adaptor）
    for (name, factory) in BUILTIN { host.register_command(name, factory()); }
    host.initialize().await?;
    // ... envelope / locale / output_profile（与 main.rs:77-164 逐行等价）...
    let code = lania_cli_kit::run_single_command(spec_of(args), factory_of(args), args).await;
    std::process::exit(code);
}

// 命令包单独安装 —— commands/env/rust/src/bin/lan-env.rs（手写，固定两行注册）
#[tokio::main] async fn main() -> Result<()> {
    activate_managed_runtimes();
    let mut host = HostRuntime::new();                       // 仅 host-core + runex-home，无其它命令 crate
    host.register_command("env", lania_plugins_command_env::factory());   // ← 只注册自己
    host.initialize().await?;
    let code = lania_cli_kit::run_single_command(
        lania_plugins_command_env::spec(),
        lania_plugins_command_env::factory(),
        lania_cli_kit::normalize_os_args(std::env::args_os()),
    ).await;
    std::process::exit(code);
}
```

两处 `run_single_command` 是**同一函数**（A.4）：补回命令名 → `apply_legacy_aliases` → `localize_command_specs` → `render_builtin_command` → `build_cli(binary_name="lan", ...)` → `command_context_from_matches` → `host_core::execute_command` → `execution_json_value` / `render_output_value` → exit code。

要点：

- **monolith 与 atomic 都是 Rust 静态链接**，共用同一 `CommandPlugin` trait，彼此之间**没有 FFI / dylib / IPC**——差异仅在于链接进二进制的命令 crate 集合不同（monolith = 12 个，atomic = 1 个）。这就是「行为不变」能成立的根本原因。
- `order` 字段（manifest）驱动 `BUILTIN` 数组的注册顺序，必须与 `main.rs:62-75` 完全一致（D.1 已记）；`gen-builtin-commands.mjs` 按 `order` 排序后生成，纳入 golden 对比。
- 冻结导出面（D.2）对两种形态同等适用：`factory()` 返回 `Box<dyn CommandPlugin>`，monolith 与 atomic 二进制都依赖同一 trait 签名，core 内 `SUPPORTED_PROTOCOL` 校验的是这个签名族，不是某个具体 crate。

### 5.5 E. 依赖方向与 Bridge Hub 部署形态

依赖方向与硬规则见 §4。

> **命名澄清（先读这段，避免把 `lania-bridge-host` 误当成"另一个独立 bridge"）**
>
> - **`bridge-hub` 是产品 / 包名**（`packages/products/bridge-hub/`，npm `@lania-cli-v2/bridge-hub`）。它是**唯一内聚全部桥接能力的包**，包含 4 个 Rust crate（`lania-bridge-protocol` / `bus` / `core` / `host`）+ 5 个语言端点 adaptor（`adaptors/{ts,py,go,dart,rust}/`，即原 `lania-{node,py,go,dart,rs}-bridge` 整体并入）。
> - **`lania-bridge-host` 不是独立于 bridge-hub 的 bridge**，它是 `bridge-hub/rust/` 下的**宿主侧集成 crate**（bridge-hub 自身的一部分）。职责：装配 Hub、持有 `SharedBridgeRegistry`、注册 5 个 adaptor、实现 `host-core` 定义的 `BridgeProvider` trait，对外暴露 `load_bridge_provider()`。
> - 所以命令二进制写 `lania-bridge-host` 依赖，准确含义是 **"依赖 bridge-hub 的宿主装配入口"**，而不是"依赖某个单独的语言 bridge"。桥接两端（宿主端与各语言端）都在 `bridge-hub` 包内；宿主只拿到 host 侧 crate，**永远不 import 任何具体语言 bridge crate**——这正是「5 个 bridge 内聚进 bridge-hub」的落地形态，与本决策不矛盾。
>
> | bridge-hub 子 crate             | 角色                                                                                                              |
> | ------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
> | `lania-bridge-protocol`         | 帧协议 + 生成绑定（schema 唯一事实源）                                                                            |
> | `lania-bridge-bus`              | Hub 本体 + `Channel` 抽象（端点寻址 / 广播）                                                                      |
> | `lania-bridge-core`             | 共享核心：帧循环骨架、pending、错误码                                                                             |
> | `lania-bridge-host`             | **宿主侧集成**：装配 Hub、`SharedBridgeRegistry`、注册 5 adaptor、实现 `BridgeProvider`、`load_bridge_provider()` |
> | `adaptors/{ts,py,go,dart,rust}` | 5 个语言端点实现（数据面 / 端点侧，不是 crate 名）                                                                |
>
> **"部署形态"只回答一个进程拓扑问题**：Hub 是**独立常驻进程**（core spawn 一个 daemon，命令经 socket/端口连它），还是**嵌在命令进程内**（命令二进制静态链接 `lania-bridge-host`，Hub 与命令同生共死）。本方案选后者，理由见下。

**决策：Hub 进程内嵌，不采用 core 持有 sidecar。** 这是本文对 `bridge-layer.zh-CN.md` §6 未决项「独立 Hub 进程还是嵌入 CLI 进程需结合其它模块的重新设计结果决定」的收敛结论。理由：

1. `bridge-layer.zh-CN.md` §5 已写「Hub 是唯一汇合点，**可嵌入 CLI 进程（推荐）**，无需独立进程、可用进程内 channel」，且 §6 明确**不做 dylib 热插拔**。
2. core launcher 的生命周期是「spawn 完即跟随子进程退出」，**不常驻**，无法持有长生命周期 Hub。强行 sidecar 需引入 daemon 常驻、socket/端口发现、端点鉴权、孤儿回收、启动竞态 —— 与「极小的 core」「无 postinstall」「安装期不联网」「不产生额外进程跳转」四条硬约束直接冲突。
3. 内嵌只需把 `lania-bridge-host` lib 静态链接进 `lan-<cmd>`：Rust 端点走**进程内 channel**（bridge-layer §4），TS/Py/Go/Dart 端点走 **stdio 子进程**（沿用原 `lania-node-bridge` 的懒加载模型，端点代码现位于 `bridge-hub/adaptors/<lang>/`）。不新增常驻进程、不新增 IPC 发现机制。
4. Hub 的状态（capability/topic 表、端点生命周期）在**单次命令调用**内有效；命令之间需要共享的是 config / preferences 文件，不是 Hub 内存状态。

| 场景      | Hub 形态                           | 进程数变化                                 |
| --------- | ---------------------------------- | ------------------------------------------ |
| 总包模式  | `lania-cli` 单进程内嵌 1 个 Hub    | 对外无变化                                 |
| 按需模式  | 每个 `lan-<cmd>` 各自内嵌 1 个 Hub | 每命令独立，互不共享                       |
| `pi` 命令 | `lan-pi` 内嵌 Hub + agent TS 端点  | 与 `lania-plugins-command-pi` 薄适配层兼容 |

**兜底（不默认）**：`products/bridge-hub/npm/` 仍提供可选独立 `lan-hub` bin（主包双形态要求），用于调试与未来「长驻 watch 跨命令共享 Hub」，以 feature 开关启用。文档旧布局 `bridge/protocol/schema.json` + `bridge/adaptors/<lang>/` 迁入 `packages/products/bridge-hub/protocol/` 与 `.../adaptors/`，4 个 crate 迁入 `.../rust/`。

#### E.1 `lania-bridge-host` 进程内装配接线（命令二进制侧）

把 bridge-host 嵌入 `lan-<cmd>` 的具体接线：需要语言运行时的命令二进制，在 `HostRuntime::new()` 之后调用 `host.attach_bridge(...)`，把 A.5 (3) 的 `BridgeProvider` 实现注入 `host-core`；`env`/`config` 等纯 Rust 命令**不调用**这一步，`host.bridge` 保持 `None`。

```rust
// packages/commands/build/rust/src/bin/lan-build.rs（需 Node 的命令）
#[tokio::main] async fn main() -> Result<()> {
    activate_managed_runtimes();                          // A.2
    let mut host = HostRuntime::new();                   // host-core，零 bridge
    // 注入桥接层：host-core 接收 Option<Arc<dyn BridgeProvider>>
    let provider = lania_bridge_host::load_bridge_provider().await;   // 懒启动 node adaptor 子进程
    host.attach_bridge(provider).await;                  // ← env/config 完全不调用此行
    host.register_command("build", lania_plugins_command_build::factory());
    host.initialize().await?;
    let code = lania_cli_kit::run_single_command(
        lania_plugins_command_build::spec(),
        lania_plugins_command_build::factory(), args).await;
    std::process::exit(code);
}
```

- `lania_bridge_host::load_bridge_provider()` 返回 A.5 (3) 的具体 `BridgeProvider` 实现：内部按 `RuntimeKind`（已内聚进 bridge-host，§3.4）为 JS 簇启动**一个** Node stdio 子进程、为 Py/Go/Dart 注册懒启动端点、为 Rust 注册**进程内 channel**（bridge-layer §4），并暴露 `register_language(id, spec)` 供 host-core 装配。
- `env` / `config` 二进制**不链接 `lania-bridge-host`**，也就没有 `load_bridge_provider` 符号；`host.bridge` 为 `None`，`config.loadLan` 回落到 `lania-config`（A.5 (5)），**不拉起任何 Node 子进程**。`cargo tree -p lan-env` 可证明零桥接依赖。
- `pi` 还需 agent TS 端点：`bridge_host.attach_agent_endpoint()` 把 `@lania-cli-v2/agent`（作为 `agent` 切片，B.4）接入 Hub；`lania-plugins-command-pi` 经 host 侧 agent client 调 `lania_agent::run()`（§3.4）。这与现有 `lania-plugins-command-pi` 作为「唯一认识宿主的适配层」的定位一致。

> 验证门：在 `lan-build` 二进制里 `host.attach_bridge` 之后，`config.loadLan` / `register_language` 走 `BridgeProvider::call`；在 `lan-env` 里同一调用点走 `lania-config` 纯 Rust 路径——两条路径由 `host.bridge` 是否为 `None` 在运行时分支决定，无需编译期 feature。

### 5.6 F. 动态运行时命令、脚手架封装新命令、宿主底层能力的拆分处理

本节处理「按命令原子化」与现有三类**非 builtin 扩展机制**的冲突。先确认现状事实（已读 `rust/crates/lania-host/src/runtime/dynamic/types.rs`、`ts/packages/node-bridge/src/plugins/dynamic-commands/*`、`ts/packages/node-bridge/src/core/schema-tools.ts:104-126`）：

- **动态运行时命令是 100% 的 node-bridge（JS 运行时）特性**：项目级 `lan.config.*` 声明 `runtimeCommands: [{ mount, commands: [{ name, handler: { plugin: './lania.plugin.js', method } | async fn }] }]`；解析/合并/执行全在 `ts/packages/node-bridge/src/plugins/dynamic-commands/`（parse.ts / execute\*.ts / scaffold.ts / workflow-steps.ts 等 30+ 文件）。Rust 侧 `runtime/dynamic/types.rs` 只有 `BridgeCommandHandler`（把命令转发到某个 bridge method）这一个**薄壳**——**没有 node-bridge 就跑不了动态命令**。
- **脚手架是注入给 handler 的能力**：`ctx.tools.scaffold: ScaffoldTools`（`schema-tools.ts:120`）由 `dynamic-commands/scaffold.ts` 的 `resolveScaffoldPlan` 提供，消费命令声明里的 `spec.scaffold`（`ManifestCommandScaffoldSpec`）。"利用脚手架封装新的命令"因此有两层含义（见下）。
- **宿主底层能力是 `host-core` 的 leaf 重导出**：`exec/fs/git/logger/pm/progress/prompt/runtime/task/workflows/preferences/config/hooks`（`lania-host/Cargo.toml:18-35`、`lib.rs:2-38`）。动态命令与脚手架通过 node-bridge 的 `ctx.tools`（`SchemaTools`，`schema-tools.ts:104-126`）访问它们——即**只要 node-bridge 在跑，能力就在**，与装了哪些 builtin 命令包无关。

**矛盾点**：本方案把 `env`/`config` 做成**不链接 node-bridge 的薄二进制**（§5.5 E.1），而动态命令/脚手架**必须** node-bridge。若 dispatcher 只在 `known-commands.json` 里找 builtin 包，项目级动态命令和脚手架封装出的新命令就会"找不到宿主"。

#### F.1 解：把"动态命令 + 脚手架运行时"收敛成一个**共享设施包** `packages/runtime/`

它不是一条 named 命令，而是**可扩展宿主设施**——对应现状「全量 `lania-cli` 里负责项目级扩展性的那一半」。构成：

```text
packages/runtime/  @lania-cli-v2/runtime   kind: "command-runtime"
  rust/lania-command-runtime/   # 链接 全量 host-core + bridge-host + node-bridge
    bin/lan-runtime.rs          # [[bin]] lan-runtime：project 级 runtimeCommands 的执行宿主
  ts/                           # 复用 node-bridge 的 dynamic-commands + scaffold + templates 切片
  npm/runtime/                  # bin/lan-runtime.mjs + lib/bridge/(动态+脚手架切片)
```

- 它携带 `host-core`（全量 leaf）+ `bridge-host` + node-bridge + 切片 `dynamic-commands` / `scaffold` / `templates`，即**完整的宿主底层能力面**。
- **不**作为 `@lania-cli-v2/core` 的硬依赖；改为**按需懒加载 + 缺失即提示**（`core` 的 optionalDependency 或 peerDependency），与 §5.3 C「未安装命令给可安装包名提示」的约定一致。这样「只装 env」的纯净安装 = `core` + `cmd-env`，**不拖 node-bridge**；只有当你真正要跑项目里的动态命令 / 脚手架封装命令时，才需要 `npm i @lania-cli-v2/runtime`。

#### F.2 解：dispatcher 增加「Phase 2 动态回退」

在 §5.3 C.7 的 Phase 1（builtin → `lan-<cmd>`）之后增加回退：

| 步骤        | 动作                                                                                                                                                              | 说明                                                                                     |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| 2' 动态探查 | Phase 1 未命中 → 若已装 `runtime` 设施，spawn `lan-runtime --resolve <cmd> --cwd <cwd>`，由 node-bridge 读 `lan.config.*` 的 `runtimeCommands` 判定该命令是否挂载 | 仅项目目录内触发；纯净 `lan env` 在无非动态命令项目里永不 spawn 它                       |
| 3' 命中动态 | `lan-runtime --resolve` 返回 `mount + handler` → 以 `lan-runtime <mount> <sub> [args]` 执行（同进程内 node-bridge 已加载，直接跑 handler）                        | 输出/退出码与现状 `lania-cli` 跑动态命令**逐字节一致**（同一份 node-bridge + host-core） |
| 3'' 未命中  | `lan-runtime` 未装 → 提示 `npm i @lania-cli-v2/runtime`；已装但仍无该命令 → 提示未知命令                                                                          | 与 builtin 未装提示同一套文案                                                            |

`lan --help` / completion 在校园目录内也走此回退：向 `lan-runtime` 询问项目 `runtimeCommands` 列表，与静态 `known-commands.json` 合并展示（现状 `lan --help` 本就会因 `config.loadLan` 拉起 node-bridge，行为一致）。

#### F.3 解："脚手架封装新的命令"的两条路径都已覆盖

| 含义                                                                                                                    | 落地方式                                                                                                                                                            | 是否需新包                               |
| ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| **(a) 项目级动态命令**：脚手架生成 `lan.config.*` 的 `runtimeCommands` 条目（含 `./lania.plugin.js`）                   | 被 F.1/F.2 的 `runtime` 设施直接消费，**无需打包**                                                                                                                  | 否（仍走 node-bridge 动态机制）          |
| **(b) 生成一个新的 builtin 风格命令包**：`create`（含 `template` 子命令）/`generate` 用脚手架产出**可独立安装的命令包** | 生成器须按 §6.1 的 `lania.package.json` schema 产出 `packages/commands/<name>/` 布局；产物作为普通命令包参与构建/安装，dispatcher 经 `known-commands.json` 自动识别 | 是（但生成的是标准命令包，复用整套机制） |

要点：路径 (a) 是**运行时**扩展，天然与"命令原子化"正交——它不依赖你装了哪些 builtin 包，只依赖 `runtime` 设施；路径 (b) 是**构建期** codegen，产物是一个标准命令包，与"原子化"完全同构（生成器本身需注意改读新 `packages/commands/*/lania.package.json` 布局，属工具链适配，非运行时问题）。

#### F.4 解：宿主底层能力对动态命令 / 脚手架**始终完整**

因为动态命令与脚手架**只在 `runtime` 设施进程内执行**，而该进程链接**全量 `host-core`**，`ctx.tools`（`exec/fs/git/pm/runtime/scaffold/...`）与现状 `lania-cli` 完全一致。薄 per-command 二进制（`lan-env` 等）不编译这些能力进来，**因为它们根本不执行动态命令**——能力可用性绑定在"执行该命令的设施"上，而非绑定在"你装了哪些 builtin 包"上。这样彻底解耦：

- 只装 `env` → `lan env` 极小、无 node-bridge；
- 同一项目里又跑 `lan ops ping`（动态命令）→ 自动走 `runtime` 设施 → 完整 `ctx.tools`；
- 二者互不污染依赖。

#### F.5 验收（追加到 §7.1 验收清单）

- `lan ops ping`（项目 `lan.config.*` 声明 `runtimeCommands`）在**全量 cli** 与 **`core + cmd-env + runtime`** 两种安装下输出逐字节一致（golden 新增动态命令用例，现状 `tests.rs:167/251/301` 已有三类声明形态可复用）。
- `cargo tree -p lan-env` **不得**出现 `lania-command-runtime` / `lania-node-bridge` / `dynamic-commands`；`lan-runtime` 的 tree 必须出现它们。
- 纯净 `core + cmd-env`（无 `runtime`）下，`lan env` 正常；`lan ops ping` 给出 `npm i @lania-cli-v2/runtime` 提示而非崩溃。
- `lan --help` 在项目目录内能列出 `runtimeCommands` 挂载项，与现状一致。

#### F.6 补：脚手架 / 动态命令如何"读配置 + 跑脚本"

脚手架与项目级动态命令同样落在 `runtime` 设施进程内，因此它们与 §5.7 G 的 `ctx.config` / `ctx.script` 契约**完全同构**——只是 `ctx` 由 `runtime` 设施注入而非由命令二进制注入。即：

- `create`（含 `template` 子命令）/`generate` 在生成新命令包时，把"读 `lania.config.*` + 跑包内 `scripts/`"的样板直接产出为对 `ctx.config` / `ctx.script` 的调用（见 §5.7 G.5 作者写法），生成物无需自带文件读取 / 进程 spawn 代码。
- 项目级动态命令的 handler 通过 node-bridge 的 `ctx.tools`（已含 `exec/fs/git/pm/runtime/scaffold`，`schema-tools.ts:104-126`）调用同一套能力，与 builtin 命令二进制走 `ctx.script` 是同一语义的两个入口。

---

### 5.7 G. 命令通用横切能力：配置读取与脚本执行契约

#### G.0 问题：横切能力尚未合成契约（本方案补强点）

"每个命令都可能要读配置文件或执行脚本"是**所有 12 个命令共享的横切需求**，但前文把它拆散在了不同位置：`lania-config`/`lania-exec`/`lania-fs` 是 host-core 的 leaf（A.1 依赖矩阵），`config.loadLan` 路由在 A.5(5)，`ctx.tools` 在 §5.2 / §5.6。缺少的是一条**命令包作者统一入口**：

- ❌ 若每个命令包自行 `std::fs::read` + `serde_yaml` 或自行 `std::process::Command::new("sh")`：① 路径解析、`.ts` 配置求值、TTY 转发、超时、退出码映射会**各处漂移**；② 命令二进制各自把 `exec`/`fs` 重新拉进来，**原子化初衷被破坏**；③ API 态（被总包静态链接）与 CLI 态（独立 `lan-<cmd>` 二进制）**行为可能不一致**。
- ✅ 本方案要求：命令包**永远不自己读文件 / 不自己 spawn shell**，统一经 `ctx.config` / `ctx.script` 两个访问器。能力由注入的 `host` 提供，命令代码只写一次。

#### G.1 配置读取：双层模型 + 单一 `ctx.config` 访问器

配置分两个 scope，命令一律经 `ctx.config` 读取，不感知底层是纯 Rust 还是走 Node：

**① 项目 / 用户配置（project scope）** —— 操作对象项目的 `lan.config.*`。复用 A.5(5) 的 `ConfigSource` 抽象与 `config.loadLan` 路由：

```rust
// host-core 已在 A.5(5) 抽象：ConfigSource::load(cwd) -> Option<LanConfigSnapshot>
// 纯 .yaml/.json → lania-config（零 bridge）；.ts → BridgeProvider::call("config.loadLan")
trait HostApi {
    fn config(&self) -> &dyn ConfigView;
}
trait ConfigView {
    /// 读操作对象项目的 lan.config.*；bridge:None 时回落纯 Rust 快照（A.5(5)）
    fn project(&self, cwd: &Path) -> Option<LanConfigSnapshot>;
}
```

**② 命令包内本地配置（package-local scope）** —— 这是"每个命令都要读自己的配置文件"的核心落点。命令包发布时**自带静态配置**，例如：

```text
packages/commands/build/
├── lania.package.json
├── lania.config.yaml        # 命令自己的默认构建目标 / 脚本表 / 平台差异
└── scripts/
    ├── prebuild.sh
    └── postbuild.mjs
```

由 `host-core` 提供统一读取，**命令不自己读文件**：

```rust
trait ConfigView {
    /// 读 <命令包安装目录>/lania.config.yaml，纯 Rust（lania-config），零 bridge
    /// install_dir 由 manifest 已知（见 §6.1），命令无需关心绝对路径
    fn package(&self, command_name: &str) -> Option<LanConfigSnapshot>;
}
```

> 双形态一致：API 态由总包的 `host` 提供；CLI 态由 launcher 把**同一个 `host`** 注入 `ctx`（§5.3 C）。命令代码完全相同，不感知自己处于哪种形态。

#### G.2 脚本执行：统一 `ctx.script` / `ctx.tools.exec` 执行器

命令包可在 `lania.package.json` 的 `scripts` 字段（或包内 `scripts/`）声明脚本，统一经执行器运行，由 `host-core` 的 `exec`/`fs` leaf（或可选的 `bridge-host`）支撑：

```rust
trait ScriptView {
    /// 跨平台执行：shell 解析(cmd/pwsh/sh) + cwd/env/timeout/TTY 转发/stdio 捕获/退出码
    /// kind 分发：shell → exec leaf；node → bridge-host（若已链接）；package → 解析到包安装目录后执行
    fn run(&self, target: ScriptTarget, opts: ScriptOpts) -> Result<ScriptOutput, ExecutionError>;
}
enum ScriptTarget {
    Project(String),        // 操作对象项目里声明的脚本（如 dev/build），读其 package.json / lan.config
    Package(String),        // 本命令包 scripts/ 内的脚本
    Inline(String),         // 命令包内联声明的脚本体
}
```

TS 侧 handler 走 node-bridge 既有 `ctx.tools`（已含 `exec/fs/git/pm/runtime/scaffold`，`schema-tools.ts:104-126`），与 Rust 侧 `ctx.script` 是**同一语义的两个入口**：

```ts
// ts handler 内
await ctx.tools.exec('npm', ['run', 'build'], { cwd, tty: true });
await ctx.tools.fs.readFile('lania.config.yaml'); // 已统一在 ctx.tools
```

**关键约束（保住原子化）**：执行器能力来自命令 manifest 声明的依赖。若某命令未链接 `exec` leaf，`ctx.script.run` 返回明确的 `CapabilityNotLinked` 错误而非静默失败——命令作者因此被迫在 manifest 里声明 `hostDependencies: ["lania-exec"]`，依赖图保持诚实（A.1 矩阵同理）。

#### G.3 双形态一致性（API 态 vs CLI 态）

两个形态注入的是**同一个 trait 对象类型**，命令代码零分支：

| 形态                             | `host` 由谁构建                                               | `ctx.config.project`                         | `ctx.script.run`            |
| -------------------------------- | ------------------------------------------------------------- | -------------------------------------------- | --------------------------- |
| API 态（被总包静态链接）         | 总包 `HostRuntime`（host-core 全量 + bridge-host 全 adaptor） | `.ts` 走 Node、`yaml` 走 Rust                | 全能力可用                  |
| CLI 态（`lan-<cmd>` 独立二进制） | 该命令 manifest 声明的 `hostDependencies` 构建                | bridge:**None** 时回落纯 Rust 快照（A.5(5)） | 只要链了 `exec` leaf 即可用 |

这正是 §5.3 C「双组装」目标在"配置 / 脚本"上的落地：命令作者只写 `ctx.config.xxx` / `ctx.script.run(...)`，完全不感知自己被总包静态链还是被单独安装。

#### G.4 与既有设计衔接（不重复造轮子）

- 复用 **A.5(5)** 的 `ConfigSource` 抽象与 `config.loadLan` 路由，不再新建配置读取层；`lania-config` 纯 Rust 实现直接服务 `package()` 与 `project()` 的 yaml/json 分支。
- 复用 host-core 既有 13 leaf 中的 `lania-exec` / `lania-fs` / `lania-config` / `lania-hooks`（脚本前置/后置 hook 已由 hooks leaf 覆盖，无需命令自管）。
- TS 侧直接复用 node-bridge 既有 `ctx.tools`（`schema-tools.ts:104-126`），**不新增 TS 执行器**。
- `bridge-slice-config`（`§5.2 B.4`）已把 `configPlugin` 切片挂到"所有需 `config.loadLan` 的命令"；G 节把它的消费方式固化成 `ctx.config` 访问器。

#### G.5 manifest 字段补充 + 命令作者写法

`lania.package.json`（§6.1）新增可选 `scripts` 与 `hostDependencies` 字段，由 schema 校验，禁止硬编码路径：

```jsonc
// packages/commands/build/lania.package.json
{
  "kind": "command",
  "command": "build",
  "rust": {
    "lib": "lania-plugins-command-build",
    "bin": "lan-build",
    "factory": "lania_plugins_command_build::factory",
    "spec": "lania_plugins_command_build::spec",
  },
  "hostDependencies": ["lania-exec", "lania-fs", "lania-config"], // 声明后才可用 ctx.script/ctx.config.package
  "scripts": { "prebuild": "scripts/prebuild.sh", "postbuild": "scripts/postbuild.mjs" },
  "bridgeSlices": ["config", "agent"],
}
```

命令包里作者只写：

```rust
// packages/commands/build/rust/src/lib.rs
fn execute(&self, ctx: &CommandContext, host: &dyn HostApi) -> Result<CommandExecution, ExecutionError> {
    let cfg = host.config().package("build")                 // 读本命令的 lania.config.yaml
        .ok_or(ExecutionError::config_missing("build"))?;
    host.script().run(ScriptTarget::Package("prebuild".into()),  // 跑包内脚本，跨平台/TTY 自动处理
        ScriptOpts { tty: true, ..Default::default() })?;
    let proj = host.config().project(&ctx.cwd);              // 读操作对象项目配置
    // ... 真正的 build 逻辑
    Ok(CommandExecution::ok())
}
```

#### G.6 验收（追加到 §7.2 验收清单）

- 新增验收项：**任意命令（以 `build` 为例）在「全量 cli」与「`core + cmd-build` 独立安装」两种形态下，读 `lania.config.yaml` 与跑 `scripts/prebuild.sh` 的输出逐字节一致**（golden 新增"命令读配置 + 跑脚本"用例）。
- `cargo tree -p lan-env` **不得**出现 `lania-exec`（env 不声明 `hostDependencies: ["lania-exec"]`）；`cargo tree -p lan-build` **必须**出现 `lania-exec` / `lania-fs`，且 `ctx.script` 可用。
- 命令包内 `grep -rn "std::fs::read\|std::process::Command\|tokio::process" packages/commands/*/rust/src` 应为**空**（统一经 `ctx.config` / `ctx.script`，无命令自管文件/进程）。

---

## 6. 构建、打包、发布与版本族

### 6.1 `lania.package.json`：单一事实源

每个 `packages/*/` 一份，由 `packages/schemas/lania.package.schema.json` 校验。

| 字段                                                                                               | 含义                                                          |
| -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `name` / `kind`                                                                                    | 包标识；`kind ∈ {foundation, product, command, distribution}` |
| `command`                                                                                          | **命令名**（= `spec().name`，仅 command 类）                  |
| `versionFamily`                                                                                    | `lan` \| `runex` \| `agent` \| `bridge-hub`                   |
| `version` / `api.protocol`                                                                         | 版本与 API 协议整数                                           |
| `npm.name` / `npm.bin` / `npm.optionalDependencies` / `npm.platformPackages[]`                     | npm 分发描述（`os`/`cpu`/`binary` 显式化，取代前缀派生）      |
| `rust.cratePath` / `rust.lib` / `rust.bin` / `rust.features` / `rust.api.{factory,spec,surface[]}` | Cargo 描述                                                    |
| `payload.slices[]` / `payload.nodeBridge`                                                          | TS payload 切片                                               |
| `shimHost`                                                                                         | `lan` \| `runex-binary` \| `command-binary`（见 §6.6）        |
| `describe` / `order`                                                                               | 构建期 `--describe` 产物、总包注册顺序                        |

**字段类型约束（`lania.package.schema.json` 摘录）**：

```json
{
  "type": "object",
  "required": ["name", "kind", "versionFamily", "version", "api"],
  "properties": {
    "kind": { "enum": ["foundation", "product", "command", "distribution"] },
    "command": { "type": "string", "description": "仅 command 类必有，必须等于 spec().name" },
    "versionFamily": { "enum": ["lan", "runex", "agent", "bridge-hub"] },
    "version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+$" },
    "api": {
      "type": "object",
      "required": ["protocol", "rust"],
      "properties": {
        "protocol": { "type": "integer", "minimum": 1 },
        "rust": {
          "type": "object",
          "required": ["lib"],
          "properties": {
            "cratePath": { "type": "string", "default": "rust" },
            "lib": { "type": "string", "description": "Cargo crate 名" },
            "bin": { "type": "string", "description": "[[bin]] 名，如 lan-env / lania-cli" },
            "features": { "type": "array", "items": { "type": "string" } },
            "dependencies": {
              "type": "array",
              "items": { "type": "string" },
              "description": "命令二进制显式链接的额外仓内 crate"
            },
            "api": {
              "type": "object",
              "required": ["factory", "spec", "surface"],
              "properties": {
                "factory": {
                  "type": "string",
                  "description": "如 lania_plugins_command_env::factory"
                },
                "spec": { "type": "string", "description": "如 lania_plugins_command_env::spec" },
                "surface": {
                  "type": "array",
                  "items": { "type": "string" },
                  "description": "冻结导出面，驱动 D.2 守卫与 api.protocol 校验"
                }
              }
            }
          }
        }
      }
    },
    "npm": {
      "type": "object",
      "description": "仅 command/distribution 类；含 name/bin/optionalDependencies/platformPackages[]"
    },
    "payload": {
      "type": "object",
      "properties": {
        "slices": {
          "type": "array",
          "items": { "type": "string" },
          "description": "空数组或 [\"*\"]；驱动 LANIA_BRIDGE_SLICES（B.4）"
        },
        "nodeBridge": {
          "type": "boolean",
          "description": "是否需要 Node 运行时（注入 LANIA_NODE_BRIDGE_DIR）"
        }
      }
    },
    "shimHost": { "enum": ["lan", "runex-binary", "command-binary"], "description": "见 §6.6" },
    "describe": { "type": "string" },
    "order": { "type": "integer", "description": "总包注册顺序，必须等于 main.rs:62-75" }
  }
}
```

> 此 schema 是「单一事实源」的机器校验层：`scripts/pack.mjs`（§6.2）、`gen-builtin-commands.mjs`（D.1）、`gen-known-commands.mjs`（C.7）、`release.mjs`（§6.5）、CI 矩阵（§6.4）全部从它派生，不再各自硬编码包名/路径。Cargo workspace 的 `members` 由各 `rust.cratePath`/`rust.lib` 推出（§6.3），根 `Cargo.toml` 用 glob `packages/*/*/rust` 收口，新增命令包无需改根配置。

**示例 1 — `packages/commands/env/lania.package.json`（无 JS 命令）**

```json
{
  "$schema": "../../schemas/lania.package.schema.json",
  "name": "cmd-env",
  "kind": "command",
  "command": "env",
  "versionFamily": "lan",
  "version": "0.0.3",
  "api.protocol": 1,
  "order": 11,
  "npm": {
    "name": "@lania-cli-v2/cmd-env",
    "bin": { "lan-env": "bin/lan-env.mjs" },
    "optionalDependencies": {
      "@lania-cli-v2/cmd-env-darwin-arm64": "0.0.3",
      "@lania-cli-v2/cmd-env-linux-x64": "0.0.3",
      "@lania-cli-v2/cmd-env-win32-x64": "0.0.3"
    },
    "platformPackages": [
      {
        "name": "@lania-cli-v2/cmd-env-darwin-arm64",
        "os": "darwin",
        "cpu": "arm64",
        "binary": "lan-env"
      },
      {
        "name": "@lania-cli-v2/cmd-env-linux-x64",
        "os": "linux",
        "cpu": "x64",
        "binary": "lan-env"
      },
      {
        "name": "@lania-cli-v2/cmd-env-win32-x64",
        "os": "win32",
        "cpu": "x64",
        "binary": "lan-env.exe"
      }
    ]
  },
  "rust": {
    "cratePath": "rust",
    "lib": "lania-plugins-command-env",
    "bin": "lan-env",
    "features": [],
    "api": {
      "factory": "lania_plugins_command_env::factory",
      "spec": "lania_plugins_command_env::spec",
      "surface": ["COMMAND_NAME", "HANDLER_ID", "spec", "factory", "EnvCommandPlugin"]
    }
  },
  "payload": { "slices": [], "nodeBridge": false },
  "shimHost": "command-binary",
  "describe": "describes/env.json"
}
```

**示例 2 — `packages/commands/pi/lania.package.json`（带 agent 切片）**

```json
{
  "name": "cmd-pi",
  "kind": "command",
  "command": "pi",
  "versionFamily": "lan",
  "version": "0.0.3",
  "api.protocol": 1,
  "order": 12,
  "rust": {
    "lib": "lania-plugins-command-pi",
    "bin": "lan-pi",
    "dependencies": ["lania-bridge-host", "lania-agent", "lania-agent-core"]
  },
  "payload": { "slices": ["config", "agent", "system"], "nodeBridge": true },
  "shimHost": "command-binary"
}
```

**示例 3 — `packages/distribution/cli/npm/cli/lania.package.json`（总包）**

```json
{
  "name": "cli",
  "kind": "distribution",
  "versionFamily": "lan",
  "version": "0.0.3",
  "npm": {
    "name": "@lania-cli-v2/cli",
    "dependencies": { "@lania-cli-v2/core": "^0.0.3" },
    "optionalDependencies": {
      "@lania-cli-v2/cli-darwin-arm64": "0.0.3",
      "@lania-cli-v2/cli-linux-x64": "0.0.3",
      "@lania-cli-v2/cli-win32-x64": "0.0.3"
    }
  },
  "rust": { "bin": "lania-cli", "features": ["*"] },
  "payload": { "slices": ["*"], "nodeBridge": true },
  "shimHost": "lan"
}
```

**示例 4 — `packages/distribution/core/lania.package.json`**

```json
{
  "name": "core",
  "kind": "distribution",
  "versionFamily": "lan",
  "version": "0.0.3",
  "api.protocol": 1,
  "supportedProtocols": [1],
  "npm": { "name": "@lania-cli-v2/core", "bin": { "lan": "bin/lan.mjs" } }
}
```

**示例 5 — `packages/runtime/lania.package.json`（可扩展宿主设施，见 §5.6 F）**

```json
{
  "$schema": "../../schemas/lania.package.schema.json",
  "name": "runtime",
  "kind": "command-runtime",
  "versionFamily": "lan",
  "version": "0.0.3",
  "api.protocol": 1,
  "rust": {
    "cratePath": "rust/lania-command-runtime",
    "lib": "lania-command-runtime",
    "bins": ["lan-runtime"],
    "links": ["host-core", "bridge-host", "node-bridge"]
  },
  "npm": {
    "name": "@lania-cli-v2/runtime",
    "bin": { "lan-runtime": "bin/lan-runtime.mjs" },
    "platformPackages": [
      "@lania-cli-v2/runtime-darwin-arm64",
      "@lania-cli-v2/runtime-linux-x64",
      "@lania-cli-v2/runtime-win32-x64"
    ]
  },
  "payload": {
    "nodeBridge": true,
    "slices": ["dynamic-commands", "scaffold", "templates"]
  },
  "shimHost": "command-binary"
}
```

- `kind: "command-runtime"`：**不进入** `known-commands.json`（不是 named 命令），但被 `core` 的 launcher 在 Phase 2 动态回退时按需调用（§5.6 F.2）。
- `links` 全量 `host-core` + `bridge-host` + `node-bridge`：保证 `ctx.tools`（`exec/fs/git/pm/scaffold/...`，`schema-tools.ts:104-126`）与现状 `lania-cli` 完全一致。
- `core` 对它的依赖为 **optional/peer**：纯净「只装 env」安装不含它；`lan` 遇到动态命令且它缺失时给出 `npm i @lania-cli-v2/runtime` 提示（§5.6 F.1）。

### 6.2 `scripts/pack.mjs` 从硬编码改为 manifest 驱动

| 现有硬编码                                                                  | 改造                                                                                                                                                                                      |
| --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `stageNodeBridgePayload()` 写死 `npm/cli/lib/node-bridge` + 3 个 dist       | `discoverPackages()` 扫 `packages/*/lania.package.json`；按 `payload.slices` 组装**每个包自己的** `lib/bridge/`，`npm install --omit=dev` 只装该切片依赖；总包 `["*"]` 保持全量逐文件一致 |
| `cargo build --release -p lania-cli`                                        | 从 manifest 汇总 `rust.bin` 生成 `-p` 列表，**一次** `cargo build --release -p lania-cli -p lan-dev ... -p lan-pi -p runex`（共享 `target/` 缓存；config 不产出独立 bin，决策 #11）       |
| `readRustWorkspaceVersion()` 从 `rust/Cargo.toml` 取全局版本                | 改为 `readVersionFamily(pkg)` 从 manifest 取；删除 `[workspace.package] version`                                                                                                          |
| `assertVersionSync(expected)` 校验写死路径                                  | `assertVersionFamilies(packages)`：按 `versionFamily` 分组，组内 `package.json` / `Cargo.toml` / 平台包 / `describes/*.json` 全等，**组间允许不同**                                       |
| `discoverPlatformPackageDescriptors()` 靠 `optionalDependencies` + 前缀派生 | 直接读 `npm.platformPackages[]`（含 `os`/`cpu`/`binary`），不再字符串替换                                                                                                                 |
| `assertStagedBinaryVersion()` 用全局 `expected`                             | 按 `descriptor.versionFamily` 取期望版本；仅宿主平台跑 `--version`，跨平台资产按 tag 拉取时标签须能解析出「命令名 + 平台 + 版本族」防串包                                                 |
| `stagePlatformBinaries()` 单一 `rust/target/release/lania-cli`              | 遍历所有 `rust.bin`，从 `target/release/<bin>[.exe]` 取产物                                                                                                                               |
| `fetchPlatformBinaries()` 资产名 `lania-cli-<platform>`                     | 扩展为 `lan-<cmd>-<version>-<platform>` / `lania-cli-<version>-<platform>` / `runex-...`，按 `versionFamily` 的 tag 拉取                                                                  |

`scripts/release.mjs:21-39` `discoverAllPackages()` → `discoverPackages()`；`bumpRustWorkspaceVersion()` / `bumpCliOptionalDeps()` → 按 manifest 的 `npm.optionalDependencies` 与 `rust.cratePath` 逐一 bump，只 bump **同一版本族**。`scripts/publish-all.mjs:25-28` 4 行 → 拓扑排序生成发布序列（平台包 → 命令包 → core → 总包；主包按各自 tag 独立发布）。

### 6.3 Cargo workspace

见 §3.1 glob `members = ["packages/*/rust/*"]`。要点：

- 删除 `[workspace.package] version`（全局版本锁），各 crate 显式 `version = "<其 versionFamily 版本>"`；`[workspace.dependencies]`（第三方）保留。
- `runex/Cargo.toml` 已是独立 `0.1.0`；`products/agent`、`products/bridge-hub` 同理。
- 删除 `rust/Cargo.toml`（内容上移为根 `Cargo.toml`）与 `ts/pnpm-workspace.yaml`，最终**只有一套 workspace**。
- CI 增加 `cargo metadata --no-deps` + `pnpm list -r --depth -1` 与 manifest 枚举结果的一致性断言。

### 6.4 CI 产物矩阵

由 `scripts/gen-artifact-matrix.mjs` 从全部 manifest 生成 `artifact-matrix.json`，经 `$GITHUB_OUTPUT` 注入 `strategy.matrix.include`（GitHub Actions 支持 JSON 动态 matrix）：

| 产物族                                      | 平台数                                       | 资产数 | 触发 tag                     |
| ------------------------------------------- | -------------------------------------------- | ------ | ---------------------------- |
| 全量 `lania-cli`                            | 3（沿用 `release-binaries.yml`）             | 3      | `lan-v*`                     |
| 12 × `lan-<cmd>`                            | 3                                            | **36** | `lan-v*`                     |
| `lan-rs-bridge` 独立 bin                    | 3                                            | 3      | `lan-v*`                     |
| `runex`（保持现状）                         | 5（Windows `-C target-feature=+crt-static`） | 5      | `runex-v*`                   |
| `agent` / `bridge-hub` 独立 bin（若发 npm） | 3                                            | 6      | `agent-v*` / `bridge-hub-v*` |

**成本与缓存策略**：

- 12 个 `lan-<cmd>` 共享 90%+ 编译依赖（foundation + host-core）。**绝不为每个 bin 起一个独立 job**：单平台单 job 内一次 `cargo build --release --bins`（或生成 `-p` 列表一次性传入），Cargo 复用 `target/` 里的 rlib，12 个 bin 只是 **12 次链接**，成本 ≈ 1 次全量编译 + 12 次 link。
- `Swatinem/rust-cache@v2` 的 `workspaces` 从 `rust` 改为根 `Cargo.toml` 路径；cache key 增加 `hashFiles('**/Cargo.lock', 'packages/*/lania.package.json')`。
- 一次 job 上传全部带平台后缀的资产（`fail_on_unmatched_files: true` 沿用），避免 39 个 job。
- **分 tag 触发**：`lan-v*` 只构建 lan 族，`runex-v*` 只构建 runex，主包各自独立，避免一次 tag 触发全矩阵。
- 增量发布：初期全量重建；后续用 `dorny/paths-filter` 或 Cargo 依赖图只重建受影响命令包。

### 6.5 版本族与兼容矩阵

- 解除 `rust/Cargo.toml` 的 `[workspace.package] version = 0.0.3` 全局锁；解除 `.changeset/config.json` 的单一 `fixed` 组（现锁 6 包）。
- **4 个版本族**，各自 changesets `fixed` 组：

| 版本族       | 成员                                                                                                                        | tag                                  |
| ------------ | --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ |
| `lan`        | `core` + `cli` + 3 `cli-*` + 12 `cmd-*` + 各 `cmd-*-<platform>` + `bridge-runtime` + 12 个 `bridge-slice-*` + `describes/*` | `lan-v*`                             |
| `runex`      | `runex` / `runex-core` / `runex-home` / 5 平台包                                                                            | `runex-v*`（现状保持，版本 `0.1.0`） |
| `agent`      | `lania-agent*` / `@lania-cli-v2/agent`                                                                                      | `agent-v*`                           |
| `bridge-hub` | 4 个 `lania-bridge-*` / adaptors / `@lania-cli-v2/bridge-hub`                                                               | `bridge-hub-v*`                      |

- `scripts/sync-rust-version.mjs` 改为**只同步 `lan` 族 crate**；其余版本族由各自 bump 脚本维护 `Cargo.toml`。
- 兼容矩阵 `packages/distribution/core/npm/core/lib/compat.json`（构建期生成）：

| 关系                                       | 约束                                                                   |
| ------------------------------------------ | ---------------------------------------------------------------------- |
| `@lania-cli-v2/cli` → `@lania-cli-v2/core` | `dependencies: "^0.0.x"`                                               |
| `core` → `cmd-*`                           | `api.protocol == SUPPORTED_PROTOCOL(1)`，**运行时**读 manifest 校验    |
| `cmd-*` → foundation / 主包                | 编译期由 Cargo 锁；`runex` lib 用 `=0.1.x`，`lania-agent` 用其独立版本 |
| `lan` 族 ↔ 其它三族                        | **无版本锁**，仅 API 协议约束（冻结导出面）                            |

- `assertStagedBinaryVersion` 泛化：所有二进制 `--version` 仍输出**纯版本号**（保持现有形态），期望值取自 `descriptor.versionFamily` 对应 manifest。

### 6.6 `ShimHost` 第三种模式

现状（`rust/crates/runex/src/api.rs:81-83` 与 `runex/src/tui/` 已确认）：`ShimHost::lan()` 生成 `lan env which`；`ShimHost::binary("runex", exe)` 生成 `runex which`。新增：

```rust
// runex-core（ShimHost 定义处）
ShimHost::command_binary("lan-env", std::env::current_exe().ok())
// 生成 shim：exec "<abs>/lan-env" which "<name>"
```

- 由 manifest 的 `shimHost` 选择：总包 `lan`、独立 runex `runex-binary`、命令子包 `command-binary`。
- **只改变 shim 里写死的管理命令，不改变 `which` 语义**；shim 写**绝对路径**到真实 `lan-<cmd>` 二进制，且该二进制不得放在 `~/.runex/shims/` 内。
- **必须遵守 `self-managed-runtime-manager.zh-CN.md` §11.4 硬约束**：`RuntimeStore::which` 只读 `state.json`，**不得回退 PATH 扫描**（否则 shim 自引用死循环）。
- 动机：shim 会被大量外部命令调用，直写 `lan-env which` 可绕过 Node launcher，零额外开销。

---

## 7. 体积口径与验收

### 7.1 对比口径

| 口径           | 只装 `core + cmd-env`                                                                                                                                                           | 装总包                                                                                                       |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| 安装单元       | `@lania-cli-v2/core` + `@lania-cli-v2/cmd-env` + `@lania-cli-v2/cmd-env-<platform>`                                                                                             | `@lania-cli-v2/cli` + 3 `cli-*` (+ core)                                                                     |
| Node 侧        | launcher + 1 个 thin wrapper（KB 级脚本）                                                                                                                                       | launcher + 全量 `lib/node-bridge/`                                                                           |
| Rust 二进制    | **1 个** `lan-env[.exe]`                                                                                                                                                        | **1 个** `lania-cli[.exe]`                                                                                   |
| 二进制静态链接 | `lania-plugins-command-env` + `lania-host-core`（零 `bridge-host`） + `runex` lib（含 `runex-core` 下载引擎、TUI，env 必需） + `runex-home` + `lania-command` + `lania-cli-kit` | 全部 12 命令 lib + `host-core` 全量 + `bridge-host` 全 adaptor + `lania-agent` / `runex` / `lania-workflows` |
| TS payload     | **0**（无 `lib/bridge/`、无 `LANIA_NODE_BRIDGE_DIR`）                                                                                                                           | 全量（node-bridge + templates + agent dist + ejs/tsx/yaml/pi-\*）                                            |

**度量方式**：`npm pack --dry-run --json` 的 `size` / `unpackedSize`；`du -sh` 对比 payload；`cargo tree -p lan-env -e normal` 统计依赖数；二进制 `ls -lh`。总包体积应**不增加**（bin 不进总包产物，lib 集合不变）。

**禁止出现清单（验收断言）**：

- `cmd-env` 的 `node_modules` 顶层不得有 `ejs` / `tsx` / `yaml` / `@earendil-works/pi-agent-core` / `@earendil-works/pi-ai` / `@lania-cli-v2/agent` / `@lania-cli-v2/templates`；
- `cargo tree -p lan-env` 不得出现 `lania-agent` / `lania-agent-core` / `lania-bridge-host` / `lania-bridge-bus` / `lania-node-bridge` / `lania-py-bridge` / `lania-go-bridge` / `lania-dart-bridge` / `lania-plugins-command-pi` / `lania-workflows` / 其它命令 crate；
- `cargo tree -p lania-command -e normal` 只含 clap/serde/serde_json（dev-deps 为空）。

### 7.2 验收清单（13 条）

1. `pnpm pack` 后，11 个 `cmd-*` 与总包 `npm pack` 成功；`cmd-env` 等零 payload 命令包内**不存在** `lib/bridge/`，manifest `payload.slices == []`。
2. 干净环境（临时 `npm prefix`）只装 `core + cmd-env`：`lan env list --json`、`lan env which node`、`lan env init`、`lan env ui`（TTY）通过；现有 `lan env` **13 条 e2e 全绿**。
3. 总包 **12 命令输出逐字节对比**迁移前 golden 基线（含 stdout JSON envelope、stderr 渲染、退出码；原 `lan template` 输出改由 `lan create template` 对齐）；`scripts/smoke-pi.mjs` **8/8**。
4. `lan --help` 在三种拓扑（`core+1 包` / `core+N 包` / `core+总包`）下聚合正确；未安装命令输出 `npm i @lania-cli-v2/cmd-<name>` 提示；`lan --refresh-help` 能重建缓存。
5. 全局参数一致性：同一 `--json` / locale / output profile，在总包与 dispatcher 下输出一致。
6. `ShimHost` 第三种模式：`lan env init` 生成的 shim exec `lan-env which <name>`，且 **`which` 不回退 PATH 扫描**。
7. 冻结导出面：11 个命令包的 `frozen_export_surface_is_intact` 测试 + `runex` 同名守卫全绿；`cargo tree` 无环；`manifest.command == spec().name` 断言通过。
   7a. **每包 config 二级命令（决策 #11，§10.2）**：`packages/foundation/package-config` 引擎 crate 落位；每个命令子包 `lan <cmd> config`（bare / get / set）可读写且**仅能**读写 `~/.lania/config/<cmd>.json`（key 按该包 manifest `configSchema` 校验）；跨包/全局偏好（`~/.lania/preferences.json`、`lan.config.*`）由总包 `lan config` 独占，子包 config 触碰即报错；总包 `lan config set locale zh` 等既有行为 golden 不变。
8. `lania-command/Cargo.toml` `[dev-dependencies]` 为空；`cargo test --workspace` 全绿；本地化覆盖测试由 `command-conformance` 承接且覆盖度不降。
9. 全局搜索 `postinstall` 为空；安装期不联网（沿用现有 `optionalDependencies` + `require.resolve`）。
10. Windows：`.exe` 命名、`os`/`cpu` 字段、shim 自引用不递归（`lan env which` / `lan env doctor` 专项 e2e）。
11. `lan __lania_rs_bridge` 在总包仍可用；按需模式由 `@lania-cli-v2/rs-bridge` 的 `lan-rs-bridge` 提供。
12. `pack.mjs` / `release.mjs` / `publish-all.mjs` / `assertVersionFamilies` / CI matrix 全部 manifest 驱动；4 个版本族可独立发版（可用 `scripts/verdaccio-smoke.mjs` 演练）。
13. **动态运行时命令与脚手架封装命令**（§5.6 F）：项目 `lan.config.*` 声明 `runtimeCommands` 的 `lan ops ping` 在**全量 cli** 与 **`core + cmd-env + runtime`** 两种安装下输出逐字节一致（golden 复用 `lania-host/src/runtime/tests.rs:167/251/301` 三类声明形态）；纯净 `core + cmd-env`（无 `runtime`）下 `lan env` 正常、`lan ops ping` 给出 `npm i @lania-cli-v2/runtime` 提示；`lan --help` 在项目目录能列出 `runtimeCommands` 挂载项；`cargo tree -p lan-env` 不含 `lania-command-runtime` / `lania-node-bridge` / `dynamic-commands`，`lan-runtime` 的 tree 必须含它们。

**施工前置（本次不做）**：把现有 12 个 builtin 命令（含由 `template` 并入 `create` 的 `create template`）输出、`smoke-pi.mjs`、`lan env` 13 条 e2e 录成 golden 基线入库，作为全程不变量的比较基准。

---

## 8. 分阶段迁移路线

> 全程不变量：每阶段结束都必须能 `pnpm pack && pnpm publish:all` 产出与现状等价的全量 `@lania-cli-v2/cli`；golden 回归失败即停在该阶段回滚。

| 阶段  | 内容                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | 验收                                                                                                                                                       | 回滚点                                                                                      |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| **0** | **本次：产出本设计文档（不改代码）**。施工前置另需录 golden 基线                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | 文档评审通过；golden 基线入库                                                                                                                              | —（无代码）                                                                                 |
| **1** | 新增 `packages/*/lania.package.json`、`packages/schemas/`、5 个 `gen-*.mjs`（describes / builtin-commands / api-surface / cargo-workspace / artifact-matrix）。**不动任何现有代码路径**                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | `discoverPackages()` 枚举结果与现状（4 npm 包 + 42 crate + 3 平台包）完全一致；全量打包发布不变                                                            | 删新增目录/文件                                                                             |
| **2** | foundation 下沉（**仍在旧 `rust/` 目录内**）：<br>2a `lania-command` 提升为 API 契约层 + dev-deps 归零（`command-conformance` 承接覆盖度）<br>2b 抽出 `lania-cli-kit`，`main.rs` 改调用 kit<br>2c 抽出 `runex-home`，`main.rs:290` 改依赖，`runex::api` 保持 `pub use`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | 全量 cli 12 命令 golden **逐字节不变**（含 `create template`）；`cargo test --workspace` 全绿；冻结守卫就位                                                | 2a/2b/2c 各自独立提交，任一失败 revert 该子步                                               |
| **3** | host 拆分：<br>3a `host-core` 去 5 语言 bridge 硬依赖，改为依赖 `lania-bridge-host`（5 adaptor 内聚进 bridge-hub）<br>3b `config.loadLan` / `bootstrap_project_extensions` 经 `bridge-host` node adaptor，纯 `.yaml/.json` 由 `lania-config` 回退（不另起 config-native）<br>3c Rust 端点并入 bridge-hub（进程内 channel），删除独立 `lan-rs-bridge` 二进制                                                                                                                                                                                                                                                                                                                                                                                                                                                   | 全量 cli 链接 bridge-host 后 golden 不变；`cargo tree -p lan-env` 不含 `lania-bridge-host`                                                                 | 失败立即回滚到 lania-host 聚合态                                                            |
| **4** | TS payload 切片：<br>4a `bridge-runtime` 最小核 + `plugin-registry.ts` 动态 import（落地 `agent-architecture.zh-CN.md` A4）<br>4b 拆 12 个 slice，按 `payload.slices` 装配                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | 总包 payload 与迁移前**逐文件一致**；`bridge-runtime` 空 slices 启动不加载 agent/ejs                                                                       | 总包 `slices:["*"]` 恒可用                                                                  |
| **5** | 命令双形态：<br>5a 11 crate 补 `factory()` + 冻结守卫 + `[[bin]] lan-<cmd>`，`run_single_command` 走 kit；先落 `packages/foundation/package-config` 引擎，随后每迁一包随迁补 `config` 二级命令（决策 #11 / §10.2）<br>5b **一次迁一个包**：`rust/crates/lania-plugins-command-*` → `packages/commands/*/rust/`、`ts/packages/*` → `packages/*/ts/`、`npm/*` → `packages/*/npm/`，Cargo workspace 切 glob（config 除外，决策 #11）<br>5c 总包 Rust 侧（`lania-cli` / `lania-host` facade / `lania-plugins-command-locale`）迁入 `packages/distribution/cli/rust/` 后，删除顶层 `ts/` `rust/` `npm/` 与 `ts/pnpm-workspace.yaml`<br>5d **template 并入 create**：删除 `commands/template` 子包，把原 handler/spec 迁入 `create` 的 `template` 子命令分发，`known-commands.json` 与 `BUILTIN` 由 13 项降为 12 项 | 每迁一包跑全量 golden；`cargo metadata` + `pnpm list -r` 与 manifest 完全一致；全量 cli 仍可发布；`lan create template` 输出与旧 `lan template` 逐字节一致 | 5b 每包 git 原子提交；5d 单独提交并附 `lan template` → `lan create template` 的 golden 对照 |
| **6** | core 调度器与分发：<br>6a launcher + `known-commands.json` + `describes/*` + help 聚合/缓存<br>6b 总包去 `bin.lan`、依赖 core；平台包命名不变<br>6c 发布 11 个 `cmd-*`，跑「只装 core+cmd-env」smoke<br>6d 新增 `packages/runtime/`（可扩展宿主设施，§5.6 F）：`lan-runtime` 二进制 + node-bridge 的 `dynamic-commands`/`scaffold`/`templates` 切片；launcher 增加 Phase 2 动态回退；`core` 对 `runtime` 为 optional/peer 依赖                                                                                                                                                                                                                                                                                                                                                                                | **§7 清单全绿**（含第 13 条动态命令）；总包 golden 逐字节不变（硬门槛，不过不发布）                                                                        | launcher 异常可一键退回旧 `lan.mjs` 路径（保留一个发布周期）                                |
| **7** | 构建发布全量落地：pack/release/publish/assert/CI matrix 全部 manifest 驱动；runex/agent/bridge-hub 的 npm 包与独立 bin 矩阵                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | 4 版本族可独立发版；产物矩阵可复现；`assertVersionFamilies` 泛化通过                                                                                       | 脚本均保留 `--legacy` 路径一个发布周期                                                      |
| **8** | Bridge Hub 集成（进程内嵌）+ 旧硬编码清理 + §9 未决项收敛 + 文档更新                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | `bridge-layer.zh-CN.md` §6 未决项标记为「已定：内嵌」；全量回归                                                                                            | feature 开关可关闭 Hub 回到直连                                                             |

**施工进度（2026-11-07 更新）**：

- ✅ 阶段 0：本文档产出；bridgehub（Bridge Hub 消息总线）已先实现并提交（`7723131`，master）。
- ✅ 阶段 1（部分）：manifest 地基已落地——6 个包的 `lania.package.json`、`packages/schemas/lania.package.schema.json`、`scripts/check-package-manifests.mjs`、`scripts/lib/discover-packages.mjs`，并接入 `pnpm check`（`packages:check`）。
- ✅ 阶段 2b / 2c：`lania-cli-kit`（output/profile + `run_single_command`）、`runex-home` 已抽出。
- ✅ 阶段 3a / 3b（大部分）：`host-core`（零 bridge 接缝）、`lania-bridge-host`（`HubBridgeProvider`）已落地；5 个语言 bridge adaptor 内聚进 `packages/products/bridge-hub/`；bridge-layer 文档已更新。
- ✅ 阶段 5b（首包）：`env` 已迁移为 `packages/commands/env/`（lib + `[[bin]] lan-env` + npm `cmd-env` + 3 平台包 + describe），双形态 help 逐字节一致，拆分零回归（提交 `505980a`）。
- ✅ 阶段 5a（部分）：决策 #11 落地——`packages/foundation/package-config`（包级偏好引擎，`~/.lania/config/<pkg>.json` 按包隔离 + schema 校验）与 `lan env config` 二级命令（提交 `d233289`）；§9.5 风险 5 的配套机制就绪。
- ✅ 公共下沉阶段 1：16 个 capabilities crate 1:1 迁入 `packages/foundation/capabilities/rust/`（13 host leaf + presentation/format/validate，提交 `c5571e4..46fcbf6`，每 crate 一原子提交）；顺带修复 dart 遥测污染格式化退出码缺陷（`9807aca`）。
- ✅ 阶段 2a：`lania-command` 迁入 `packages/foundation/command/rust/`（`0e7dbc7`）；dev-deps 环已打破——新建 `packages/foundation/command-conformance/rust/lania-command-conformance` 测试专用 crate 承接本地化覆盖测试（`20ff9c3`），覆盖度不降，§9.5 风险 5 消除；`cargo tree -p lania-command -e normal` 仅 clap/serde/serde_json。
- ✅ 公共下沉 TS-1：`ts/packages/lania`（作者侧 DSL，npm 名不变）迁入 `packages/foundation/lania/`；`tsconfig.base.json` 上移仓库根；pnpm workspace 增补 `packages/**/npm|ts/*` glob，env/runex/bridge-hub npm 包纳入单一 workspace。
- ✅ 公共下沉 TS-2：`ts/packages/templates`（脚手架资产包，npm 名不变）迁入 `packages/foundation/templates/`；pack/clean/release 脚本路径同步。
- ⬜ 其余阶段按本表推进。

**下一阶段施工计划（文档先行：先抽公共，再逐命令拆分）**：

1. **公共部分下沉（先于任何新命令包）**——把 `rust/`、`ts/` 中「公共 / 可独立成目录」的代码迁入 `packages/foundation/`，每次迁移一个独立 commit，迁完即跑 `cargo check --workspace` + 相关单测 + 总包 golden：
   1. capabilities leaf crate 按 §3.2 清单 1:1 迁移（crate 名不变，目录名 = crate 名）：`lania-config / hooks / exec / fs / git / logger / pm / progress / prompt / runtime / task / workflows / preferences` → `packages/foundation/capabilities/rust/<crate>/`（与 §3.2 目录树一致，capabilities 下暂不加包 manifest，待包清单阶段统一登记）✅；
   2. `lania-command` → `packages/foundation/command/rust/`；`command-conformance` 测试 crate（承接 dev-deps 环，见 §5.1 A.5(7) 步 5）✅；
   3. TS 公共部分：`packages/foundation/bridge-runtime/`（最小核：电器自持、插座经 bridgehub `protocol/gen/ts/runtime.ts`，决策 #12）与 `bridge-slices/*` 切片包（阶段 4a/4b，现状文件级映射见 §5.2 B.5）；`ts/packages/lania`（作者 DSL，npm 名不变）→ `packages/foundation/lania/`；`ts/packages/templates`（脚手架资产包，npm 名不变）→ `packages/foundation/templates/`；
   4. `packages/foundation/package-config`（决策 #11 的配置引擎，见 §10.2 修订版）✅；
   5. 总包 Rust 侧归位（阶段 5c 执行）：`lania-cli`（总包硬基线）、`lania-host`（facade）、`lania-plugins-command-locale`（config 命令，决策 #11）迁入 `packages/distribution/cli/rust/`，随后删除顶层 `rust/`。
2. **按命令逐个拆子包**（每包一个原子 commit + 全量 golden，顺序按依赖从少到多）：`env` ✅ → `tools` → `generate` → `create`/`add` → `release`/`git` → `dev`/`build`/`lint` → `pi`；`config` 不拆（决策 #11）。
3. 每迁一包随迁补该包 `config` 二级命令（`~/.lania/config/<pkg>.json` + manifest `configSchema`），先 env 后续包复用同一引擎。

---

## 9. 风险与未决项

1. **spawn 开销与 TUI/交互命令**（`lan pi` REPL、`lan env ui` TUI、`lan dev` watch）：`stdio:'inherit'` 保证 TTY 归子进程，进程层数与现状等价（都是一层 Node → 二进制），**无额外 spawn 开销**。风险在信号链：Ctrl+C 在 `inherit` 下由终端发给进程组，若 Node 父进程先退出子进程会变孤儿 → launcher 必须在 `child.on('exit')` 前保持存活并转发（沿用 `lan.mjs:163-170`）。Windows 控制台进程组、`lan dev` 长驻 watch 热重载需专项 e2e。
2. **Windows shim 自引用 / 递归**：`ShimHost::command_binary("lan-env", exe)` 生成的 shim 位于 `~/.runex/shims/`；约束「写绝对路径」「`lan-<cmd>` 不放入 shims 目录」「`which` 不回退 PATH 扫描」三者缺一不可。需独立 e2e 覆盖 `lan env which` 与 `lan env doctor`。
3. **Bridge Hub 独立 / 内嵌**：本文已给明确答案（每进程内嵌，见 §5.5），但仍列为待验证项：若出现「一条命令内多语言端点 + 端点需跨命令复用」的长驻场景，需重评 sidecar；`lan-hub` 独立 bin + feature 已留兜底。
4. **11 × 3 产物矩阵**：33+ 资产的 CI 时间/存储、`fetch-platform-binaries.mjs` 下载量与命名、Release 资产数。缓解：单 job 多 link、分 tag 触发、共享 `target/` 缓存、增量重建。用户已拍板首批全发。（原「12 × 3」随决策 #11 config 不成子包降为 11。）
5. ~~**`lania-command` dev-dep 环**~~（已消除，2026-11-07）：反向 dev-dep 已在阶段 2a 打破（提交 `0e7dbc7` / `20ff9c3`）；本地化覆盖测试由 `command-conformance` 承接且覆盖度不降（1/1 绿）；`cargo tree -p lania-command -e normal` 仅 clap/serde/serde_json，命令子包可无环依赖它。
6. **命令间共享状态与项目动态命令**：单进程 `HostRuntime` 一次性加载 `LanConfigSnapshot` 并注入 envelope；拆多进程后每命令各读一次 —— 现状每条命令启动本来就是新进程，语义等价。但项目级动态命令（`lan.config.*` / `.lania/schemas`）在子包模式下只有带 `config` slice 的命令包能看见；`known-commands.json` **不含动态命令**，`lan --help` 也不呈现它们，只给「装总包或带 config 切片的包」提示。与「不做完全热插拔」一致，不发明新机制。
7. **总包去 `bin.lan` 的安装拓扑变化**：单独装 `@lania-cli-v2/cli` 后 `lan` 由 core 依赖带入；若 npm 的 `--omit=optional` 或某些包管理器裁剪依赖，可能得不到 `lan`。缓解：把 core 放进 `dependencies`（非 optional/peer），并在验收清单加「装 cli 后 `lan --version` 可用」。
8. **`lan.config.ts` 与 `config.loadLan` 的隐式依赖**：`env` / `config` 完全不带 payload 后，项目级 locale / output profile 由 `lania-config`（纯 Rust、零 bridge，复用现有 crate，不另起 config-native）读 `lan.config.yaml/json` 补齐；`.ts` 配置仍需 Node/tsx，回落 preferences + warning。此差异登记为已知边界，不破坏「行为不变」（现状 `.ok()` 本就吞掉加载失败）。
9. **`spec()` / `CommandSpec` 序列化漂移**：`--describe` JSON 与 `known-commands.json` 依赖 `CommandSpec` 的 `Serialize` 形态。建议增加 `schemaVersion` 并纳入冻结导出面，避免字段增减导致 help 渲染错乱。
10. **`assertStagedBinaryVersion` 跨平台泛化**：现有只对宿主平台跑 `--version`、其它靠「按 `v{version}` 拉取」的来源标签。命令 bin × 12 后，标签必须能解析出「命令名 + 平台 + 版本族」，防止把 `lan-dev` 的资产误 staged 到 `cmd-env`。
11. **版本兼容矩阵的实际执行**：`optionalDependencies` 的平台包在 npm 里失败是静默的，`peerDependencies` 不会强校验；`api.protocol` 必须由 core launcher **运行时**读 manifest 校验，不能只写在 package.json 里。
12. **无 bundler / tsx 依赖（施工前实测）**：TS 侧动态 `import()` 必须指向 tsc 编译后的 `dist/*.js`，切片包需在各自 `package.json` 声明 `exports`；`pack.mjs:147-149` 现在装 `tsx` 是为了 `node --import tsx`，说明 `dist/entry/stdio.js` 可能仍依赖 tsx 运行时。**施工前需实测 `node dist/entry/stdio.js` 能否在无 tsx 下启动**；若不能，`bridge-runtime` 最小核需保留 `tsx`（但仍不带 ejs/agent/pi），「完全无 JS 依赖」的边界相应收紧。这不影响 `env` / `config` 零 payload 的结论（它们根本不启动 bridge）。
13. **总包 12 次注册 → 编译期数组**：`gen-builtin-commands.mjs` 生成的 `BUILTIN` 顺序必须与 `main.rs:62-75` 完全一致（template 并入 create 后为 12 项），否则 `lan --help` 输出会变。顺序写进 manifest `order` 并纳入 golden 对比。
14. **`locale` crate 与 `config` 命令名分叉**：目录/crate 用 `locale`、命令名用 `config`，易在脚本里写错。缓解：`command` 字段为唯一事实源，`gen-*` 脚本禁止从目录名推断命令名；加 CI 断言 `manifest.command == spec().name`（由 `describes/*.json` 校验）。**随决策 #11 config 不再独立成子包，该分叉的发布面（cmd-config / lan-config）已消除**；crate 名 `lania-plugins-command-locale` 保留，仅总包内部使用。

---

## 10. 方案答疑与补强（针对三问）

本节回应评审中的三个追问：**(Q1)** 本方案是否与 runex / agent / bridge-hub 三份设计文档兼容（agent 与 bridge-hub 尚未完工或未启动）；**(Q2)** 各命令子包能否增加 `config` 二级命令、把总包 config 迁入、且配置文件在包之间隔离；**(Q3)** 当前 13 个命令全部依赖 `lania-host`（Rust 宿主层），如何拆分到各命令子包。

### 10.1 Q1：与 runex / agent / bridge-hub 三份设计文档的兼容性

**结论：完全兼容，且三份文档恰好预设了本方案所需的"独立性"。** 具体逐一看：

- **runex**（`self-managed-runtime-manager.zh-CN.md`）：已设计为 **lib + bin 双形态产品**（§12 独立 `runex` CLI、§13 `runex::api::Runex` 产品 API），并已有**独立五平台发布节奏**（`release-runex.yml` 的 `runex-v*` tag、`version = 0.1.0`，独立于 lania `0.0.3`）。本方案仅把它从 `rust/crates/runex{,core}` 迁到 `packages/products/runex/{rust/,npm/}`，不改其零 lania 依赖与冻结导出面（§11）。**需同步更新的路径引用**：该文档第 485–600 行区域的 `rust/crates/runex-core/`、`rust/crates/runex/`、`rust/Cargo.toml` members 写法，在迁移后改为 `packages/products/runex/rust/...`。**桥接点**：文档 §12.4 的 `ShimHost::lan()`（生成 `lan env which`）需新增同形的 `ShimHost::command_binary("lan-env", exe)`（见 §6.6），供 dispatcher 模式下 core 调起 `lan-env`。
- **agent**（`agent-architecture.zh-CN.md`）：**已落地** `lania-agent-core`（零 lania 依赖）+ `lania-agent`（零 lania 依赖，§3.1/§10），唯一耦合宿主的是 `lania-plugins-command-pi` → 归本方案 `packages/commands/pi`。A4 待办（node-bridge 改动态 import agent）正是本方案 §5.2 payload 切片的**前置改造**；§11.4 的 Bridge Hub 待办由 §5.5/§10.3 收敛。也就是说 **agent 是最兼容的**，按"双形态"约定继续做即可，**无需返工**。
- **bridge-hub**（`bridge-layer.zh-CN.md`）：文档状态为"方案定稿 / 代码暂停未实施"，本方案直接提供 `packages/products/bridge-hub/{rust/,ts/}` 包壳，并定调**进程内嵌**（§5.5），由此**闭合其 §6 未决项**（独立 Hub 进程 vs 嵌入 CLI）。文档里的 `bridge/protocol`、`bridge/adaptors/<lang>/` 路径在本文映射为 `packages/products/bridge-hub/`。

**去风险要点（回应"还没做完/没开始"的担忧）**：因为 agent / bridge-hub 内部实现尚未完成，正确顺序是**先把本方案的"地基"搭好并稳定**——即 `packages/*` 布局、`lania.package.json` manifest schema、构建/发布/CI 流水线——再让 agent / bridge-hub **直接实现进已经存在的包壳里**。分包地基与主包内部实现是**解耦**的：地基不假设主包内部细节，主包只需遵守"双形态"约定。这样即使主包后续大改，也不用重做分包。

### 10.2 Q2：命令子包增加 `config` 二级命令 + 配置文件包隔离

**现状事实（已读 `lania-plugins-command-locale/src/lib.rs` 核实）**：`config` 命令的命令名就是 `config`（`lib.rs:50`），`Plugin::meta().name = "command-config"`，作用域刻意共用一个命令（`lib.rs:3` 注释），分两类：

1. **全局用户偏好** `~/.lania/preferences.json`（`lania_preferences` 读写，仅 `locale` / `log.timestamps` / `output.mode` 三键）；
2. **项目配置** `lan.config.*`（项目 cwd，`lania_config` + `project_config` 模块处理 `init` / `doctor` / `sync` / `set <项目路径>`）。

**设计：三类配置作用域，隔离规则如下**——

| 作用域                   | 存储位置                     | 归属                                                                              | 是否按包隔离                                                                  |
| ------------------------ | ---------------------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| 项目配置                 | `<cwd>/lan.config.*`         | 项目级、跨命令共享                                                                | 否（单文件，每命令只读自己顶层 section，已由 `known_config_fields` 校验强制） |
| 全局用户偏好             | `~/.lania/preferences.json`  | CLI 级跨命令（locale/output.mode/log.timestamps）                                 | 否（本就是全局，由总包 `lan config` 独占）                                    |
| **包级用户偏好（新增）** | `~/.lania/config/<pkg>.json` | 命令专属（如 `env.defaultRuntime`、`agent.model`、`agent.baseUrl`、`pi.timeout`） | **是（按包隔离）**                                                            |

**机制（修订版，2026-11-06 决策 #11：config 不独立成子包）**：

- 配置引擎下沉为 foundation crate：`packages/foundation/package-config/rust/lania-package-config`（零 lania 依赖，仅 serde/serde_json/anyhow）。提供：包级偏好路径（`~/.lania/config/<pkg>.json`，`<pkg>` 只允许 `[a-z0-9-]`，杜绝路径穿越）、load/save、点分 key 的 get/set，以及 schema 校验工具（key 白名单 + 类型/枚举 + 默认值）。
- 每个命令包在 `lania.package.json` 声明 `configSchema`（该包自有 key 列表 + 类型 + 默认值），构建期与运行期共用这份事实源。
- `lan <cmd> config`（bare）/ `get <key>` / `set <key> <value>`：是 `<cmd>` 命令包内的普通二级子命令（与 `env list` 同级），作用域**硬锁定** `~/.lania/config/<cmd>.json`，key 按该包 `configSchema` 校验；引擎 API 天然只能触碰传入的 `<pkg>` 命名空间 → **包间隔离由引擎保证，不靠调用方自觉**。
- `lan config`（全局）**保留在总包**：管全局偏好（locale / log.timestamps / output.mode）+ 项目配置（`init` / `doctor` / `sync` / `set <项目路径>`），并可用 `--package <cmd>` 以总包视角管理某命名空间；`lan config list` 列出已装包的命名空间。crate `lania-plugins-command-locale` 不迁出 `rust/crates/`。
- **per-package `config` 子命令 = 同一引擎的作用域视图**，不是复制一份代码 → 无重复维护。
- **隔离保证**：子包 config 命令对 `~/.lania/preferences.json`、`lan.config.*` 与其它包的 `<pkg>.json` **无任何读写路径**（引擎拒绝 + 命令包不依赖 preferences crate）；跨包读取须经显式 API + 能力授权，而非直接文件访问。

**一个关键边界**：`init` / `doctor` / `sync` 作用于**项目** `lan.config.*`（项目级、单文件、跨命令共享），**不应**被 per-package 隔离；per-package `config` 子命令只暴露该包**自有偏好键**（bare / get / set 三形态），`init/doctor/sync` 仍归总包 `lan config` 全局掌控。

**总包模式**：`lan <cmd> config` 就是 env 等命令的一个普通子命令（与 `env list` 同级），无调度跳转；`lan config` 行为与现状逐字节一致（决策 #11 不改总包任何行为）。

**首个落地示例（env）**：`packages/commands/env/lania.package.json` 声明 `configSchema`，首个 key 为 `shim.host`（枚举 `lan` / `runex`，默认 `lan`）：`lan env config set shim.host runex` 后，`lan env use/install` 生成的 shim 里管理命令从 `lan env which`（`ShimHost::lan()`）切换为 `runex which`（`ShimHost::binary("runex", …)`）；默认值 `lan` 时与现状逐字节一致。与 §6.6 的 manifest `shimHost` 关系：manifest 决定「包形态默认值」，`shim.host` 偏好决定「用户显式覆盖」，用户覆盖优先。

### 10.3 Q3：`lania-host` 宿主层拆分到各命令子包

> 模块级落点、trait 接缝与迁移步骤的施工清单见 **§5.1 A.5**。本节给结论与映射。

**现状事实（已读 `lania-host/Cargo.toml` 与 `lania-command/Cargo.toml` 核实）**：`lania-host` **本身只是一层聚合 crate**，依赖两类：

- **能力 leaf crate（14 个，均 bridge-free）**：`lania-command`、`lania-config`、`lania-hooks`、`lania-exec`、`lania-fs`、`lania-git`、`lania-logger`、`lania-pm`、`lania-progress`、`lania-prompt`、`lania-runtime`、`lania-task`、`lania-workflows`、`lania-preferences`；
- **语言 bridge crate（5 个）**：`lania-node-bridge`、`lania-py-bridge`、`lania-go-bridge`、`lania-dart-bridge`、`lania-rs-bridge`——**这些将整体并入 `packages/products/bridge-hub/adaptors/<lang>/`（bridge-layer §4），不再作为独立 crate 散落各处**。

也就是说，所谓的"宿主层很重"几乎**全在那 5 个 bridge + `loadLan` 自动拉起 node-bridge**（`main.rs:86/110`）；能力 leaf 已经是可分叶的。这把"拆分宿主层"从"拆一个单体"降维成"去掉 bridge 依赖 + 让每命令只链所需 leaf"。

**拆分步骤**：

1. **抽 `host-core`** = `lania-host` 去掉 5 个 bridge 后的部分（即上述 14 个 leaf 的再导出 + 宿主装配逻辑 `runtime.rs` / `plugin.rs` / `catalog.rs`）。_校验门_：每个 leaf 不得 transitive 依赖任意 bridge（施工前 `grep` 验证；当前 `bridges.rs` 仅在 host 层硬编码 4 个 bridge 可执行名，leaf 不引 bridge，预期可通过）。
2. **5 个 bridge 内聚进 bridge-hub**：原 `lania-{node,py,go,dart,rs}-bridge` 整体并入 `packages/products/bridge-hub/adaptors/{ts,py,go,dart,rust}/`（bridge-layer §4 规定，桥接两端运行时直接接入即可）。`lania-host` 删掉 5 条语言硬依赖（`Cargo.toml:24-31`），改为依赖 `lania-bridge-host`（bridge-hub 的宿主侧集成 crate）；命令包需要桥接语言运行时时，依赖 `bridge-host` 即可，**不再有 `bridge-*` feature**。Rust 端点走进程内 channel，TS/Py/Go/Dart 端点走 stdio 子进程，由 `bridge-host` 统一装配。
3. **命令子包不再依赖 `lania-host`**，改为依赖 `host-core` + 各自最小 leaf 子集 + 需要时的 `lania-bridge-host`。每个命令二进制的链接集：
   - **必链**：`lania-command` + `host-core`（或最小 leaf 子集）+ `runex-home`（极小，见下）+ `lania-cli-kit`（output/profile 渲染，保证与总包同源）。
   - **按需**：`lania-bridge-host`（dev/build/lint/generate/tools/pi，即需语言运行时的 6 命令）、`lania-workflows`（create/add/release/git/generate）、`runex`（env）、`lania-agent` + `lania-agent-core`（pi）。
   - **纯 Rust 骨架**：`config`（即 locale）仅需 `host-core` 的 config/preferences/logger 子集，零 `bridge-host`、零 workflow、零 node payload。
4. **抽 `runex-home`**：从 runex 抽出 `RuntimeHome::activate()`（文档 §11.4，约百行、零下载引擎），作为每命令二进制的 leaf 依赖——因为 `main.rs:47` **无条件**激活运行时 shim，所以**每条命令二进制都要它**，但它极小，不拖入 6k 行下载逻辑。
5. **抽 `lania-cli-kit`**：从 `main.rs` 抽出 `output/` + `profile.rs`（§5.1），保证子命令二进制与总包渲染逐字节一致。
6. **Rust 端点并入 bridge-hub**：`rust/crates/lania-rs-bridge` 整体并入 `packages/products/bridge-hub/adaptors/rust/`，经进程内 channel 接入，删除独立 `lan-rs-bridge` 二进制（见 §5.1 A.3）。
7. **打破 `lania-command` 的 dev-dependencies 环**：其 `[dev-dependencies]` 反向引用 `lania-host` + 12 个命令 crate（一个 localization coverage 集成测试，`Cargo.toml:13-29` 注释说明）。把该测试迁到 `lania-cli`（二进制 crate）或新建 `lania-cli-tests`，使 `lania-command` 成为干净 leaf，命令子包才能无环依赖它。

**12 命令 → 宿主子集映射示例**（以 manifest 的 `deps` 字段为准，CI 校验"无命令二进制链接 `lania-host` 全量"）：

| 命令包                                  | 宿主依赖（去 bridge 后）                                   | 额外产品依赖                          |
| --------------------------------------- | ---------------------------------------------------------- | ------------------------------------- |
| `commands/env`                          | `host-core` + `runex-home`                                 | `runex`（零 bridge、零 node payload） |
| `config`（留总包，决策 #11）            | 不迁出 `rust/crates/`，仅随总包链接（依赖 host-core 全量） | —                                     |
| `commands/build` `dev` `lint` `tools`   | `host-core` + `lania-bridge-host`                          | —                                     |
| `commands/generate`                     | `host-core` + `lania-bridge-host` + `lania-workflows`      | —                                     |
| `commands/create` `add` `release` `git` | `host-core` + `lania-workflows`                            | —                                     |
| `commands/pi`                           | `host-core` + `lania-bridge-host`                          | `lania-agent` + `lania-agent-core`    |

**验证门**：施工后 `cargo tree -p <cmd-bin>` 不得出现 `lania-node-bridge` / `lania-py-bridge` / `lania-go-bridge` / `lania-dart-bridge`（它们已并入 bridge-hub 的 adaptors）；`env` / `config` 二进制的 tree 更不得出现 `lania-bridge-host`；`cargo bloat` 对比 `env` 二进制体积应显著低于当前 `lania-cli` 单二进制。

### 10.4 Q4：每个命令都要读配置文件 / 执行脚本，有设计吗？

**结论：原先零散覆盖、现已合成为明确契约 —— 见 §5.7 G「命令通用横切能力：配置读取与脚本执行契约」。**

评审中的追问"每个命令都可能要读配置或跑脚本"是**所有 12 个命令共享的横切需求**，而前文只把零件散落在：`lania-config`/`lania-exec`/`lania-fs` 三个 host-core leaf（A.1 依赖矩阵）、`config.loadLan` 路由（A.5(5)）、`ctx.tools`（§5.2 / §5.6）。缺的是一条**命令包作者统一入口**。本方案补强要点：

- **配置读取双层模型 + 单一 `ctx.config` 访问器**（§5.7 G.1）：
  - _项目 / 用户配置_ —— 复用 A.5(5) 的 `ConfigSource` 抽象，纯 `.yaml/.json` 走 `lania-config`（零 bridge），`.ts` 经 `BridgeProvider::call("config.loadLan")`；命令只调 `ctx.config.project(cwd)`，不感知 Node 是否拉起。
  - _命令包内本地配置_ —— 每个命令包可自带 `lania.config.yaml`（默认目标 / 脚本表 / 平台差异），由 `host-core` 统一读取（`ctx.config.package(name)`），**命令不自己读文件**。
- **脚本执行统一执行器 `ctx.script` / `ctx.tools.exec`**（§5.7 G.2）：跨平台 shell 解析、cwd/env/timeout/TTY 转发、stdio 捕获、退出码映射；支持 _项目脚本 / 包内脚本 / 内联脚本_ 三类 target；能力来自命令 manifest 声明的 `hostDependencies`，未声明则 `ctx.script.run` 返回 `CapabilityNotLinked` 而非静默失败（保住原子化）。
- **双形态一致**（§5.7 G.3）：API 态（被总包静态链接）与 CLI 态（独立 `lan-<cmd>` 二进制）注入的是**同一个 trait 对象类型**，命令代码零分支。
- **不重复造轮子**（§5.7 G.4）：复用 A.5(5) 路由、`lania-config`/`lania-exec`/`lania-fs` leaf、`ctx.tools`，不新增 TS 执行器。
- **manifest 字段 + 作者写法**（§5.7 G.5）：`lania.package.json` 新增可选 `scripts` 与 `hostDependencies`；命令作者只写 `host.config().package(...)` / `host.script().run(...)`。
- **验收追加**（§5.7 G.6，计入 §7.2）：命令读配置 + 跑脚本在「全量 cli」与「`core + cmd-build` 独立安装」两种形态下输出逐字节一致；命令包源码 `grep -rn "std::fs::read|std::process::Command"` 必须为**空**（统一经 `ctx`）。

> 一句话：每个命令"读配置 / 跑脚本"的能力都**已经设计**，且被收敛为命令包只依赖 `ctx.config` / `ctx.script` 两个访问器，命令自己不再直接碰文件系统和进程 spawn —— 这样既统一了行为，又不破坏按命令原子化拆分的初衷。

---

## 附录：引用的现存文件清单

> 以下路径均为**当前仓库现状**，供施工时核对；本文不修改其中任何文件。路径相对仓库根目录。

**设计 / 架构文档**

- `docs/design/self-managed-runtime-manager.zh-CN.md`（§11 / §11.4 / §12.4 / §13）
- `docs/design/agent-architecture.zh-CN.md`（§3.1 / §3.2 / §10 A4 / §11.4）
- `docs/design/bridge-layer.zh-CN.md`（§2 / §4 / §5 / §6 / §8）
- `docs/design/multi-runtime-bridge.zh-CN.md`（§8 包结构与发行、§9 CI）
- `docs/architecture/框架化与产品化总览.zh-CN.md`（§2 / §6 / §14）
- `docs/architecture/模块设计与通信总览.zh-CN.md`（§2 / §7 / §11 / §14）
- `docs/architecture/代码链路与数据结构补充.zh-CN.md`（§11.2 / §14.5）

**workspace 与配置**

- `rust/Cargo.toml`、`pnpm-workspace.yaml`、`ts/pnpm-workspace.yaml`、`package.json`
- `.changeset/config.json`、`.npmrc`、`ts/.npmrc`、`.gitignore`

**Rust 现状核心**

- `rust/crates/lania-cli/{Cargo.toml,src/main.rs,src/output/,src/profile.rs}`
- `rust/crates/lania-command/{Cargo.toml,src/spec.rs}`
- `rust/crates/lania-host/{Cargo.toml,src/runtime.rs,src/plugin.rs,src/runtime/bridges.rs,src/runtime/catalog.rs,src/runtime/config/loading.rs}`
- `rust/crates/lania-plugins-command-*/{Cargo.toml,src/lib.rs}`（13 个）
- `rust/crates/lania-plugins-command-locale/src/lib.rs`（命令名 `config`）
- `rust/crates/runex/{Cargo.toml,src/api.rs,src/tui/}`、`rust/crates/runex-core/`
- `rust/crates/lania-agent/` / `lania-agent-core/` / `lania-{node,py,go,dart,rs}-bridge/`

**TS payload 与分发**

- `ts/packages/node-bridge/src/core/plugin-registry.ts`
- `ts/packages/{node-bridge,templates,agent,lania}/package.json`
- `npm/cli/package.json`、`npm/cli/bin/lan.mjs`、`npm/cli-{darwin-arm64,linux-x64,win32-x64}/package.json`

**脚本与 CI**

- `scripts/{pack,release,publish,publish-all,fetch-platform-binaries,clean,sync-rust-version,smoke-pi,verdaccio-smoke}.mjs`
- `.github/workflows/{release-binaries,release-runex,docs}.yml`

---

## 附：与既有文档的关系

| 既有文档                                            | 本文的处理                                                                                                                                                                                                                                            |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `self-managed-runtime-manager.zh-CN.md` §11 / §13   | **沿用**：`runex-core` 零 lania 依赖、`runex::api::Runex` 是唯一产品 API、冻结导出面 + 编译期守卫、§11.4 的 `activate()` / `which` 约束。本文新增 `runex-home` 极小 crate 与 `ShimHost` 第三种模式，不改上述结论。                                    |
| `agent-architecture.zh-CN.md` §3.1 / §10 A4 / §11.4 | **沿用**三层拆包与零 lania 依赖边界；把 A4（node-bridge 改动态 import）升格为本文 §5.2 的前置改造；§11.4 的 Bridge Hub 待办由本文 §5.5 给出部署形态。                                                                                                 |
| `bridge-layer.zh-CN.md` §6                          | **收敛**未决项：「独立 Hub 进程还是嵌入 CLI 进程」定为**进程内嵌**；其余未决（多 provider 冲突、背压/超时/重连/鉴权、能力命名空间）仍由该文档管理。旧布局 `bridge/protocol`、`bridge/adaptors/<lang>/` 在本文映射为 `packages/products/bridge-hub/`。 |
| `multi-runtime-bridge.zh-CN.md` §8                  | **沿用** `optionalDependencies` + `os`/`cpu` + 无 postinstall 的分发契约；本文把它从「1 个 CLI × 3 平台包」推广到「12 个命令 × 3 平台包」，并由 manifest 取代前缀派生。                                                                               |
| `框架化与产品化总览.zh-CN.md` §6 / §14              | **遵守**四个稳定分发边界与「Rust 保持命令树主控权、不做完全热插拔」。本文的 `known-commands.json` 是构建期静态表，不是运行时发现。                                                                                                                    |
