# 已知测试失败与遗留问题（待处理）

本文档记录在 `master` 上运行全量测试时**与本功能无关、但会阻塞全绿**的既有失败，供后续单独处理。最后更新：2026-10-24，`master` @ `709eff5`。

环境：win32 10.0.26300、Node v24.12.0、pnpm 10.25.0、cargo workspace。

复现命令：

```bash
pnpm -C ts test
cargo test --manifest-path rust/Cargo.toml --workspace
```

> 全量 TS 套件含 `product.publish` 长测试，单次约 6–8 分钟，可能超过默认命令超时。

---

## 1. `contract.test.ts` handshake 未包含 `agent.*` 方法

- 文件：`ts/packages/node-bridge/src/plugins/contract.test.ts:16`
- 复现：在 `ts/packages/node-bridge` 下运行 `node --import tsx --test src/plugins/contract.test.ts`
- 现象：实际 methods 比期望多出
  - `agent.session.create`
  - `agent.prompt`
  - `agent.interrupt`
  - `agent.session.dispose`
  - `agent.models.list`
- 原因：提交 `8c7361f feat(agent): 新增 lan agent 命令与宿主工具审批` 增加了 bridge 方法，但未同步契约测试的期望数组。
- 建议：把这 5 个方法加入 `expected`；同时检查 `response.events` 是否也需要补 `agent_*` 事件。

## 2. `product.publish` 系列 4 个用例失败

- 文件：`ts/packages/node-bridge/src/plugins/product.test.ts`
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

## 3. `generate_module_workflow_supports_schema_embedded_transport_metadata` 失败

- 文件：`rust/crates/lania-workflows/src/tests/generate_module.rs:1084`
- 复现：`cargo test --manifest-path rust/Cargo.toml -p lania-workflows`
- 现象：
  ```
  assertion failed: thrift_http_register.contains("Get(\"/users/:id\", controller.GetUser)")
  ```
- 初步判断：thrift HTTP 注册代码生成结果与期望字符串不一致（生成器输出或断言过期）。
- 建议：打印实际 `thrift_http_register` 内容，确认是生成逻辑回归还是断言需要更新。

## 4. `create_workflow_initializes_git_repository` 失败

- 文件：`rust/crates/lania-workflows/src/tests/create_add.rs:215`
- 现象：`create workflow succeeds: dependency version lookup object missing version field`
- 初步判断：create 流程里的依赖版本查询返回结构缺少 `version` 字段，可能与测试环境/注册表 stub 或 fixture 形状变化有关。
- 建议：检查该测试对依赖查询的 mock 是否仍匹配当前 `package_manager`/依赖版本查询协议；尽量做成离线 hermetic 测试。

---

## 5. 多语言运行时功能的已知边界（非缺陷）

以下为设计内行为，记录以免误判：

- `lan dev` / `lan build` 已实时流式转发启动与运行期日志，统一为 compiler 事件格式（`▶ start` / `• status` / `✖ issue` / `✔ done`）。
- Python 无通用 dev 命令，必须显式配置 `lan.config.runtimes.python.dev`，否则直接失败。
- `lan build --watch` 不支持非 web runtime，会明确报错。
- 非 web 构建失败以非零退出码返回；工具链缺失时 fail-fast，不伪造 dev server。
- POSIX 下 `stop` 通过独立进程组终止子进程树；Windows 下使用 `taskkill /PID <pid> /T /F`。

多语言运行时自身的测试（`runtime-adapters`、`runtime-detect`、`compiler`、`template` 以及 Rust 相关 crate）在 `master` 上全部通过。
