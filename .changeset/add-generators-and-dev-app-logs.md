---
'@lania-cli-v2/cli': minor
'@lania-cli-v2/node-bridge': minor
---

`add` 生成器重构 + dev 运行期日志实时透出（设计文档 B5 / B6）。

- **`lan add` 变成框架感知的生成器**：`lan add resource users` 在 NestJS 项目里一次生成
  module / controller / service / create-dto / update-dto / entity / spec 共 7 个文件，
  并在 `src/app.module.ts` 的 `// <lania:imports>` 锚点后追加 `UsersModule` 导入。
- **生成器表只有一处定义**：`lania-workflows/src/create/add/generators.rs`；
  TS 侧通过 bridge 方法 `addGenerator.list`（宿主 host RPC）读取，不再维护平铺清单。
- **命名推导**：`users` → `Users` / `user`，`user-profile` → `UserProfile` / `userProfile` /
  `user_profile`；内置 `-s` / `-es` / `-ies` / 不规则表单复数规则，不引入新依赖。
- **框架探测**：`lan.config` 的 `add.framework` → 特征文件（`nest-cli.json` / `next.config.*`）
  → 回退到通用生成器。**探测失败不猜框架**，只暴露 `component` / `config`。
- **`lan add --list`**：按框架列出可用生成器；无框架项目只列通用生成器。
- **冲突处理**：`--force` 直接覆盖；交互式终端逐条询问「覆盖 / 跳过 / 重命名」
  （重命名会给出 `Button-1.tsx` 这类不冲突的候选路径）；非交互环境保持
  「跳过 + 计入 `conflicts`」，不会阻塞等待 stdin。
- **兼容**：旧的平铺模板名（`rfc` / `v3` / `eslint` …）继续可用，映射为
  `component` / `config` 生成器的 `variant`；`--template` 与 `--filepath` 保持兼容。
  新增位置参数 `lan add <generator> <name>`，`--name` 的既有用法不变。
- **`lan dev` 运行期日志实时透出**：compiler worker 的 stdout/stderr 改为逐行
  `event.diag`（`channel: "build"`），并在流式模式下立刻发出而不等停止；
  服务端就绪后的应用日志保持 `channel: "app"`，渲染为 `[app]` 前缀，与 `[build]` / `[dev]` 分离。
- **新增开关**：`ui.dev.quietApp: true` 或 `lan dev --quiet-app` 不再显示应用自身日志。
- **stderr 不再一律当 warning**：worker stderr 按内容推断 level，info 级不再额外造
  `event.compiler_issue`。
