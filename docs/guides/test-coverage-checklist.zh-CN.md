# Lania CLI v2 功能清单与测试覆盖检查表

## 目的

本文档列出当前仓库的全部功能模块、每项功能的测试状态，以及补充测试的行动清单。

## 现状说明：三层测试体系

包拆分完成后，当前测试由三层组成：

1. **Rust workspace 单元测试**：workspace 根在仓库根（`Cargo.toml` 在根目录），各 crate 内 `#[cfg(test)]` 覆盖命令注册、参数解析、输入构建等。
2. **phase3_e2e**：位于 `packages/distribution/cli/rust/lania-cli/tests/phase3_e2e/`，共享夹具 `common.rs` 按 launcher 语义路由子包二进制。
3. **`pnpm -r test`**：各 TS 包（bridge-runtime、bridge-slices、templates 等）自带的单元测试，随根 `pnpm test` 一起执行（`pnpm test` = `pnpm -r test && cargo test --manifest-path Cargo.toml --workspace`）。

---

## 第一部分：功能清单总表

### 1.1 内置命令（11 个）

| #   | 命令      | 执行方式                     | 子命令                                                                                   |
| --- | --------- | ---------------------------- | ---------------------------------------------------------------------------------------- |
| 1   | `dev`     | Bridge (`compiler.dev`)      | `product dev`                                                                            |
| 2   | `build`   | Bridge (`compiler.build`)    | `product build/pack/publish/inspect/doctor`                                              |
| 3   | `lint`    | Bridge (`lint.run`)          | `check`, `fix`                                                                           |
| 4   | `create`  | Workflow (`CreateWorkflow`)  | `template`（二级命令）                                                                   |
| 5   | `add`     | Workflow (`AddWorkflow`)     | —                                                                                        |
| 6   | `release` | Workflow (`ReleaseWorkflow`) | `plan`, `exec`/`run`, `resume`, `status`                                                 |
| 7   | `git`     | Workflow (`GitWorkflow`)     | `status`, `log`, `diff`, `branch *`, `tag *`, `add`, `restore`, `sync`, `commit`, `push` |
| 8   | `tools`   | Bridge / Local               | `list`, `run`, `view`                                                                    |
| 9   | `env`     | Local / Bridge               | `list`, `info`, `doctor`, `install`（dry-run 默认）                                      |
| 10  | `pi`      | Local                        | pi 引擎入口                                                                              |
| 11  | `config`  | Local (`lania_preferences`)  | `get`, `set`（各子包另有自带 `config` 二级命令）                                         |

`generate` 命令已删除并归档到本地分支 `archive/generate-command`，重新设计后再启用。

### 1.2 内置命令（由 clap 提供）

- `help` — 多级帮助
- `version` — 版本号

### 1.3 Product 二级命名空间

`product` 是 `dev`/`build` 包的共享二级命名空间：

| 子命令            | 执行方式                   | 功能                   |
| ----------------- | -------------------------- | ---------------------- |
| `product dev`     | Rust 本地 watch            | 开发模式运行产品       |
| `product build`   | Bridge (`product.build`)   | 构建产品 snapshot      |
| `product pack`    | Bridge (`product.pack`)    | 生成 install-root 布局 |
| `product publish` | Bridge (`product.publish`) | 生成 npm package       |
| `product inspect` | Bridge (`product.inspect`) | 检查产品配置           |
| `product doctor`  | Bridge (`product.doctor`)  | 诊断兼容性             |

### 1.4 Dynamic Commands 动态命令系统

**能力**：通过 `lania.schemas.*` 文件声明自定义命令

**声明式能力**：

- `commands[]` — 自定义命令（name, about, options, args, examples, prompt, workflow）
- `workflows` — 工作流函数
- `templates` — 模板元数据
- `plugins` — 外部插件引用
- `presets` — 预设配置
- `features` — 功能开关
- `hooks` — Hook 绑定
- `guards` — 前置检查
- `mergeRules` — 文件合并规则
- `resolvers` — 模板路径解析
- `postActions` — 后置动作
- `dependencyRecipes` — 依赖配方
- `promptSpec` — 交互式问题

**运行时上下文** (`DynamicCommandContext`)：

- `ctx.cwd` — 当前目录
- `ctx.mount` — 挂载点
- `ctx.argv` — 参数
- `ctx.tools` — 宿主能力代理（SchemaTools）
- `ctx.scaffold` — 脚手架计划
- `ctx.product` — 产品上下文
- `ctx.runtime` — 运行时上下文

**SchemaTools 提供的能力**（通过 Host RPC 回调 Rust）：

| 域                        | 方法示例                                                  |
| ------------------------- | --------------------------------------------------------- |
| `ctx.tools.fs.*`          | readFile, writeFile, glob, ensureDir, readJson, writeJson |
| `ctx.tools.exec.*`        | run, runSync                                              |
| `ctx.tools.git.*`         | init, commit, status                                      |
| `ctx.tools.pm.*`          | install, add, remove                                      |
| `ctx.tools.log.*`         | info, success, warn, error                                |
| `ctx.tools.tasks.*`       | runTask                                                   |
| `ctx.tools.progress.*`    | start, update, complete                                   |
| `ctx.tools.interaction.*` | prompt                                                    |
| 纯 JS 工具                | path, json, text, env, isStudio, config                   |

### 1.5 Node Bridge 切片（12 个）

Node Bridge 为「核 + 切片」结构：协议核在 `packages/foundation/bridge-runtime/ts`，切片在 `packages/foundation/bridge-slices/*`。

| 切片               | 方法                                               | 功能       |
| ------------------ | -------------------------------------------------- | ---------- |
| `config`           | `config.*`                                         | 配置能力   |
| `dynamic-commands` | `commands.resolveDynamic`, `command.invokeDynamic` | 动态命令   |
| `lifecycle`        | `lifecycle.*`                                      | 生命周期   |
| `compiler`         | `compiler.dev`, `compiler.build`                   | 编译       |
| `product`          | `product.*`                                        | 产品化     |
| `lint`             | `lint.run`                                         | Lint       |
| `toolchain`        | `toolchain.*`                                      | 工具链     |
| `system`           | `system.listCommands`                              | 系统       |
| `template`         | `template.*`                                       | 模板       |
| `agent`            | `agent.*`                                          | Agent      |
| `commitizen`       | `commitizen.run`                                   | Commitizen |
| `commitlint`       | `commitlint.run`                                   | Commitlint |

### 1.6 国际化/多语言

- 全部内建命令帮助文本支持 `en`/`zh` 双语言
- 用户提示信息支持 i18n 对象 `{ zh: "...", en: "..." }`
- 通过 `config set locale zh` 切换

---

## 第二部分：当前测试覆盖状态

### 2.1 ✅ 已覆盖（存在 E2E 或单元测试）

| 功能                         | 测试位置                                  | 覆盖内容                                                                                        | 测试平台      |
| ---------------------------- | ----------------------------------------- | ----------------------------------------------------------------------------------------------- | ------------- |
| `help` 命令                  | `phase3_e2e/help_and_config.rs`           | 默认 help、子命令 help、未知命令错误                                                            | ✅            |
| `version` 命令               | `phase3_e2e/help_and_config.rs`           | 版本号输出                                                                                      | ✅            |
| `config get/set`             | `phase3_e2e/help_and_config.rs`           | 多模式输出、持久化                                                                              | ✅            |
| 中文 help 本地化             | `phase3_e2e/help_and_config.rs`           | build/config/tools/release/product 等命令                                                       | ✅            |
| `dev` 命令                   | `phase3_e2e/build_and_lint.rs`            | vite/webpack/rollup 三种工具                                                                    | ⚠️ 需 CI 确认 |
| `build` 工作流               | `phase3_e2e/build_and_lint.rs`            | vite/webpack/rollup、watch 模式                                                                 | ⚠️ 需 CI 确认 |
| `lint` 工作流                | `phase3_e2e/build_and_lint.rs`            | lint check/fix、多工具、分组输出                                                                | ⚠️ 需 CI 确认 |
| `create` 交互式              | `phase3_e2e/create_and_add.rs`            | 交互式创建                                                                                      | ⚠️ 需 CI 确认 |
| `add` 命令                   | `phase3_e2e/create_and_add.rs`            | template 选项                                                                                   | ⚠️ 需 CI 确认 |
| `git` 工作流                 | `phase3_e2e/git_and_release.rs`           | sync / status / log / diff / branch / tag / 破坏性操作确认                                      | ✅            |
| `release` 工作流             | `phase3_e2e/git_and_release.rs`           | plan/exec/resume                                                                                | ⚠️ 需 CI 确认 |
| `template list/view`         | `phase3_e2e/template_commands.rs`         | 模板列表和查看                                                                                  | ✅            |
| 产品化分发                   | `phase3_e2e/distribution_and_dynamic.rs`  | 安装布局、二进制分发、build/pack/publish 全链路、inspect/doctor、tarball 安装                   | ⚠️ 需 CI 确认 |
| 动态命令基础(installed)      | `phase3_e2e/distribution_and_dynamic.rs`  | resolveDynamic + invokeDynamic 在 installed 模式下的完整链路                                    | ⚠️ 需 CI 确认 |
| `tools list` 命令            | `phase3_e2e/tools_commands.rs`            | list 默认输出、--filter、--names-only、--names-only --plain                                     | ✅            |
| `tools view` 命令            | `phase3_e2e/tools_commands.rs`            | view 查看文件内容                                                                               | ✅            |
| `tools --help`               | `phase3_e2e/tools_commands.rs`            | help 输出格式                                                                                   | ✅            |
| `tools run` 命令             | `phase3_e2e/tools_run.rs`                 | shebang / 扩展名识别与执行                                                                      | ✅            |
| `env` 命令                   | `phase3_e2e/env_commands.rs`              | list/info/doctor、install dry-run、非法版本拒绝                                                 | ✅            |
| 文件冲突与 force 覆盖        | `phase3_e2e/conflict_and_force.rs`        | `add --force`、create 目标目录冲突                                                              | ✅            |
| 错误场景                     | `phase3_e2e/error_scenarios.rs`           | 非法参数、缺少必选参数、不存在路径                                                              | ✅            |
| 国际化运行时行为             | `phase3_e2e/i18n_runtime.rs`              | 中文错误消息、进度/日志输出                                                                     | ✅            |
| `product dev` 本地 watch     | `phase3_e2e/product_dev_watch.rs`         | `lan product dev` watch 链路                                                                    | ✅            |
| 各 crate 单元测试            | 各 crate 内 `#[cfg(test)]`                | 命令注册、参数解析、输入构建                                                                    | ✅            |
| TS 包单元测试                | 各 TS 包（`pnpm -r test`）                | bridge-runtime entry、切片插件、模板运行时等                                                    | ✅            |
| 动态命令 dev 模式 + Host RPC | `phase3_e2e/dynamic_commands_host_rpc.rs` | `ctx.tools.fs.writeFile/readFile`、`ctx.tools.exec.run`、`ctx.tools.log.*`、runtime.mode、mount | 🐧 unix only  |

### 2.2 ⚠️ 部分覆盖

| 功能          | 问题                                                             |
| ------------- | ---------------------------------------------------------------- |
| `pi` 命令     | 仅冒烟脚本覆盖（`scripts/smoke-pi.mjs`），E2E 覆盖不足           |
| launcher 调度 | `known-commands.json` 调度逻辑以夹具方式覆盖，真实安装态覆盖不足 |

### 2.3 ❌ 未覆盖（全部平台）

| 功能                                                  | 缺失原因                |
| ----------------------------------------------------- | ----------------------- |
| Schema 解析异常路径（非法格式、插件引用失败）         | 无测试                  |
| Prompt 高阶场景（condition/goto/validate/onAnswered） | 无测试                  |
| Windows 兼容性                                        | 测试中有 `unix_fs` 引用 |

---

## 第三部分：补充测试行动清单

### 优先级 P1 — 重要能力补齐

- [ ] **P1-1：`pi` 命令 E2E 测试**
  - 从冒烟脚本（`scripts/smoke-pi.mjs`）固化为 phase3_e2e 用例

- [ ] **P1-2：Schema 解析异常路径测试（TypeScript 单元测试）**
  - 非法 schema 格式
  - 插件引用失败
  - 保留命令冲突
  - mount 缺失

### 优先级 P2 — 边缘场景与质量加固

- [ ] **P2-1：Prompt 高阶场景测试**
  - condition/goto/validate/onAnswered/returnable
  - 多 prompt 串行跳转

- [ ] **P2-2：launcher 调度真实安装态测试**
  - 真实 npm 安装后的 `known-commands.json` 路由验证

### 优先级 P3 — 长期改进

- [ ] **P3-1：Git 集成测试（lania-git crate）**
- [ ] **P3-2：任务/进度系统测试（lania-task, lania-progress）**
- [ ] **P3-3：Windows 兼容性修复**
  - 修复 `unix_fs` 引用，使 `common.rs` 全平台编译
  - 验证 Node bridge 子进程在 Windows 上的路径解析
  - 在 Windows CI 上运行测试套件

> 已知环境限制：`build_and_lint` 等 e2e 在无真实 vite/webpack/go 工具链的沙箱内会失败，需在 CI 环境复跑确认。

---

## 第四部分：当前测试文件结构

```
packages/distribution/cli/rust/lania-cli/tests/phase3_e2e/
├── common.rs                          # 共享夹具（按 launcher 语义路由子包二进制）
├── help_and_config.rs                 # help/version/config/国际化
├── build_and_lint.rs                  # dev/build/lint
├── create_and_add.rs                  # create/add 工作流
├── distribution_and_dynamic.rs        # product CLI + 动态命令(installed)
├── dynamic_commands_host_rpc.rs       # 动态命令 dev 模式 + Host RPC (unix only)
├── git_and_release.rs                 # sync/release 工作流
├── template_commands.rs               # template list/view/交互式
├── tools_commands.rs                  # tools list/view/help
├── tools_run.rs                       # tools run 执行链路
├── env_commands.rs                    # env list/info/doctor/install dry-run
├── product_dev_watch.rs               # product dev watch
├── conflict_and_force.rs              # 文件冲突与 force 覆盖
├── error_scenarios.rs                 # 错误场景
├── i18n_runtime.rs                    # 国际化运行时行为
├── p3_remaining.rs                    # P3 收尾场景
├── phase3_benchmark.rs                # 性能基准
```

TS 侧测试随各包分发（`pnpm -r test`），如 `packages/foundation/bridge-runtime/ts/src/entry/index.test.ts` 与各切片包内的 `*.test.ts`。

---

## 第五部分：关于 Windows 兼容性

当前 E2E 测试套件存在以下 Windows 特定问题：

### 5.1 `unix_fs` 符号链接依赖

`common.rs` 中的 `copy_dir_recursive` 函数使用了 `std::os::unix::fs`，在 Windows 上编译时会产生未使用变量警告，但有 `#[cfg(unix)]` 保护，不阻止编译。

### 5.2 Node bridge 子进程路径解析（dynamic_commands_host_rpc 测试失败的根本原因）

Node bridge 的启动入口在 Windows 上通过 `LANIA_NODE_BRIDGE_DIR` 环境变量定位。已知问题：

- `import.meta.dirname` 在 Windows 上的路径格式可能与 POSIX 不同（`\` vs `/`）
- node_bridge 子进程的工作目录可能与 Rust 主进程不同
- `--import tsx` 的 loader 路径解析在 Windows 上可能因路径分隔符问题失败
- `process.stdin` 的 JSONL 协议输入在 Windows pipe 模式下行为与 Unix 有差异
- Windows 上子进程 `stdout` 编码/换行符 (`\r\n`) 可能干扰 jsonl 解析

> **当前修复方案**：`dynamic_commands_host_rpc.rs` 中的测试统一添加 `#[cfg(unix)]` 编译门控，在 Windows 上编译期跳过。这些测试依赖完整的 Node bridge 子进程基础设施（stdio 协议握手、Host RPC 双向通信、tsx loader），该基础设施的 Windows 兼容性属于 P3-3 长期改进项，需在后续统一修复。

### 5.3 TS 构建产物差异

`pnpm build` 在 Windows 上构建的 JS 产物与 macOS/Linux 相同，但 `script` 命令（用于交互式测试 `run_cli_interactive`）是 unix-only。

### 5.4 临时目录路径

`temp_dir` 使用 `std::env::temp_dir()`，Windows 上的临时目录路径可能包含空格或 `~` 短路径，影响 Node 模块解析。

### 5.5 Bash 通道与 git 命令

在本机沙箱环境中，Bash 通道执行 git 命令偶发因 PortableGit msys 冲突挂死；换用 PowerShell 通道可复现通过。这不影响测试逻辑本身，属于本地环境问题（详见 `../operations/known-issues.zh-CN.md`）。

---

## 第六部分：执行顺序建议

| 阶段                 | 任务                                                  | 状态      | 前置条件      |
| -------------------- | ----------------------------------------------------- | --------- | ------------- |
| **阶段 1（已完成）** | 动态命令 Host RPC + tools + env + 冲突/错误/i18n 场景 | ✅ 完成   | —             |
| **阶段 2（已完成）** | product dev watch + tools run + P3 收尾               | ✅ 完成   | 无            |
| **阶段 3**           | P1-1(pi E2E) + P1-2(schema parse err)                 | 📋 待开始 | 无            |
| **阶段 4**           | P2-1(prompt) + P2-2(launcher 安装态)                  | 📋 待开始 | 无            |
| **阶段 5**           | P3-1(git) + P3-2(task/progress) + P3-3(Windows)       | 📋 待开始 | 阶段 1-4 完成 |

---

## 附录：快速命令速查

```bash
# 运行所有测试（TS 各包 + Rust workspace）
pnpm test

# 仅 Rust 测试（标准）
pnpm test:rust:std

# 仅 Rust 测试（nextest）
pnpm test:rust

# 仅 TypeScript 测试（各包）
pnpm -r test

# 运行 tools 命令测试（全平台）
cargo test --manifest-path Cargo.toml --package lania-cli --test phase3_e2e tools_commands

# 运行动态命令 Host RPC 测试（unix only）
cargo test --manifest-path Cargo.toml --package lania-cli --test phase3_e2e dynamic_commands_host_rpc

# 检查测试覆盖范围
cargo llvm-cov --manifest-path Cargo.toml --workspace --lcov --output-path coverage/rust/lcov.info
```
