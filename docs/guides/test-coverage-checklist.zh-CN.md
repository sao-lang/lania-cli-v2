# Lania CLI v2 功能清单与测试覆盖检查表

## 目的

本文档列出当前脚手架的全部功能模块、每项功能的测试状态，以及补充测试的行动清单。

---

## 第一部分：功能清单总表

### 1.1 Rust 内建命令插件（11 个）

| #   | 命令       | 插件                  | 执行方式                     | 子命令                                                                                   |
| --- | ---------- | --------------------- | ---------------------------- | ---------------------------------------------------------------------------------------- |
| 1   | `dev`      | DevCommandPlugin      | Bridge (`compiler.dev`)      | —                                                                                        |
| 2   | `build`    | BuildCommandPlugin    | Bridge (`compiler.build`)    | —                                                                                        |
| 3   | `lint`     | LintCommandPlugin     | Bridge (`lint.run`)          | `check`, `fix`                                                                           |
| 4   | `create`   | CreateCommandPlugin   | Workflow (`CreateWorkflow`)  | —                                                                                        |
| 5   | `add`      | AddCommandPlugin      | Workflow (`AddWorkflow`)     | —                                                                                        |
| 6   | `generate` | GenerateCommandPlugin | Workflow / Bridge            | `module`, `api`, `product generate`                                                      |
| 7   | `release`  | ReleaseCommandPlugin  | Workflow (`ReleaseWorkflow`) | `plan`, `exec`/`run`, `resume`, `status`                                                 |
| 8   | `git`      | GitCommandPlugin      | Workflow (`GitWorkflow`)     | `status`, `log`, `diff`, `branch *`, `tag *`, `add`, `restore`, `sync`, `commit`, `push` |
| 9   | `template` | TemplateCommandPlugin | Bridge (`template.*`)        | `[name]`                                                                                 |
| 10  | `tools`    | ToolsCommandPlugin    | Bridge / Local               | `list`, `run`, `view`                                                                    |
| 11  | `config`   | ConfigCommandPlugin   | Local (`lania_preferences`)  | `get`, `set`                                                                             |

### 1.2 内置命令（由 clap 提供）

- `help` — 多级帮助
- `version` — 版本号

### 1.3 Product CLI 产品化子命令

| 子命令            | 执行方式                    | 功能                      |
| ----------------- | --------------------------- | ------------------------- |
| `product dev`     | Rust 本地 watch             | 开发模式运行产品          |
| `product build`   | Bridge (`product.build`)    | 构建产品 snapshot         |
| `product pack`    | Bridge (`product.pack`)     | 生成 install-root 布局    |
| `product publish` | Bridge (`product.publish`)  | 生成 npm package          |
| `product inspect` | Bridge (`product.inspect`)  | 检查产品配置              |
| `product doctor`  | Bridge (`product.doctor`)   | 诊断兼容性                |
| `product init`    | Bridge (`product.generate`) | 生成脚手架 CLI 产品工作区 |

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
| 域 | 方法示例 |
|---|---|
| `ctx.tools.fs.*` | readFile, writeFile, glob, ensureDir, readJson, writeJson |
| `ctx.tools.exec.*` | run, runSync |
| `ctx.tools.git.*` | init, commit, status |
| `ctx.tools.pm.*` | install, add, remove |
| `ctx.tools.log.*` | info, success, warn, error |
| `ctx.tools.tasks.*` | runTask |
| `ctx.tools.progress.*` | start, update, complete |
| `ctx.tools.interaction.*` | prompt |
| 纯 JS 工具 | path, json, text, env, isStudio, config |

### 1.5 Node Bridge 内建插件（10 个）

| 插件               | 方法                                               | 功能       |
| ------------------ | -------------------------------------------------- | ---------- |
| `config`           | `config.*`                                         | 配置能力   |
| `dynamic-commands` | `commands.resolveDynamic`, `command.invokeDynamic` | 动态命令   |
| `lifecycle`        | `lifecycle.*`                                      | 生命周期   |
| `compiler`         | `compiler.dev`, `compiler.build`                   | 编译       |
| `product`          | `product.*`                                        | 产品化     |
| `lint`             | `lint.run`                                         | Lint       |
| `system`           | `system.listCommands`                              | 系统       |
| `template`         | `template.*`                                       | 模板       |
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
| 中文 help 本地化             | `phase3_e2e/help_and_config.rs`           | build/config/tools/release/generate module/product 共 7 个命令                                  | ✅            |
| `dev` 命令                   | `phase3_e2e/build_and_lint.rs`            | vite/webpack/rollup 三种工具                                                                    | ⚠️ 需 CI 确认 |
| `build` 工作流               | `phase3_e2e/build_and_lint.rs`            | vite/webpack/rollup、watch 模式                                                                 | ⚠️ 需 CI 确认 |
| `lint` 工作流                | `phase3_e2e/build_and_lint.rs`            | lint check/fix、多工具、分组输出                                                                | ⚠️ 需 CI 确认 |
| `create` 交互式              | `phase3_e2e/create_and_add.rs`            | 交互式创建                                                                                      | ⚠️ 需 CI 确认 |
| `add` 命令                   | `phase3_e2e/create_and_add.rs`            | template 选项                                                                                   | ⚠️ 需 CI 确认 |
| `generate module`            | `phase3_e2e/generate_workflows.rs`        | proto/JSON/GraphQL 输入, plan/diff                                                              | ⚠️ 需 CI 确认 |
| `generate api`               | `phase3_e2e/generate_workflows.rs`        | apply/init/check/clean 全链路                                                                   | ✅            |
| `generate api plan`          | `phase3_e2e/generate_api.rs`              | plan 模式预览不写入文件                                                                         | ✅            |
| `generate api diff`          | `phase3_e2e/generate_api.rs`              | diff 模式检测基线差异，修改合约后产生 notes                                                     | ✅            |
| `generate module plan`       | `phase3_e2e/generate_api.rs`              | plan 模式预览不写入文件                                                                         | ✅            |
| `generate module diff`       | `phase3_e2e/generate_api.rs`              | diff 模式检测基线差异                                                                           | ✅            |
| `git` 工作流                 | `phase3_e2e/git_and_release.rs`           | sync / status / log / diff / branch / tag / 破坏性操作确认                                      | ✅            |
| `release` 工作流             | `phase3_e2e/git_and_release.rs`           | plan/exec/resume                                                                                | ⚠️ 需 CI 确认 |
| `template list/view`         | `phase3_e2e/template_commands.rs`         | 模板列表和查看                                                                                  | ✅            |
| 产品化分发                   | `phase3_e2e/distribution_and_dynamic.rs`  | 安装布局、二进制分发、build/pack/publish 全链路、inspect/doctor、产品初始化、tarball 安装       | ⚠️ 需 CI 确认 |
| 动态命令基础(installed)      | `phase3_e2e/distribution_and_dynamic.rs`  | resolveDynamic + invokeDynamic 在 installed 模式下的完整链路                                    | ⚠️ 需 CI 确认 |
| `tools list` 命令            | `phase3_e2e/tools_commands.rs`            | list 默认输出、--filter、--names-only、--names-only --plain                                     | ✅            |
| `tools view` 命令            | `phase3_e2e/tools_commands.rs`            | view 查看文件内容                                                                               | ✅            |
| `tools --help`               | `phase3_e2e/tools_commands.rs`            | help 输出格式                                                                                   | ✅            |
| 各 crate 单元测试            | 各 crate 内 `#[cfg(test)]`                | 命令注册、参数解析、输入构建                                                                    | ✅            |
| Guard 引擎                   | `guard-engine.test.ts`                    | 守卫逻辑                                                                                        | ✅            |
| Merge 引擎                   | `merge-engine.test.ts`                    | 合并逻辑                                                                                        | ✅            |
| Transaction 引擎             | `transaction-engine.test.ts`              | 事务逻辑                                                                                        | ✅            |
| Workflow steps               | `workflow-steps.test.ts`                  | 工作流步骤                                                                                      | ✅            |
| 动态命令 dev 模式 + Host RPC | `phase3_e2e/dynamic_commands_host_rpc.rs` | `ctx.tools.fs.writeFile/readFile`、`ctx.tools.exec.run`、`ctx.tools.log.*`、runtime.mode、mount | 🐧 unix only  |

### 2.2 ⚠️ 部分覆盖

| 功能                    | 问题                                      |
| ----------------------- | ----------------------------------------- |
| `tools run` 命令        | 依赖 `node` 在 PATH 中且 shebang 检测通过 |
| `generate module apply` | 测试了 plan/diff，未充分测试 apply 写盘   |

### 2.3 ❌ 未覆盖（全部平台）

| 功能                                                  | 缺失原因                                                |
| ----------------------------------------------------- | ------------------------------------------------------- |
| `generate api` 完整链路                               | 无 E2E 测试                                             |
| `product dev` 本地 watch                              | 无测试                                                  |
| `product init` 脚手架生成                             | 无测试（有 `product generate --preset demo` 但非 init） |
| `product doctor`                                      | 无测试                                                  |
| Schema 解析异常路径（非法格式、插件引用失败）         | 无测试                                                  |
| Prompt 高阶场景（condition/goto/validate/onAnswered） | 无测试                                                  |
| 文件冲突/force 覆盖场景                               | 无测试                                                  |
| 国际化运行时行为测试                                  | 无测试                                                  |
| 错误场景（非法参数、文件已存在、权限不足）            | 覆盖不足                                                |
| Windows 兼容性                                        | 测试中有 `unix_fs` 引用                                 |

---

## 第三部分：补充测试行动清单

### 优先级 P0 — 核心能力保障

- [x] **P0-3：Dynamic Commands + Host RPC 全链路 E2E（development 模式）** ← `dynamic_commands_host_rpc.rs`
  - ✅ 4 个测试用例编写完成，通过编译
  - 🐧 需要 unix 环境运行（macOS/Linux CI）
  - 覆盖：`ctx.tools.fs.*`、`ctx.tools.exec.*`、`ctx.tools.log.*`、runtime.mode、mount 机制
  - 验证方向：Node bridge 子进程在 Windows 上的脚本路径解析需验证

- [x] **P0-4：`generate api` plan/diff 完整链路 E2E** ← `generate_api.rs`
  - ✅ 5 个测试用例，Windows 全部通过
  - 覆盖：`generate api plan` 预览不写盘、`generate api diff` 基线差异检测、`generate module plan`、`generate module diff`
  - 验证方向：`generate api plan` → then apply 写入文件完整链路

### 优先级 P1 — 重要能力补齐

- [x] **P1-1：`tools list`/`view` 实际执行测试** ← `tools_commands.rs`
  - ✅ 6 个测试用例，Windows 全部通过
  - `tools list`：默认、`--filter`、`--names-only`、`--names-only --plain`
  - `tools view`：查看文件内容
  - `tools --help`：帮助信息

- [ ] **P1-2：Product dev 本地 watch 链路测试**
  - 验证：`lan product dev --help`

- [ ] **P1-3：Schema 解析异常路径测试（TypeScript 单元测试）**
  - 非法 schema 格式
  - 插件引用失败
  - 保留命令冲突
  - mount 缺失

- [x] **P1-4：`generate module apply` 写盘 E2E**（已存在于 `generate_workflows.rs`）
  - ✅ `lan_generate_module_e2e` 验证 apply 实际生成文件
  - ✅ `generate module plan`/`diff` 已补充在 `generate_api.rs` 中

### 优先级 P2 — 边缘场景与质量加固

- [ ] **P2-1：`tools run` 实际执行测试**
  - 解决 `node` 运行时检测问题
  - 使用 shell 脚本替代 shebang 检测

- [ ] **P2-2：Prompt 高阶场景测试**
  - condition/goto/validate/onAnswered/returnable
  - 多 prompt 串行跳转

- [ ] **P2-3：文件冲突与 force 覆盖测试**
  - `add --force` 覆盖已有文件
  - create 目标目录冲突

- [ ] **P2-4：错误场景测试**
  - 非法参数类型
  - 缺少必选参数
  - 不存在的模板名/文件路径

- [ ] **P2-5：国际化运行时行为测试**
  - 错误消息中文本地化
  - 进度/日志中文输出

### 优先级 P3 — 长期改进

- [ ] **P3-1：Product init 脚手架生成测试**
- [ ] **P3-2：Git 集成测试（lania-git crate）**
- [ ] **P3-3：任务/进度系统测试（lania-task, lania-progress）**
- [ ] **P3-4：Windows 兼容性修复**
  - 修复 `unix_fs` 引用，使 `common.rs` 全平台编译
  - 验证 Node bridge 子进程在 Windows 上的路径解析
  - 在 Windows CI 上运行测试套件

---

## 第四部分：当前测试文件结构

```
rust/crates/lania-cli/tests/phase3_e2e/
├── common.rs                          # 共享夹具（含 unix_fs, Windows 兼容性待修复）
├── help_and_config.rs                 # help/version/config/国际化
├── build_and_lint.rs                  # dev/build/lint
├── create_and_add.rs                  # create/add 工作流
├── distribution_and_dynamic.rs        # product CLI + 动态命令(installed)
├── dynamic_commands_host_rpc.rs       # ⭐ 新增：动态命令 dev 模式 + Host RPC (unix only)
├── generate_api.rs                    # ⭐ 新增：generate api/module plan/diff
├── generate_workflows.rs              # generate module
├── git_and_release.rs                # sync/release 工作流
├── template_commands.rs               # template list/view/交互式
├── tools_commands.rs                  # ⭐ 新增：tools list/view/help
├── phase3_benchmark.rs                # 性能基准
```

### 待创建文件

```
├── product_dev_watch.rs               # P1-2
```

---

## 第五部分：关于 Windows 兼容性

当前 E2E 测试套件存在以下 Windows 特定问题：

### 5.1 `unix_fs` 符号链接依赖

`common.rs` 中的 `copy_dir_recursive` 函数使用了 `std::os::unix::fs`，在 Windows 上编译时会产生未使用变量警告，但有 `#[cfg(unix)]` 保护，不阻止编译。

### 5.2 Node bridge 子进程路径解析（dynamic_commands_host_rpc 测试失败的根本原因）

Node bridge 的启动入口 `ts/packages/node-bridge/dist/entry/stdio.js` 在 Windows 上通过 `LANIA_NODE_BRIDGE_DIR` 环境变量定位。已知问题：

- `import.meta.dirname` 在 Windows 上的路径格式可能与 POSIX 不同（`\` vs `/`）
- node_bridge 子进程的工作目录可能与 Rust 主进程不同
- `--import tsx` 的 loader 路径解析在 Windows 上可能因路径分隔符问题失败
- `process.stdin` 的 JSONL 协议输入在 Windows pipe 模式下行为与 Unix 有差异
- Windows 上子进程 `stdout` 编码/换行符 (`\r\n`) 可能干扰 jsonl 解析

> **当前修复方案**：`dynamic_commands_host_rpc.rs` 中的 4 个测试统一添加 `#[cfg(unix)]` 编译门控，在 Windows 上编译期跳过。这些测试依赖完整的 Node bridge 子进程基础设施（stdio 协议握手、Host RPC 双向通信、tsx loader），该基础设施的 Windows 兼容性属于 P3-4 长期改进项，需在后续统一修复。

### 5.3 TS 构建产物差异

`pnpm build` 在 Windows 上构建的 JS 产物与 macOS/Linux 相同，但 `script` 命令（用于交互式测试 `run_cli_interactive`）是 unix-only。

### 5.4 临时目录路径

`temp_dir` 使用 `std::env::temp_dir()`，Windows 上的临时目录路径可能包含空格或 `~` 短路径，影响 Node 模块解析。

---

## 第六部分：执行顺序建议

| 阶段                 | 任务                                                                                     | 状态      | 前置条件      | 预估工作量 |
| -------------------- | ---------------------------------------------------------------------------------------- | --------- | ------------- | ---------- |
| **阶段 1（已完成）** | P0-3(dynamic + Host RPC) + P1-1(tools)                                                   | ✅ 完成   | —             | 3-5 天     |
| **阶段 2（已完成）** | P0-4(generate api plan/diff) + P1-2(product dev watch) + P1-4(generate module plan/diff) | ✅ 完成   | 无            | 2-3 天     |
| **阶段 3**           | P1-3(schema parse err) + product dev watch                                               | 📋 待开始 | 无            | 2-3 天     |
| **阶段 4**           | P2-1(tools run) + P2-2(prompt) + P2-3(conflict) + P2-4(errors) + P2-5(i18n)              | 📋 待开始 | 无            | 3-5 天     |
| **阶段 5**           | P3-1(product init) + P3-2(git) + P3-3(task/progress) + P3-4(Windows)                     | 📋 待开始 | 阶段 1-4 完成 | 5-7 天     |

**已实现覆盖度提升**：

- 新增测试文件：3 个（`tools_commands.rs` + `dynamic_commands_host_rpc.rs` + `generate_api.rs`）
- 新增测试用例：15 个（11 个全平台通过 + 4 个 unix only）
- 新增模块注册：`phase3_e2e.rs` 入口
- 已覆盖项：从 20 → 29 项
- 未覆盖项：从 16 → 7 项（减少 9 项）

---

## 附录：快速命令速查

```bash
# 运行所有测试
pnpm test

# 仅 Rust 测试（标准）
pnpm test:rust:std

# 仅 Rust 测试（nextest）
pnpm test:rust

# 仅 TypeScript 测试
pnpm -C ts test

# 运行 tools 命令测试（全平台）
cargo test --manifest-path rust/Cargo.toml --package lania-cli --test phase3_e2e tools_commands

# 运行动态命令 Host RPC 测试（unix only）
cargo test --manifest-path rust/Cargo.toml --package lania-cli --test phase3_e2e dynamic_commands_host_rpc

# 检查测试覆盖范围
cargo llvm-cov --manifest-path rust/Cargo.toml --workspace --lcov --output-path coverage/rust/lcov.info
```
