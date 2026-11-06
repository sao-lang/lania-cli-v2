# lan pi 技术方案

> 目标：在 `lan` 上提供与 omp 同量级能力的 coding agent，但底座与 omp 相反——
> **执行权威在 Rust 宿主，Node 只做模型编排**；长尾能力走插件协议而非内置。
>
> 命令名：`lan pi`（`lan agent` 保留为别名，见 §3.1）。

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

> **已落地**（见 §10）：`lania-agent-core` 已建立，`lania-agent` 已瘦身为命令层，
> 命令名改为 `pi`、`agent` 作为别名。以下是实际的文件布局。

已落地的三层结构（仿 runex 拆法：`runex-core` → `runex` → `lania-plugins-command-env`）：

```text
lania-agent-core                   ← 零 lania 依赖：类型与算法
├── Cargo.toml  deps: anyhow, async-trait, serde, serde_json, tokio
└── src/
    ├── spec.rs     SessionSpec / PromptRequest / ApprovalMode / ThinkingLevel / ToolTier
    ├── event.rs    AgentEvent::from_method(method, params) + Usage
    ├── client.rs   trait AgentSessionClient / SessionInterrupt + mock
    ├── session.rs  SessionStore / 分支与 leaf / fork
    └── config.rs / tokens.rs / prompt.rs

lania-agent                        ← 产品层：零 lania 依赖（只依赖 core 与通用库）
├── Cargo.toml  deps: core + crossterm / ratatui / unicode-width / tokio ...
└── src/
    ├── lib.rs           run(AgentOptions, Box<dyn AgentSessionClient>, Option<ApprovalSetup>)
    ├── options.rs       AgentOptions（纯数据，不认识 CommandExecutionContext）
    ├── approval.rs      ApprovalBus / ApprovalPrompt（通道注入）
    ├── session_plan.rs / context.rs / runtime.rs / render.rs / repl.rs
    └── tui/             mod.rs / app.rs / view.rs / actor.rs

lania-plugins-command-pi           ← CLI 适配层（唯一认识宿主的地方）
├── Cargo.toml  deps: lania-agent, lania-agent-core, lania-command, lania-host, lania-node-bridge
└── src/
    ├── lib.rs        PiCommandPlugin（命令 pi，别名 agent）
    ├── handler.rs    CommandExecutionContext → AgentOptions；装审批 broker / 门禁
    ├── options.rs    参数提取（错误返回，不碰 EXIT_USAGE）
    └── bridge.rs     BridgeAgentSessionClient：把 NodeBridgeClient 注入成 AgentSessionClient
```

依赖方向（单向，无环；引擎在 Node 子进程，通过 bridge 协议接入）：

```text
lania-cli ──▶ lania-plugins-command-pi ──▶ lania-agent ──▶ lania-agent-core
                        ├──▶ lania-host ──────────────▶ lania-node-bridge
                        └──▶ lania-command
```

**边界判定标准**（可作为 code review checklist）：

| 判定项                                                | 归属                               |
| ----------------------------------------------------- | ---------------------------------- |
| `CommandSpec` / `Plugin` / `register_builtin_command` | `lania-plugins-command-pi`         |
| argv 解析（`option_string` / `positional_prompt`）    | `lania-plugins-command-pi`         |
| `eprintln!` / `print!` / `io::stdout().flush()`       | `lania-agent`                      |
| bridge 方法名与传输调用（`agent.session.create` 等）  | `lania-plugins-command-pi::bridge` |
| 事件结构（`AgentEvent`）与事件名映射                  | `lania-agent-core`                 |
| 终端颜色、`/help` 文案、进度提示                      | `lania-agent`                      |
| 工具执行、门禁、审批裁决                              | `lania-host`（主包不见）           |

**验收标准**：

- [x] `cargo tree -p lania-agent-core` / `-p lania-agent` 中不出现 `lania-host` / `lania-command` / `lania-node-bridge`。
- [x] 产品入口是纯数据 + trait：`run(AgentOptions, Box<dyn AgentSessionClient>, Option<ApprovalSetup>)`。
- [x] `pi` 命令支持 `--approval-mode`、`/help`、`/model`、`/quit` 与新增的会话命令。
- [x] `lania-agent-core` 的 `AgentSessionClient` 可被单测 mock（`mock::MockSessionClient`，不启动 node 进程）。
- [ ] 原 `agent` 命令「输出逐字节一致」已不适用：命令名与交互形态都变了（见 §3.3）。

### 3.2 与 TS 包的对应关系

| Rust                                                           | TypeScript                         | 职责                                   |
| -------------------------------------------------------------- | ---------------------------------- | -------------------------------------- |
| `lania-agent-core::AgentSessionClient`                         | `@lania-cli-v2/agent`（`kernel/`） | 会话与模型循环的**边界**               |
| `lania-agent`（产品层） / `lania-plugins-command-pi`（适配层） | —                                  | 运行/TUI 与 CLI 接线（都不含模型编排） |
| `lania-node-bridge`（传输）                                    | `@lania-cli-v2/node-bridge`        | 协议与进程管理                         |

⚠️ 跨语言边界只有一处：`agent.*` 6 个 bridge 方法与 `event.agent_*` 9 个事件。
两侧的 Rust 结构体与 TS 类型必须逐字段对齐（见 §4.2、§4.3），改动需同 PR 完成。
**待改进（§11）**：这些名字/字段目前仍手写于 Rust core / 插件层 / 宿主 / TS 多处，
下一步用「协议单一事实源 + 代码生成」消除重复定义。

当前 TS 侧的耦合问题：`ts/packages/node-bridge/src/core/plugin-registry.ts` 在**模块顶层静态 import** `@lania-cli-v2/agent`，且 `node-bridge` 把它写在 `dependencies`。这让「装 CLI 必须拉 pi 依赖」。本方案不强制改（agent 是 CLI 的一等功能），但**在 `lania-agent-core` 落地后应改为动态 import**，使 bridge 可独立运行。

### 3.3 命令形态与实现状态

```bash
lan pi                         # 全屏 TUI（stdout 是终端时）
lan pi "explain the crate layout"
lan pi -p                      # 单次；无位置参数时从 stdin 读
lan pi -c                      # 续接最近一次会话
lan pi --resume <session-id>
lan pi --sessions              # 列出会话
lan pi --new                   # 强制新建
lan pi --no-session            # 不落盘
lan pi --json                  # 一行一个事件（末尾补一条 result）
lan pi --no-tui                # 强制行式 REPL
lan pi --snapshot              # 渲染 TUI 两帧到 stdout 后退出（CI）
lan pi --model claude-sonnet-4-5 --thinking high --approval-mode write
```

斜杠命令（TUI 与行式 REPL 共用一套语义）：`/help` `/status` `/model [id]` `/thinking [level]`
`/approve [mode]` `/compact` `/rewind [n]` `/new` `/fork` `/sessions` `/quit`。

TUI 键位：`Enter` 发送 · `Ctrl-J` 换行 · `PgUp/PgDn` 滚动 · `Up/Down` 历史（输入为空时）
· `Esc` 中断生成 · `Ctrl-C` 中断/退出 · 审批弹窗内 `a` 单次 `s` 永久 `d`/`Esc` 拒绝。

**本轮落地的范围**（对照 §7 里程碑）：

| 里程碑                         | 状态 | 说明                                                                                                                                                                                           |
| ------------------------------ | ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M0 骨架 + bridge + 一次性      | ✅   | `lan pi -p`，事件流可见                                                                                                                                                                        |
| M1 工具 8 个 + 审批 + 会话落盘 | ✅   | 工具 10 个（见 §4.4），`--continue` 续上下文，未授权 exec 弹窗                                                                                                                                 |
| M2 TUI                         | ✅   | 全屏 TUI（ratatui）：正文折行 + 底部对齐、工具三态卡片、thinking 折叠、多行输入 + 历史、审批覆盖层、状态栏、滚动、Esc/Ctrl-C 中断；`--no-tui` 回落行式 REPL，`--snapshot` 供 CI。实现见 §5.3。 |
| M3 `task` 子 agent + hooks     | ✅   | 子 agent（只读并行/写串行/禁递归）；工具门禁 `custom.agent.guard`（deny / require-approval）。`agent://` 输出文件未做。                                                                        |
| M4 AST / skills / MCP / memory | 部分 | skills 索引与 memory 注入已做（读 `.lan/skills`、`.lan/agent/memory`）；AST、MCP 未做                                                                                                          |

**与原文的偏差（有意，均已在此登记）**：

1. **工具 schema 留在 TS**（typebox），Rust 只做 tier 判定与执行。原文 §5.2 计划由 Rust 导出 schema；但 schema 只影响「模型看到什么」，tier 才影响「能不能干」，把安全相关的部分留在 Rust、把提示词相关的部分留在 TS，避免同一份 schema 在两种语言里各写一遍再对账。
2. **压缩摘要是抽取式**（谁问过什么 / 动过哪些文件 / 用过哪些工具），不额外发起一次模型调用。确定性、可单测、零成本；语义摘要留到后续版本。
3. **Ctrl-C 只中断生成**：通过 `NodeBridgeClient` 的 `Arc` 共享，在 `agent.prompt` 飞行期间插入 `agent.interrupt`，不 drop 未来的 future。

## 4. 关键接口

### 4.1 Bridge 方法（Rust → Node）

| 方法                   | 参数                                                                     | 返回                                             |
| ---------------------- | ------------------------------------------------------------------------ | ------------------------------------------------ |
| `agent.session.create` | `{sessionId, model, thinking, systemPrompt, messages, tools, maxTokens}` | `{sessionId, model, contextWindow}`              |
| `agent.prompt`         | `{sessionId, prompt, images?}`                                           | 流式事件 + `{text, messages, usage, stopReason}` |
| `agent.interrupt`      | `{sessionId}`                                                            | `{ok}`                                           |
| `agent.state`          | `{sessionId}`                                                            | `{isStreaming, pendingToolCalls, contextTokens}` |
| `agent.models.list`    | `{provider?}`                                                            | `[{provider, id, contextWindow, reasoning}]`     |

会话历史由 Rust 持有：`session.create` 时把 JSONL 还原成 `messages` 一次性传入，Node 不落盘。
`prompt` 返回的 `messages` 只含**本轮新增的模型产出**（assistant / toolResult）——user 消息在发问前已由 Rust 落盘，避免重复记录。

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

**已实现 11 个**（`lan pi` 启动时打印的实际工具表，与本节一致）：

| 工具               | tier  | 实现                      | 说明                                                           |
| ------------------ | ----- | ------------------------- | -------------------------------------------------------------- |
| `read`             | read  | `std::fs` + 行区间        | `start`/`end` 1-based、二进制拒绝、2000 行/20KB 截断           |
| `glob`             | read  | `ignore` + `globset`      | 遵循 .gitignore，上限 500 条                                   |
| `grep`             | read  | `ignore` + `regex`        | 输出 `path:line: text`，上限 200 条                            |
| `git_status`       | read  | `lania-exec`（argv 直传） | `git status --short --branch`                                  |
| `git_diff`         | read  | `lania-exec`（argv 直传） | `git diff --no-color -U3`，支持 `path` / `staged`              |
| `write`            | write | `std::fs`                 | 自动建父目录、整体覆盖；返回字节数与行数                       |
| `edit`             | write | 精确串替换                | `old_string` 必须唯一（或 `replace_all`），否则报错不写盘      |
| `exec`             | exec  | `lania-exec`              | 超时 10 分钟、输出截断、默认需要确认                           |
| `project_command`  | exec  | `lania-exec`              | 以 argv 形式跑 `lan <args>`，避免 shell 注入                   |
| `task`             | exec  | Node 侧子 agent（§5.5）   | 独立上下文；只读并行、可写串行；深度 ≥1 不注册，禁递归         |
| （MCP / 插件工具） | write | `lan.config` 声明         | 命名 `mcp__<server>__<tool>`，默认 tier = `write` 并按配置降级 |

**未实现（保持名额）**：`ast_grep` / `ast_edit`（tree-sitter）、`ask_user`、`web_fetch`。
名额上限仍是 13，新增内置工具需说明「为什么插件做不到」。

工具**参数 schema 在 TS（typebox）**，tier 与执行在 Rust（见 §3.3 偏差说明 1）。

### 4.5 会话文件

```
.lan/agent/sessions/<sessionId>.jsonl
{"v":1,"type":"header","sessionId","cwd","createdAt","model","thinking","parentSessionId"?}
{"type":"user","id","parentId","ts","message":{"role":"user",...}}
{"type":"assistant","id","parentId","ts","message":{"role":"assistant",...}}
{"type":"tool","id","parentId","ts","message":{"role":"toolResult",...}}
{"type":"compaction","id","parentId","ts","summary","firstKeptEntryId","tokensBefore"}
{"type":"meta","id","parentId","ts","meta":{...}}
```

- `message` 保存 pi 的**原始消息 JSON**（不做字段级重建）：恢复时原样回放，pi 升级消息结构不需要改 Rust。
- append-only；当前分支叶子存 `.lan/agent/sessions/<sessionId>.leaf`（单行 id）。
- `--continue` 读 leaf 重建；`/rewind` 只改 `.leaf`；`/fork` 新建文件并写 `parentSessionId`。
- 损坏容忍：解析失败的尾行直接跳过（崩溃最多丢当前这一轮）。
- 不实现 omp 的版本迁移/blob 外置：v1 数据不跨大版本保留，`v` 不匹配直接拒绝加载并提示。

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
    // 工具门禁：命中即拒绝 / 强制审批（顺序敏感，首个命中生效）
    "guard": [
      { "tool": "exec", "pattern": "rm\\s+-rf", "action": "deny", "reason": "no recursive delete" },
      { "tool": "write", "pattern": "\\.env", "action": "require-approval" },
    ],
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

组件拆成三个文件，边界与 `render.rs` 一致——**只有 view 层认识 ratatui**：

| 文件           | 职责                                                                                                                                |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `tui/app.rs`   | `App` 状态机：行模型（User/Assistant/Thinking/Tool/Notice/Error）、输入框（char 光标 + 历史）、弹窗、滚动。纯数据，可在单测里喂按键 |
| `tui/view.rs`  | 只用 ratatui 画一帧：正文（按显示宽度手工折行）、输入框、状态栏、审批覆盖层、窄屏提示                                               |
| `tui/actor.rs` | 运行时 actor：`UiCommand` 进 / `UiUpdate` 出，外加 `UiSink` 的通道实现与审批代理                                                    |
| `tui/mod.rs`   | 终端生命周期（`TerminalGuard` 保证还原 raw mode）、输入线程、三路事件合流、`--snapshot`                                             |

**为什么是 actor 而不是在 UI 里直接调运行时**：`run_turn` 会在 `await` 上停留数秒到数分钟，
期间界面必须继续刷新并响应 Ctrl-C。所以运行时被搬进独立 task（`OwnedBridgePort` 让它 `'static`），
UI 只发命令收更新。副作用是 `/model`、`/compact`、`/fork` 天然串行，不会出现「生成中切模型」。

事件循环：

```
loop {
  terminal.draw(&app)?;
  select! {
    key   = key_rx.recv()     => handle_key,        // 独立线程 poll(80ms) + read
    upd   = update_rx.recv()  => apply_update,      // actor
    ask   = approval_rx.recv()=> app.dialog = Some(..),  // 宿主 broker
    _     = sleep(120ms)      => spinner += 1,
  }
}
```

- 键盘不用 `crossterm EventStream`：独立线程 `poll` 比引 `futures` 依赖更轻，
  且主循环永远不会被 `event::read()` 卡住（`command-env/tui` 那种写法不能照抄）。
- 工具卡片三态：`agent_tool_start → running`、`agent_tool_update → partial`、`agent_tool_end → done(ok|error)`。
- 窄屏（< 40×10）只画一行提示，不做布局；`--snapshot` 用 `TestBackend` 渲染一帧，
  CI 里比对文本即可发现渲染退化（不需要 TTY）。

**审批为什么需要新通道**：授权判定必须留在宿主的 `agent.tool.exec` 内（防 TOCTOU），
但「问用户」在 TUI 里要弹覆盖层、在行式 REPL 里读 stdin。
因此宿主新增 `lania_host::approval::ApprovalBroker` 插槽：命令层进入交互前装上实现，
退出时由 RAII handle 摘除；未安装时回落到原有终端提示。broker 的 `ask` 运行在 bridge
reader 线程上，所以实现必须是**有界等待**（默认 600s，超时按拒绝）。

### 5.4 会话与压缩

- 写入时机：`agent_end` 后批量 append（user / assistant / tool 三类）；崩溃最多丢一轮。
- token 估算：`chars/3.2` 粗估（CJK 权重单独调），每轮 `agent_turn_end.usage` 校准。
- 触发：`contextTokens > 0.7 × contextWindow` 时：
  1. 机械裁剪：超过 200 行的旧工具结果截断为「前 20 行 + … + 后 20 行」。
  2. 仍超阈值 → 摘要：取最老 60% 的消息，用当前模型生成 ≤ 1500 token 摘要，写 `compaction` entry，`firstKeptEntryId` 指向保留起点。
- `/compact` 手动触发同一流程；摘要失败时回退「保留最近 40% 消息 + 丢弃其余」，并写 `warning` meta。

### 5.5 子 agent

**已实现**（`ts/packages/agent/src/kernel/subagent.ts`）：

- 参数：`{description, prompt, read_only?}`，`read_only` 默认 true。
- 与原文的关键差异：子 agent 是 **Node 侧又一个 pi `Agent` 实例**，不是 Rust 调度。
  原因很硬：`agent.tool.exec` 跑在 bridge 的 reader 线程上，在那里再发**嵌套 bridge 请求**
  （`agent.session.create` / `agent.prompt`）会自己等自己 → 死锁。
  子 agent 的每次工具调用仍然走 `agent.tool.exec`，所以宿主的门禁与审批照常生效——
  被移动的只是「模型循环」，那本来就是 Node 的职责。
- 并发：上限 4；只读子 agent 并行，**可写子 agent 走全局写锁串行**。
- 递归：深度 ≥ 1 时工具表里**不注册 `task`**，结构性禁止。
- 结果：只回传最终文本（截断 8KB），父会话不会被子 agent 的中间过程撑爆。
- 未实现（仍在设计中）：`tasks[]` 批量形态、`outputSchema`、worktree 隔离、
  `.lan/agent/agents/<id>/output.md` 与 `agent://<id>` 解析、子 agent usage 汇总。

### 5.6 hooks / skills / MCP / memory

- **hooks（工具门禁，已实现）**：`custom.agent.guard` 规则在宿主内同步判定，
  `deny` 直接拒绝、`require-approval` 把 yolo 降级为「要问」。
  没有直接接 `lania-hooks` 的 hook bus：那条链路是 async 的，而 `agent.tool.exec`
  跑在 bridge reader 线程上，`block_on` 一个可能拉起子进程的 hook 会把事件流与心跳一起卡住。
  规则不经过 Node，模型无法伪造参数绕过。
- **skills（已实现）**：扫描 `.lan/skills/*/SKILL.md`（项目）+ `~/.lan/skills`（用户），
  把 `name + description + path` 索引进 system prompt，正文由模型按需 `read`。
  `/skill` 显式调用未做（索引已覆盖主要场景）。
- **MCP（未实现）**：计划用 `rmcp`（stdio + SSE），配置 `.lan/agent/mcp.json`，
  工具命名 `mcp__<server>__<tool>`，默认 tier = write。
- **memory（已实现）**：注入 `.lan/agent/memory/MEMORY.md` + `learned.md`（截断 4800 字符）；
  `learn` 工具未做。

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

状态以 `scripts/smoke-pi.mjs`（无需真实模型）的通过情况为准。

| 阶段 | 内容                                                                                                       | 验收                                                         | 估时   | 状态 |
| ---- | ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ | ------ | ---- |
| M0   | `@lania-cli/agent` 骨架；bridge `agent.session.create` / `agent.prompt`；事件 emit；`lan pi -p`            | 能对仓库提问并输出答案；事件流可见                           | 1 周   | ✅   |
| M1   | 工具 10 个（read/glob/grep/git_status/git_diff/write/edit/exec/project_command + 插件位）+ 审批 + 会话落盘 | 修真实 bug 并跑测试；`--continue` 续上下文；未授权 exec 弹窗 | 3 周   | ✅   |
| M2   | TUI（消息/工具卡片/输入/状态栏/审批）                                                                      | 键盘全操作；Ctrl-C 只中断生成；退出终端干净                  | 2–3 周 | ✅   |
| M3   | `task` 子 agent（只读并行 + 写串行）+ 工具门禁 + `agent://`                                                | 子 agent 结果回传；门禁能拒绝工具调用                        | 2–3 周 | ✅   |
| M4   | AST 工具、skills、MCP、memory、压缩                                                                        | 长会话不爆窗；`/skill` 生效；MCP 工具可用且受审批            | 3 周   | 部分 |

M2 现状：全屏 TUI 已落地（ratatui 0.28 + crossterm 0.29）。正文按显示宽度折行并底部对齐、
工具三态卡片、thinking 折叠成一行、多行输入（Ctrl-J 换行）+ 历史、审批覆盖层
（`a` 单次 / `s` 永久 / `d`·Esc 拒绝）、状态栏（model · thinking · approval · ctx%）、
PageUp/PageDown 滚动、Esc/Ctrl-C 中断生成、Ctrl-C 空闲退出。
`--no-tui` 回落到行式 REPL；stdout 非 TTY 时不进 TUI（脚本/管道安全）。
`--snapshot` 渲染两帧（正文 / 审批弹窗）到 stdout，CI 可直接比对。

M3 现状：`task` 子 agent（Node 侧 pi Agent，工具调用仍走宿主 → 门禁与审批不被绕过）；
工具门禁 `custom.agent.guard` 在宿主内同步判定（`deny` / `require-approval`）；
`/rewind [n]` 只移 leaf 不删数据。`agent://` 输出文件与子 agent usage 汇总未做。

M4 现状：skills 索引、memory 注入、自动/手动压缩（抽取式摘要）已做；AST、MCP、`/skill` 未做。

## 8. 与 omp 的差异（同一张功能表，相反的底座）

| 维度     | omp                       | lan pi                            |
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

| 阶段 | 内容                                                                                               | 验收                                            | 状态 |
| ---- | -------------------------------------------------------------------------------------------------- | ----------------------------------------------- | ---- |
| A0   | 新建 `lania-agent-core`，把 `spec.rs` / `event.rs` / `client.rs` 写出来（先只放类型，不接 bridge） | `cargo check` 通过；无 host 依赖                | ✅   |
| A1   | `bridge.rs` 承接现有 `create_session` / `run_prompt` 的请求构造逻辑                                | `lania-agent` 改为调用 core，`pi` 命令 e2e 全绿 | ✅   |
| A2   | 把 `render_event` / REPL / `compact_json` 收到 `render.rs` / `repl.rs`                             | 渲染层与传输层无交叉引用                        | ✅   |
| A3   | 加 `AgentSessionClient` 的 mock 实现 + 单测                                                        | 无 node 进程也能跑通 handler 主流程             | ✅   |
| A4   | （配套）TS 侧 `plugin-registry.ts` 改动态 import，agent 变可选                                     | 移除 agent 依赖后 `lan build` / `lint` 仍可用   | ⏳   |

**A4 的风险**：`readyEvent().plugins` 与 `HandshakeResponse.methods` 会随 agent 是否安装而变化。
Rust 侧目前不校验 methods（只记录），因此安全；但**必须在 A4 同 PR 补一条 e2e**：临时将
`node_modules/@lania-cli-v2/agent` 改名，验证 `lan --help` / `lan build` / `lan lint` 均不报错。

> 已知坑（A4 之前也值得知道）：`NodeBridgeClient::call_async` 在 process transport 启动失败时会
> **静默退回 mock**，表现为 `E_METHOD_NOT_FOUND: Unsupported method: agent.session.create`。
> 排查时设 `LANIA_BRIDGE_DEBUG=1` 可看到真实的 spawn 错误。

## 11. 收口进度（2026-11-06）

本次收口 = 三层拆包重构完成 + 可验证部分的验证结果 + 遗留待办。

### 11.1 拆包落地状态

| 层     | crate / 包                 | 职责                                              | 依赖                                                 | 状态        |
| ------ | -------------------------- | ------------------------------------------------- | ---------------------------------------------------- | ----------- |
| 能力层 | `lania-agent-core`         | 类型、事件、会话文件、配置、压缩、prompt          | 仅通用库，**零 lania 依赖**                          | ✅          |
| 产品层 | `lania-agent`              | `run()`、TUI/REPL、审批通道、会话规划             | core + ratatui/crossterm 等通用库，**零 lania 依赖** | ✅          |
| 适配层 | `lania-plugins-command-pi` | `lan pi` 命令、bridge 客户端、审批/门禁接线       | `lania-host` / `lania-command` / `lania-node-bridge` | ✅ 代码完成 |
| 引擎   | `ts/packages/agent`        | pi-agent-core / pi-ai 编排、工具 schema、子 agent | `pi-agent-core@1.4.0` / `pi-ai@1.4.0`                | ✅          |
| 宿主   | `lania-host`               | 工具执行、门禁、审批裁决                          | —                                                    | ✅          |

关键接缝（保证主包不反向依赖脚手架）：`AgentOptions` 是纯数据，由适配层从
`CommandExecutionContext` 翻译；传输以 `AgentSessionClient` trait 注入，
`BridgeAgentSessionClient` 只存在于适配层；审批是 `UnboundedReceiver<ApprovalPrompt>`
单向通道；工具执行/门禁/裁决全在宿主，主包完全不见。

### 11.2 验证结果

| 项目                   | 命令                                            | 结果                   |
| ---------------------- | ----------------------------------------------- | ---------------------- |
| core + 产品层单测      | `cargo test -p lania-agent-core -p lania-agent` | ✅ 29 + 44 = 73 passed |
| TS 引擎类型检查        | `pnpm --filter @lania-cli-v2/agent typecheck`   | ✅                     |
| pi 插件 / 全 workspace | `cargo check -p lania-plugins-command-pi`       | ⏳ 环境阻塞（见 11.3） |
| 端到端冒烟（8 项断言） | `node scripts/smoke-pi.mjs`                     | ⏳ 重构前 8/8，待重跑  |

本次收口修复：`lib.rs` 的 `AgentOptions` 重复导入（E0252）、`tui/actor.rs` 无用导入、
`SessionPlan::resolve` 恢复不存在会话时暴露底层 os error（现返回 `unknown session ...`）。

### 11.3 环境阻塞（非代码缺陷）

本机没有 MSVC 的 VC 工具集/Windows SDK，也不再具备独立 MinGW-w64；
`lania-workflows → tree-sitter` 的 C 代码需要完整 `gcc`（含 cc1），
而 rustup 自带的 self-contained MinGW 只有链接工具（ld/as/ar/dlltool + gcc 驱动），
缺 cc1，故 `cargo check` 停在 tree-sitter 的 build script：
`Failed to find tool. Is gcc.exe installed?`。

解除办法：安装 MinGW-w64（含 C 编译器）或 MSVC Build Tools 的 VC 工具集 + Windows SDK，然后重跑：

```bash
cargo check -p lania-plugins-command-pi
cargo test --workspace
node scripts/smoke-pi.mjs
```

本地临时配置（**不入库**）：`rust/.cargo/config.toml` 把 gnu target 的 linker 指向 rustup
自带的 `x86_64-w64-mingw32-gcc.exe`；目录级 `rustup override` 到
`stable-x86_64-pc-windows-gnu`；跑测试时用 `TMP/TEMP` 指向 `rust/target/tmp`
（沙箱不允许写系统 Temp）。

### 11.4 下一步待办（仅方案，未开工）

1. **协议单一事实源**：`agent.*` / `event.agent_*` / `agent.tool.exec` 的方法名与字段名
   目前手写于约 10 个文件（Rust core、插件层、`lania-node-bridge::BridgeEventMethod`、
   宿主、TS 侧）。计划用一份 schema 生成两端绑定（`gen-bridge-protocol.mjs` + `--check`），
   消除「改动需同 PR」的人肉同步。
2. **bridge 公共逻辑抽离 + 语言 Provider 化**：仓库已有 `lania-node-bridge` /
   `lania-py-bridge` / `lania-go-bridge` / `lania-dart-bridge` / `lania-rs-bridge` /
   `lania-bridge-support`，各语言 bridge 共用同一套 jsonl 协议但传输逻辑分散，
   方案另文讨论。
3. M4 遗留：AST 工具、MCP、`/skill`、`agent://` 输出文件、子 agent usage 汇总。
