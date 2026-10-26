---
'@lania-cli-v2/cli': minor
'@lania-cli-v2/node-bridge': minor
---

多运行时 bridge 与 lint 结果统一（设计文档 B0–B4）。

- **工具链血缘**：`lan` 启动时把 `~/.runex/shims` 前置到自身 PATH（幂等、shims 不存在时跳过），
  因此 `lan env use <runtime> <ver>` 之后同一次 `lan build` / `lan lint` 会真正使用 managed 版本。
  `lan env doctor` 把「本进程 PATH 是否前置」与「shell profile 是否固化」拆成两条独立诊断。
- **运行时知识单点**：新增 `toolchain.describe` bridge 方法，特征文件、默认 build/dev 命令、
  lint 工具集在仓库内只有一处定义（`node-bridge/src/plugins/toolchain.ts`）。
  运行时判定顺序固定为 `deno.json` > `bun.lock(b)` > 非 JS 强信号 > `package.json`。
- **bridge 启动可参数化**：任意说同一套 jsonl 协议的程序都能被当作 bridge 启动（`BridgeLaunchSpec`），
  并新增 `BridgeRegistry` 骨架按运行时懒加载；未登记时明确报错供上层回落。
- **配置统一**：Rust 不再维护 `lan.config.*` 候选列表（改用 bridge 的 7 元素默认表），
  `lan.config.mts` / `.cts` 在两条路径下都能被找到；`LanConfigSnapshot` 新增强类型
  `runtime` / `runtimes`；配置加载失败与校验问题会以 warning 显示而不是静默降级；
  函数式配置现在能拿到 `{ env, cwd, command }`（`env` 来自 `LANIA_ENV`，缺省 `development`，不读 `NODE_ENV`）。
- **统一结果模型**：新增 `event.diag`（带 `source` / `stream` / `channel` / 跨流单调 `seq`），
  lint 的 8 个语言工具全部产出结构化 `Diagnostic`（行号、列号、规则号、消息、修复建议）。
  stderr 不再一律被当作 warning。
- **lint 渲染**：TTY 下按文件分组、列对齐、按 severity 高亮，末尾输出汇总与按工具计数；
  `NO_COLOR` / 非 TTY 时无 ANSI 转义。

`lan lint --json` 为**新增字段**（不破坏）：`result.diagnostics[]`（`tool` / `id` / `severity` /
`message` / `file` / `range` / `fix`）与 `result.counts`。旧字段 `summaryByAdaptor` /
`resultsByAdaptor` / `summaryText` 保留。
