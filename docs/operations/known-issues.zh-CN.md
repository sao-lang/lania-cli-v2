# 已知测试失败与遗留问题（待处理）

本文档记录运行全量测试时**与本功能无关、但会阻塞全绿**的既有失败，以及当前已知的环境性限制，供后续单独处理。

环境：win32、Node v24、pnpm 10、Rust workspace（根在仓库根目录）。

复现命令：

```bash
pnpm -r test
cargo test --manifest-path Cargo.toml --workspace
```

> 全量 TS 套件含 `product.publish` 长测试，单次约 6–8 分钟，可能超过默认命令超时。

---

## 1. `product.publish` 系列 4 个用例失败

- 文件：`packages/foundation/bridge-slices/product/ts/src/plugins/product.test.ts`
- 失败用例：
  - `product.publish creates a publish-ready npm package layout from packed output`
  - `product.publish stages linux-x64 bundle from explicit env binary source`
  - `product.publish stages linux-x64 bundle from platformBinariesDir auto-discovery`
  - `product.publish requires --yes for non-dry-run execution`
- 现象：`Error: npm pack failed in C:\Users\...\AppData\Local\Temp\...`，随后打印被打包内容清单；`requires --yes` 用例因前面的 `npm pack` 先失败，未走到 `/--yes/` 断言。
- 备注：同文件中 `product.publish can execute publish-manifest steps through npm publish --dry-run` 在非 POSIX 环境会被跳过（`npm publish dry-run test requires POSIX shell environment`）。
- 单测耗时约 95–110 秒/个，是全量套件超时的主要来源。
- 初步判断：Windows 下 `npm pack` / 平台二进制打包路径假设不成立，或临时目录/文件锁问题。
- 建议：在 Windows 上复现 `npm pack` 失败的最小命令；确认 staging 是否依赖 POSIX 权限/符号链接；必要时把平台相关用例标记为 `skip` 或改成可注入的 pack 实现。

## 2. `create_workflow_initializes_git_repository` 失败

- 文件：`packages/foundation/capabilities/rust/lania-workflows/src/tests/create_add.rs`
- 现象：`create workflow succeeds: dependency version lookup object missing version field`
- 初步判断：create 流程里的依赖版本查询返回结构缺少 `version` 字段，可能与测试环境/注册表 stub 或 fixture 形状变化有关。
- 建议：检查该测试对依赖查询的 mock 是否仍匹配当前 `package_manager`/依赖版本查询协议；尽量做成离线 hermetic 测试。

---

## 3. 沙箱内 build/lint 等 e2e 因工具链缺失失败

- 文件：`packages/distribution/cli/rust/lania-cli/tests/phase3_e2e/build_and_lint.rs` 等
- 现象：在无真实 vite/webpack/go 工具链的沙箱环境中，`build_and_lint` 等 e2e 用例失败。
- 处理：非代码回归，需在具备真实工具链的 CI 环境复跑确认。

## 4. Bash 通道 git 命令偶发挂死（本机环境）

- 现象：在本机沙箱中经 Bash 通道执行 git 命令时，偶发因 PortableGit msys 冲突而挂死；换用 PowerShell 通道可复现通过。
- 处理：环境问题，不影响测试逻辑本身；排查时优先用 PowerShell 通道复现。

## 5. 已修复：git push 因编译产物卡住

- 历史：git push 曾因误提交约 300MB 编译产物而卡住。
- 处理：已修复 `.gitignore` 并做历史重写；忽略规则为 `packages/**/npm/cmd-*-<platform>/bin/*` 与 `cli-<platform>/bin/*`。如再遇到大产物入库，先检查这两条规则是否被破坏。

---

## 6. 2026-11-20 复核：本轮修复与结论

> 用 Docker（`rust:latest`，含 gcc 14）解决本机缺 gcc 的编译阻塞后，对"测试未通过"做了系统复核。
> 结论：一部分是**真实代码/测试 bug（已修）**，一部分是**环境性失败（非代码回归）**，另有多条历史记录**已不复现**。

### 6.1 本轮已修复的真实 bug

1. **`lania-command-config` 测试自死锁（阻塞 6 个测试）** — 已修
   - 根因：`missing_or_invalid_preferences_yield_empty_overrides` 在**同一测试函数内两次调用 `HomeGuard::set`**，`std::sync::Mutex` 不可重入 → 自死锁；该测试持锁不放，连锁阻塞所有用 `HomeGuard` 的测试。
   - 不是 Windows 专属：Docker Linux 同样复现（挂起 2 个）。
   - 修复：拆成 `invalid_preferences_yield_empty_overrides` 与 `missing_preferences_yield_empty_overrides` 两个独立测试，各自只 set 一次。
   - 验证：Docker `cargo test -p lania-command-config --lib` → **12 passed / 0 failed / 0 挂起**。

2. **`lania-node-bridge` 的 `falls_back_to_tsx_in_development_mode` 测试过时** — 已修
   - 表层根因：实现要求 dev_source 模式下 `package_dir/../../templates/src/index.ts` 存在（纯源码 checkout 布局），但测试只创建了 `src/entry/stdio.ts`，导致落到 "bridge assets not found"。
   - **深层根因（复审发现并修）**：实现里的 `../templates` 路径写错了一层——开发态 `package_dir` 是 `packages/foundation/bridge-runtime/ts`，`../templates` 指向不存在的 `bridge-runtime/templates`，真实 templates 在 `packages/foundation/templates`（应为 `../../templates`）。这导致 dev_source 模式在真实 checkout 下**永远不可达**；最初的测试修复只是"按错误实现造夹具"的假绿。
   - 修复：`process.rs` 路径改为 `../../templates/src/index.ts`，错误信息补齐双条件；测试夹具同步为 `parent/bridge-runtime/ts` + `parent/templates` 复刻真实布局，并清理 parent。
   - 验证：本机 `cargo test -p lania-node-bridge --lib` → **20 passed / 0 failed**。

### 6.2 环境性失败（非代码回归，无需改代码）

- **Docker `rust:latest` 缺 Node/Python/Go/Dart**：`lania-bridge-host/stdio_endpoints` 的 node/python endpoint 用例报 `No such file or directory`；`lania-node-bridge` 的几个 transport 用例、`phase3_e2e` 的 build/lint 用例同理。本机具备全部工具链时：`stdio_endpoints` **4 passed / 1 ignored**（ignored 需本地 Go/Dart SDK）、`lania-node-bridge` **20/20**。
- **全量 `cargo test --workspace` 无法在本机跑完**：`runex-core` 依赖的 C build script 需 gcc（本机缺，Docker 有）；而 Docker 缺 JS/其他运行时。需一套同时具备 Rust+gcc+Node+Python+Go+Dart 的 CI 环境才能全绿。

### 6.3 历史记录中已不复现（本机 2026-11-20）

- **§1 `product.publish` 4 例**：本机 `pnpm --filter @lania-cli-v2/bridge-slice-product test` → **14 passed / 0 failed / 1 skipped**，不再复现。原失败疑为临时 `npm pack` / 文件锁问题。
- **§2 `create_workflow_initializes_git_repository`**：本机 `cargo test -p lania-workflows --lib create_workflow_initializes_git_repository` → **ok**（耗时约 16s，真实访问 npm registry）。属网络依赖测试，可访问 registry 时正常；原失败疑为当时 registry stub / 网络问题。
- 这两条保留原文作为历史，状态以本节为准。

### 6.4 仍待处理

- ~~`runex` 的 `from_cache` 回归测试待补~~ **已补（2026-11-20）**：install() 里把 `from_cache` 从命令式累加改为由 `cache_hits` + 纯函数 `all_from_cache()` 汇总；新增 3 个纯函数测试 + 1 个端到端测试（本地 HTTP server + `RUNEX_NODE_MIRROR`，真实跑 install 两次验证下载→false / 命中缓存→true）。已 Verify RED（注入旧 bug 后测试失败）。Docker `cargo test -p runex-core --lib` 87/87 全绿。详见 overview review6/review7。
- web dev 长任务边角（rollup watch 首轮快照语义、webpack error 文案 string 保真度）：Nit 级，不影响功能。

## 7. 多语言运行时功能的已知边界（非缺陷）

以下为设计内行为，记录以免误判：

- `lan dev` / `lan build` 已实时流式转发启动与运行期日志，统一为 compiler 事件格式（`▶ start` / `• status` / `✖ issue` / `✔ done`）。
- Python 无通用 dev 命令，必须显式配置 `lan.config.runtimes.python.dev`，否则直接失败。
- `lan build --watch` 不支持非 web runtime，会明确报错。
- 非 web 构建失败以非零退出码返回；工具链缺失时 fail-fast，不伪造 dev server。
- POSIX 下 `stop` 通过独立进程组终止子进程树；Windows 下使用 `taskkill /PID <pid> /T /F`。
