# lan agent 技术方案

> 目标：在 `lan` 上提供与 omp 同量级能力的 coding agent，但底座与 omp 相反——
> **执行权威在 Rust 宿主，Node 只做模型编排**；长尾能力走插件协议而非内置。

## 0. 范围

覆盖：agent 循环 / provider / 工具面 / 终端 TUI / 会话 / 压缩 / 子 agent / hooks / skills / MCP / AST 编辑与搜索 / 记忆（文件后端）。

不覆盖（有意）：内置桌面控制、浏览器、IDA、图像、TTS、PDF、Apple FM、插件市场、ACP、collab。它们只保留插件注册协议。

> 配套文档：`self-managed-runtime-manager.zh-CN.md`（环境管理拆包）、
> `multi-runtime-bridge.zh-CN.md`（多运行时 bridge 与命令重构）。
> 本文 §3.1 定义 agent 的**包边界**，§4.7 定义**能力契约**。二者是本方案新增的施工依据。

## 1. 五条硬规则

1. **Node 无 I/O**：`ts/packages/agent/**` 禁止 `import fs | child_process | net | http`。工具只能经 `hostCall('agent.tool.*')`。CI 用 eslint `no-restricted-imports` 强制。
2. **授权先于执行且同进程**：`agent.tool.exec` 在 Rust 内完成「策略 → 审批 → 执行」，不允许 Node 先授权再调用。
3. **工具表封顶 13 个**：新增内置工具需说明「为什么插件做不到」。
4. **写操作全局单写者**：Rust 侧按工作区加 `writer` 锁，进程内唯一。
5. **会话即文件**：消息、压缩摘要、todo、审批缓存、子 agent 输出都在 `.lan/agent/` 下，可 diff、可删。

## 2. 总体架构

```
┌──────────────────────────── lan (Rust 单二进制) ────────────────────────────┐
│ lania-agent                                                                │
│   ├─ TUI        ratatui 0.28 + crossterm 0.29（command-env 在用，需提升）   │
│   ├─ Session    .lan/agent/sessions/<id>.jsonl（append-only + leaf 指针）   │
│   ├─ Approval   tier(read|write|exec) + policy + grant 缓存                 │
│   ├─ Tools      lania-agent-tools：fs/edit/grep/glob/ast/git/exec/task/mcp  │
│   └─ Memory     .lan/agent/memory/{MEMORY.md,learned.md}                   │
│                                                                            │
│ lania-node-bridge  ◄── jsonl：request / event* / response / host_request ──┐│
└────────────────────────────────────────────────────────────────────────────┘│
                                     │                                       │
┌────────────────────────────────────▼──── ts/packages/agent ────────────────┐
│ kernel   @earendil-works/pi-agent-core 1.4.0   Agent × N（主 + 子）         │
│ provider @earendil-works/pi-ai 1.4.0           Models / streamSimple       │
│ adapter  typebox schema → AgentTool → hostCall('agent.tool.exec')          │
└────────────────────────────────────────────────────────────────────────────┘
```

进程与线程：

- 单 `lan` 进程；bridge 是既有子进程（`node --import tsx ...`），Node ≥ 22.19（pi 要求）。
- TUI 事件循环与 bridge 读取在 Rust 侧用 `tokio::select!` 合流：crossterm 事件 + agent 事件 + 定时 tick。
- 子 agent 不新起进程：Node 内多个 `Agent` 实例，各自独立 messages；会话、并发、写锁归 Rust。

## 3. 模块清单

| 模块                    | 位置     | 职责                                                        | 对外面                 |
| ----------------------- | -------- | ----------------------------------------------------------- | ---------------------- |
| `lania-agent`           | 新 crate | 命令入口、TUI、会话、审批、压缩、子 agent 调度              | `agent` 命令           |
| `lania-agent-tools`     | 新 crate | 13 个内置工具的 Rust 实现（含 AST/搜索/diff）               | `ToolRegistry::invoke` |
| `lania-agent-protocol`  | 新 crate | agent 事件、工具请求/响应的序列化模型（Rust 侧）            | 供 host 与 cli 共用    |
| `lania-natives`（可选） | 新 crate | **只在实测需要时**出现的 N-API addon                        | 不进入 v1              |
| `@lania-cli/agent`      | 新 TS 包 | pi 循环、provider、工具 schema 适配、MCP 之外的一切“纯编排” | `runAgent`             |

复用既有：`lania-host`（插件/命令/钩子/输出）、`lania-node-bridge`（协议与 stream）、`lania-task`（并发/取消/重试）、`lania-fs`、`lania-git`、`lania-exec`、`lania-prompt`、`lania-hooks`、`lania-logger`。

### 3.1 包边界（拆包决策）

现状：`lania-agent`（363 行、单文件 `lib.rs`）把**命令入口**与**会话能力**混装，且能力层直接依赖 `lania-host`。
后果：任何非 lania 宿主（GUI / IDE 插件 / CI 脚本）想复用「agent 会话」都不得不连带整套插件框架。

拆成两个 crate：

```text
lania-agent-core                      ← 新增：能力层，禁止依赖 lania-host / lania-command
├── Cargo.toml            deps: anyhow, async-trait, serde_json, lania-node-bridge
└── src/
    ├── lib.rs            re-export
    ├── spec.rs           SessionSpec / PromptRequest / ApprovalMode
    ├── event.rs          AgentEvent（与 BridgeEventMethod 一一映射）
    ├── client.rs         trait AgentSessionClient
    └── bridge.rs         BridgeAgentSessionClient（基于 lania-node-bridge 的默认实现）

lania-agent                           ← 瘦身：只剩命令插件与终端渲染
├── Cargo.toml            deps: + lania-agent-core, lania-host, lania-command
└── src/
    ├── lib.rs            AgentCommandPlugin / spec / Plugin impl
    ├── handler.rs        execute：argv → SessionSpec → core client
    └── render.rs         REPL、事件渲染、compact_json
```

依赖方向（单向，无环）：

```text
lania-cli ──▶ lania-agent ──▶ lania-agent-core ──▶ lania-node-bridge
                   └──▶ lania-host ──▶ lania-node-bridge
```

**边界判定标准**（可作为 code review checklist）：

| 判定项                                                | 归属               |
| ----------------------------------------------------- | ------------------ |
| `CommandSpec` / `Plugin` / `register_builtin_command` | `lania-agent`      |
| argv 解析（`option_string` / `positional_prompt`）    | `lania-agent`      |
| `eprintln!` / `print!` / `io::stdout().flush()`       | `lania-agent`      |
| bridge 方法名与参数拼装（`agent.session.create` 等）  | `lania-agent-core` |
| 事件结构（`AgentEvent`）与 `BridgeEventMethod` 的映射 | `lania-agent-core` |
| 终端颜色、`/help` 文案、进度提示                      | `lania-agent`      |

**验收标准**：

- [ ] `cargo tree -p lania-agent-core` 中不出现 `lania-host` / `lania-command`。
- [ ] `agent` 命令行为零变化：`--approval-mode`、`/help`、`/model`、`/quit`、事件渲染格式逐字节一致。
- [ ] `lania-agent-core` 的 `AgentSessionClient` 可被单测 mock（不启动 node 进程）。

### 3.2 与 TS 包的对应关系

| Rust                                   | TypeScript                         | 职责                     |
| -------------------------------------- | ---------------------------------- | ------------------------ |
| `lania-agent-core::AgentSessionClient` | `@lania-cli-v2/agent`（`kernel/`） | 会话与模型循环的**边界** |
| `lania-agent`（命令插件）              | —                                  | 仅终端交互               |
| `lania-node-bridge`（传输）            | `@lania-cli-v2/node-bridge`        | 协议与进程管理           |

⚠️ 跨语言边界只有一处：`agent.*` 5 个 bridge 方法与 `event.agent_*` 9 个事件。
两侧的 Rust 结构体与 TS 类型必须逐字段对齐（见 §4.2、§4.3），改动需同 PR 完成。

当前 TS 侧的耦合问题：`ts/packages/node-bridge/src/core/plugin-registry.ts` 在**模块顶层静态 import** `@lania-cli-v2/agent`，且 `node-bridge` 把它写在 `dependencies`。这让「装 CLI 必须拉 pi 依赖」。本方案不强制改（agent 是 CLI 的一等功能），但**在 `lania-agent-core` 落地后应改为动态 import**，使 bridge 可独立运行。

## 4. 关键接口

### 4.1 Bridge 方法（Rust → Node）

| 方法                   | 参数                                                                     | 返回                                             |
| ---------------------- | ------------------------------------------------------------------------ | ------------------------------------------------ |
| `agent.session.create` | `{sessionId, model, thinking, systemPrompt, messages, tools, maxTokens}` | `{sessionId, model, contextWindow}`              |
| `agent.prompt`         | `{sessionId, prompt, images?}`                                           | 流式事件 + `{message, usage, stopReason}`        |
| `agent.interrupt`      | `{sessionId}`                                                            | `{ok}`                                           |
| `agent.state`          | `{sessionId}`                                                            | `{isStreaming, pendingToolCalls, contextTokens}` |
| `agent.models.list`    | `{provider?}`                                                            | `[{provider, id, contextWindow, reasoning}]`     |

会话历史由 Rust 持有：`session.create` 时把 JSONL 还原成 `messages` 一次性传入，Node 不落盘。

### 4.2 事件协议（新增，Node → Rust，逐条即时写出）

现行 `handleExchange()` 在完成前不写 envelope（`ts/packages/node-bridge/src/entry/stdio.ts`），需要改为 `handleExchange(request, emit)`，`emit` 直接写 `{type:'event', requestId, payload}`。

`BridgeEventMethod` 封闭枚举新增：

```
event.agent_start          {sessionId, model}
event.agent_text_delta     {sessionId, messageId, delta}
event.agent_thinking_delta {sessionId, messageId, delta}
event.agent_tool_start     {sessionId, toolCallId, tool, params}
event.agent_tool_update    {sessionId, toolCallId, partial}
event.agent_tool_end       {sessionId, toolCallId, ok, durationMs, summary}
event.agent_turn_end       {sessionId, usage}
event.agent_end            {sessionId, usage, stopReason}
event.agent_error          {sessionId, code, message}
```

Rust 侧复用 `BridgeActiveCall::next_event()` 的边收边处理循环（`lania-host/src/execution/bridge_helpers/requests.rs`），无需改传输层。

### 4.3 工具调用协议（Node → Rust，host RPC）

```
agent.tool.exec {
  sessionId, traceId, tool, params,        // params 已由 typebox 校验
  callId
} -> {
  status: "ok" | "error" | "denied" | "needs_approval",
  content: string,                          // 模型可见结果
  details?: object,                         // UI 用（diff、行数、耗时）
  preview?: {kind, title, body},            // needs_approval 时给 TUI
  grantId?: string
}
```

授权与执行在同一次调用内完成（防 TOCTOU）。`needs_approval` 时 Rust 推 `event.agent_tool_update`，TUI 弹审批，用户决定后 Rust 重新进入同一调用继续执行，Node 侧只等待最终 result。

### 4.4 内置工具表（封顶 13）

| 工具                      | tier  | 实现                              | 说明                                                                         |
| ------------------------- | ----- | --------------------------------- | ---------------------------------------------------------------------------- |
| `read`                    | read  | `lania-fs`                        | 支持 `path:start-end` 区间、二进制拒绝、图片按 mime 返回                     |
| `glob`                    | read  | `ignore` + `globset`              | 遵循 .gitignore                                                              |
| `grep`                    | read  | `ignore` + `regex`                | 输出文件:行:内容，上限截断                                                   |
| `ast_grep`                | read  | `tree-sitter 0.20`                | 按语言查询节点                                                               |
| `git_status` / `git_diff` | read  | `lania-git` / `lania-exec`        | diff 走 `git diff --no-color -U3`                                            |
| `write`                   | write | `lania-fs`                        | 创建/整体覆盖；返回 diff 摘要                                                |
| `edit`                    | write | `lania-agent-tools`               | 结构化补丁：`{path, ops:[{anchor, context, replacement}]}`，应用前校验上下文 |
| `ast_edit`                | write | `tree-sitter`                     | 节点替换，失败回退 `edit`                                                    |
| `exec`                    | exec  | `lania-exec`                      | 命令白名单 + 超时 + 输出截断；默认需要确认                                   |
| `task`                    | exec  | 子 agent 调度                     | 见 5.5                                                                       |
| `ask_user`                | read  | `lania-prompt` / host interaction | 阻塞式澄清                                                                   |
| `web_fetch`               | read  | `reqwest`（新增依赖）             | 仅 http(s)，限大小                                                           |

MCP 工具统一命名为 `mcp__<server>__<tool>`，默认 tier = `write`（与 omp 一致），按 server 配置可降级。

### 4.5 会话文件

```
.lan/agent/sessions/<sessionId>.jsonl
{"v":1,"type":"header","sessionId","cwd","createdAt","model","thinking","parentSessionId"?}
{"type":"user","id","parentId","ts","content"}
{"type":"assistant","id","parentId","ts","content","usage","stopReason"}
{"type":"tool","id","parentId","ts","toolCallId","tool","params","result","isError","durationMs"}
{"type":"compaction","id","parentId","ts","summary","firstKeptEntryId","tokensBefore"}
```

- append-only；当前分支叶子存 `.lan/agent/sessions/<sessionId>.leaf`（单行 id）。
- `--continue` 读 leaf 重建；`/rewind` 只改 `.leaf`；`/fork` 新建文件并写 `parentSessionId`。
- 不实现 omp 的版本迁移/blob 外置：v1 数据不跨大版本保留，破坏性变更直接拒绝加载并提示。

### 4.6 配置

v1 从 `lan.config.*` 的 `custom.agent` 读取（`LanConfigSnapshot.custom: Value` 现成，不动 schema）：

```jsonc
{
  "agent": {
    "model": "anthropic/claude-sonnet-4-5",
    "thinking": "medium",
    "approvalMode": "always-ask", // always-ask | write | yolo
    "subagents": { "maxConcurrent": 4, "isolation": "none" },
    "exec": { "allow": ["pnpm test", "cargo test", "git status"] },
    "skills": [".lan/skills"],
    "tools": [
      // 插件工具（见 5.2）
      { "name": "db_query", "command": "lan db query", "approval": "exec" },
    ],
  },
}
```

稳定后再提升为 `LanConfigSnapshot` 的强类型字段 + `lania` 包类型。

## 5. 子系统设计

### 5.1 循环与 provider

- Node 侧 `Agent`（`pi-agent-core`）：`initialState {systemPrompt, model, thinkingLevel, tools, messages}`。
- `transformContext`：每次请求前注入「项目上下文块」——`lan.config` 摘要、可用命令、skills 索引、memory 摘要。全部来自 Rust（`agent.session.create` 的 `systemPrompt` 与每轮 `agent.context` 附带）。
- 重试：pi 自带 provider 重试；Rust 只处理「工具失败」的重试策略（同 call 最多 2 次，参数不改）。
- 模型/思考级别切换：`/model`、`/thinking` 直接改 `Agent.state`，并写 `meta` entry。

### 5.2 工具面

- Node 侧每个内置工具是一个薄适配器：typebox schema 由 Rust 导出（`agent.tools.describe` 一次拉取），`execute` 只有一行 `hostCall('agent.tool.exec', ...)`。
- 插件工具：`lan.config` 声明 `{name, command, approval}` → Rust 生成 schema（`{args: string[]}` 或用户提供的 JSON Schema）→ 执行时 spawn `lan <command>`，JSON 参数经 stdin 传入，stdout 作为结果。
- 插件工具同样走 tier 审批；默认 `exec`。

### 5.3 TUI

- 组件：`TranscriptView`（消息 + 工具卡片）、`Composer`（多行输入、历史）、`StatusBar`（model / token / context%）、`ApprovalDialog`、`TodoPanel`。
- 事件循环：

```
loop {
  select! {
    key = crossterm EventStream => handle_key,
    ev  = agent_events.recv()    => handle_agent_event,
    _   = tick(100ms)            => {},
  }
  terminal.draw(...)?;
}
```

- 不使用 `event::read()` 阻塞（`command-env/tui` 是那种写法，不能照抄）；必须可被异步事件唤醒。
- 工具卡片的状态机：`pending → running(with partial) → done(ok|error)`，对应 `agent_tool_*` 三个事件。
- 窄屏（< 80×24）降级为纯文本流式输出，不做布局。

### 5.4 会话与压缩

- 写入时机：`agent_end` 后批量 append（user / assistant / tool 三类）；崩溃最多丢一轮。
- token 估算：`chars/3.2` 粗估（CJK 权重单独调），每轮 `agent_turn_end.usage` 校准。
- 触发：`contextTokens > 0.7 × contextWindow` 时：
  1. 机械裁剪：超过 200 行的旧工具结果截断为「前 20 行 + … + 后 20 行」。
  2. 仍超阈值 → 摘要：取最老 60% 的消息，用当前模型生成 ≤ 1500 token 摘要，写 `compaction` entry，`firstKeptEntryId` 指向保留起点。
- `/compact` 手动触发同一流程；摘要失败时回退「保留最近 40% 消息 + 丢弃其余」，并写 `warning` meta。

### 5.5 子 agent

- `task` 工具参数：`{context, tasks: [{name?, agent?: "task"|"scout", task, readOnly?, outputSchema?}]}`。
- 调度（Rust `lania-task`）：
  - `readOnly: true` → 并行，不占写锁，并发上限 `min(maxConcurrent, 8)`。
  - 写型 → 排队持有工作区写锁，**串行**；`subagents.isolation = "worktree"` 时改为每个子 agent 一个 `git worktree`（需在 `lania-git` 新增 `worktree_add/remove/list`，当前只有分支类 API）。
- 每个子 agent 一个 session 文件 + 一个输出文件 `.lan/agent/agents/<id>/output.md`；`read("agent://<id>")` 由 read 工具的内部 URL 解析。
- 汇总：父会话只拿到 `{status, summary, outputPath, patchPath?, usage}`，避免上下文爆炸。
- 预算：`maxConcurrent` + 每子 agent `maxTokens` 上限；超限直接 abort 并返回 `budget_exceeded`。

### 5.6 hooks / skills / MCP / memory

- **hooks**：`lania-hooks` 新增键 `onAgentStart`、`onAgentToolCall`（waterfall，可改参数/拒绝）、`onAgentToolResult`、`onAgentCompact`、`onSessionSwitch`。
- **skills**：扫描 `.lan/skills/*/SKILL.md`（项目）与 `~/.lan/skills`（用户）；启动时把 `name + description` 索引进 system prompt，正文由模型按需 `read`。
- **MCP**：Rust `rmcp`（stdio + SSE）。配置 `.lan/agent/mcp.json`；工具注册进同一张表，命名 `mcp__<server>__<tool>`。
- **memory**：仅 `local` 后端。启动注入 `MEMORY.md` 摘要（≤ 800 token）+ `learned.md` 最近 20 条；`learn` 工具（可选开启）追加条目，去重、限长 2000 字符。

### 5.7 性能策略

- 一切工具调用一次 RPC；目标额外开销 < 5 ms（工具本身秒级，噪声）。
- `grep`/`glob` 用 `ignore` 并行遍历 + `regex`；不引入后台索引（v1）。
- AST：`tree-sitter` 只解析目标文件；不做全仓查询。
- 若未来出现真正热路径（每 token 级），再评估 `lania-natives`（napi-rs）；**默认不引入**，避免平台矩阵。

## 6. 安全模型

- tier：`read` 自动放行；`write` 依 `approvalMode` 决定；`exec` 默认询问。
- 授权键：`(tool, 归一化目标)`——路径 `realpath` + Windows 大小写折叠；`read file.ts:10-20` 与 `write file.ts` 共享同一 key。
- 授权缓存：`.lan/agent/approvals.json`，记录 tier；read 授权不覆盖 write/exec。
- 沙箱：`exec` 默认继承环境但剥离 `*_TOKEN`/`*_SECRET`；可选 `agent.sandbox: "workdir"` 限制路径逃逸（v1 只做路径校验，不做容器）。
- 密钥：不落会话文件；工具结果中命中 `sk-`/`ghp_`/`AKIA` 等模式时脱敏后再入库。

## 7. 里程碑

| 阶段 | 内容                                                                                               | 验收                                                         | 估时   |
| ---- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ | ------ |
| M0   | `@lania-cli/agent` 骨架；bridge `agent.session.create` / `agent.prompt`；事件 emit；`lan agent -p` | 能对仓库提问并输出答案；事件流可见                           | 1 周   |
| M1   | 工具 8 个（read/glob/grep/write/edit/exec/git_status/git_diff）+ 审批 + 会话落盘                   | 修真实 bug 并跑测试；`--continue` 续上下文；未授权 exec 弹窗 | 3 周   |
| M2   | TUI（消息/工具卡片/输入/状态栏/审批）                                                              | 键盘全操作；Ctrl-C 只中断生成；退出终端干净                  | 2–3 周 |
| M3   | `task` 子 agent（只读并行 + 写串行）+ hooks + `agent://`                                           | 三个并行 scout 汇总；hook 能拒绝工具调用                     | 2–3 周 |
| M4   | AST 工具、skills、MCP、memory、压缩                                                                | 长会话不爆窗；`/skill` 生效；MCP 工具可用且受审批            | 3 周   |

## 8. 与 omp 的差异（同一张功能表，相反的底座）

| 维度     | omp                       | lan agent                         |
| -------- | ------------------------- | --------------------------------- |
| 执行位置 | Node + N-API natives      | 全部 Rust，Node 只编排            |
| 授权     | Node 进程内               | Rust 同调用内，防 TOCTOU          |
| 工具来源 | 内置 30+                  | 内置 13 + 插件协议                |
| 长尾能力 | 全内置                    | 不内置，第三方插件                |
| 状态位置 | `~/.omp/agent`            | 项目内 `.lan/agent`，可 git diff  |
| 进程形态 | Bun 编译二进制 + 常驻能力 | 单一 `lan` 二进制 + 子进程 bridge |

## 9. 风险

| 风险                   | 处理                                                                                    |
| ---------------------- | --------------------------------------------------------------------------------------- |
| pi 上游 API 变更       | 版本精确锁定；所有 pi 导入收敛在 `ts/packages/agent/src/kernel/`，其余代码不直接 import |
| Windows 终端兼容       | TUI 走 crossterm 跨平台路径；CI 至少覆盖 Windows 冒烟（`--snapshot` 渲染一帧）          |
| 子 agent 成本失控      | 并发上限 + 每 agent token 预算 + 总量告警                                               |
| 压缩质量差导致行为退化 | 保留最近 40% 原文；摘要 entry 可见可回滚（rewind 到 compaction 之前）                   |
| RPC 长任务超时         | 工具调用使用独立 timeout（默认 10 分钟），与 bridge 请求超时分离                        |

## 10. 拆包施工计划（`lania-agent-core`）

拆包本身不改行为，属**纯重构**，可在功能开发过程中穿插进行。

| 阶段 | 内容                                                                                               | 验收                                               |
| ---- | -------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| A0   | 新建 `lania-agent-core`，把 `spec.rs` / `event.rs` / `client.rs` 写出来（先只放类型，不接 bridge） | `cargo check` 通过；无 host 依赖                   |
| A1   | `bridge.rs` 承接现有 `create_session` / `run_prompt` 的请求构造逻辑                                | `lania-agent` 改为调用 core，`agent` 命令 e2e 全绿 |
| A2   | 把 `render_event` / REPL / `compact_json` 收到 `render.rs`                                         | 输出与改造前逐字节 diff 为空                       |
| A3   | 加 `AgentSessionClient` 的 mock 实现 + 单测                                                        | 无 node 进程也能跑通 handler 主流程                |
| A4   | （配套）TS 侧 `plugin-registry.ts` 改动态 import，agent 变可选                                     | 移除 agent 依赖后 `lan build` / `lint` 仍可用      |

**A4 的风险**：`readyEvent().plugins` 与 `HandshakeResponse.methods` 会随 agent 是否安装而变化。
Rust 侧目前不校验 methods（只记录），因此安全；但**必须在 A4 同 PR 补一条 e2e**：临时将
`node_modules/@lania-cli-v2/agent` 改名，验证 `lan --help` / `lan build` / `lan lint` 均不报错。
