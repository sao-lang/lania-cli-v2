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

## 6. 多语言运行时功能的已知边界（非缺陷）

以下为设计内行为，记录以免误判：

- `lan dev` / `lan build` 已实时流式转发启动与运行期日志，统一为 compiler 事件格式（`▶ start` / `• status` / `✖ issue` / `✔ done`）。
- Python 无通用 dev 命令，必须显式配置 `lan.config.runtimes.python.dev`，否则直接失败。
- `lan build --watch` 不支持非 web runtime，会明确报错。
- 非 web 构建失败以非零退出码返回；工具链缺失时 fail-fast，不伪造 dev server。
- POSIX 下 `stop` 通过独立进程组终止子进程树；Windows 下使用 `taskkill /PID <pid> /T /F`。
