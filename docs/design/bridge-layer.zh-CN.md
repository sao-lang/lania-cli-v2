# 桥接层设计：Bridge Hub（语言中立消息总线）

> 状态：**方案定稿 / 代码暂停未实施**（2026-11-06）
>
> - **Bridge Hub 用 Rust 实现**，这是定死的技术选型。
> - 暂停原因：其它模块要重新设计，先落代码会互相影响，因此桥接层实现整体回退，
>   本文只固化设计决策，作为后续重新开工的基线。**仓库中当前不含任何桥接层实现代码。**
> - 代码基线：`894bbb9`（agent 三层拆包收口，`af02077` / `894bbb9` 两个提交保留不动）。
> - 暂停前的实现存档：分支 `archive/bridge-hub-wip`（含 `9fe6faf` / `6cb01f7` 两个提交）
>   - `git stash@{0}`（`bridge-hub WIP (paused, not merged)`，含 Hub 与拆包草稿）。
>     需要恢复时 `git checkout archive/bridge-hub-wip` 再 `git stash pop`。
> - 前置文档：`agent-architecture.zh-CN.md`（三层拆包）、`multi-runtime-bridge.zh-CN.md`（现状）。

---

## 1. 目标：两端不需要知道对方是什么语言

一句话：**任意两个语言运行时，通过一个 Rust 实现的 Hub 通信；两端都不指定对方的语言。**

- 端点可以是 `node` / `bun` / `deno` / `python` / `go` / `dart` / `rust` 任意组合；
  CLI 里 `node ↔ rust` 走这个包，其它语言之间也走同一个包。
- 通信不靠「按语言拿客户端」，而靠**广播/监听事件收发**与**按能力请求**：
  - `publish(topic, payload)` → Hub 广播给所有订阅该 topic 的端点
  - `subscribe(topic)` / `unsubscribe(topic)` → 动态订阅
  - `request(capability, params)` → Hub 路由到声明该能力的端点，响应回到发起方
- 寻址键只有 `capability`、`topic`、opaque 的 `endpointId` 与 `correlation id`，
  **帧里不含 `language` / `runtime` / `RuntimeKind` 等任何语言字段**。
- 端点全双工、彼此平等：任何端点都能同时提供能力、发起请求、发布事件、订阅主题。
  Rust 不再是「永远的发起方」，只是众多端点之一。
- 包**内置桥接两端各语言的 Provider/adaptor**（下称**端点 adaptor**）；
  新语言接入只需实现帧协议（或调用包里生成的端点骨架），无需改 Hub。

### 与旧模型（按语言寻址）的区别

|               | 旧：`BridgeRegistry`（现状 + 暂停前的 P0.5 草稿）    | 新：Bridge Hub                                                        |
| ------------- | ---------------------------------------------------- | --------------------------------------------------------------------- |
| 寻址          | `client_for(RuntimeKind::Python)` 按语言拿 client    | `request("lint.run", ...)` / `publish("event.diag", ...)` 按能力/主题 |
| 角色          | Rust host 发起、语言进程被调；Rust 是特殊角色        | 端点平等，任意两端可互发                                              |
| 拓扑          | 1:1 stdio 直连，无法广播                             | Hub fan-out，N:M                                                      |
| 语言知识      | 宿主硬编码 5 个语言 bridge crate、`RuntimeKind` 枚举 | 宿主只认 `Endpoint` 与能力表，不认语言                                |
| Provider 含义 | 「怎么启动某个语言的进程」                           | 「怎么连入总线 + 声明 provides/subscribes」                           |

---

## 2. 架构

```
                    ┌────────────────────────────────────────┐
                    │        Bridge Hub（Rust 实现）           │
                    │  端点表 / 能力表 / 主题表 / pending 表    │
                    │  只按 capability + topic 路由与广播       │
                    └──┬─────┬─────┬─────┬─────┬─────┬──────┘
        帧：无语言字段  │     │     │     │     │     │
      ┌──────────┴┐ ┌─┴───┐ │ ┌───┴─┐ ┌─┴────┐ ┌─┴────┐
      │  Rust     │ │Node │ │ │Bun  │ │Python│ │Go/Dart│  ← 任意端点
      │  Endpoint │ │Endpt│ │ │Endpt│ │Endpt │ │Endpt  │
      └───────────┘ └─────┘ │ └─────┘ └──────┘ └──────┘
                            （每语言一个内置端点 adaptor）

  lania-cli ─┐
  lania-agent│  都只依赖桥接层包，调用 attach / publish / subscribe / request
  其它包  ───┘  不 import 任何具体语言 bridge crate
```

**Hub 是唯一的汇合点**：可嵌入 CLI 进程（推荐，无需独立进程、可用进程内 channel），
也可独立进程（各端点用 stdio / socket 连入）。纯无中心的跨进程广播不可靠，不做。

### 分层与依赖（单向无环）

| 层                               | 内容                                        | 依赖                  |
| -------------------------------- | ------------------------------------------- | --------------------- |
| `bridge/protocol/schema.json`    | 帧协议唯一事实源                            | —                     |
| `bridge/protocol/gen.mjs`        | 生成各语言协议绑定 + 端点骨架               | —                     |
| `lania-bridge-protocol`（Rust）  | 生成物薄壳（`include!`）                    | 通用库                |
| `lania-bridge-bus`（Rust）       | **Hub 实现**、`Channel` 传输抽象、路由/广播 | protocol + 通用库     |
| `lania-bridge-core`（Rust）      | `Channel` trait、端点生命周期、资产落盘     | 零 lania 依赖         |
| `lania-bridge-host`（Rust）      | Hub 的进程装配、端点注册表、诊断            | core + bus            |
| `bridge/adaptors/<lang>/`        | 内置端点库（ts/py/go/dart/rust）            | 生成骨架 + 薄手写胶水 |
| `lania-cli` / pi 适配层 / 其它包 | 直接用 `bridge-host` 暴露的 API             | host                  |

`lania-agent` / `lania-agent-core` 主包**仍零 CLI 耦合**，只认 `AgentSessionClient` trait；
总线只是该 trait 的一种可能实现，由适配层注入。

---

## 3. 帧协议（语言中立，唯一事实源）

`bridge/protocol/schema.json` 定义 8 种帧，`kind` 是唯一判别字段：

| 帧            | 字段                                       | 语义                                                     |
| ------------- | ------------------------------------------ | -------------------------------------------------------- |
| `announce`    | `endpointId`, `provides[]`, `subscribes[]` | 端点上线，声明能力与订阅（取代按语言注册）               |
| `subscribe`   | `topic`                                    | 动态订阅                                                 |
| `unsubscribe` | `topic`                                    | 取消订阅                                                 |
| `publish`     | `topic`, `payload`                         | 向主题广播                                               |
| `event`       | `topic`, `payload`, `from`                 | Hub 投递给订阅者；`from` 是 opaque 端点 id，不是语言标签 |
| `request`     | `id`, `capability`, `params`               | 按能力发起请求                                           |
| `response`    | `id`, `result?`, `error?`                  | 响应，`id` 关联回发起方                                  |
| `bye`         | `endpointId`                               | 端点下线                                                 |

**协议层硬约束**：`busInvariant` 明确「帧不含 language / runtime / kind-of-runtime 字段」，
CI 校验生成物与手写 adaptor 均不得引入语言字段或手写骨架关键词
（`stdin` / `requestId` / `json.loads` / `protocolVersion` 等）。

### Hub 行为（Rust）

- 维护：端点表（`endpointId → Channel + provides/subscribes`）、
  能力表（`capability → 端点列表`）、主题表（`topic → 订阅者`）、
  `pending` 表（`request id → 发起方`）。
- 同步 pump 驱动（不绑定 async runtime）：从各 `Channel` 非阻塞取帧并路由。
- `announce` 注册能力/订阅；`publish` 广播 `event` 给所有订阅者（默认不含发布者自身）；
  `request` 按能力选一个 provider（round-robin）转发并记录 `pending`；
  `response` 按 `pending` 送回发起方；`bye` 摘除端点。
- 无 provider 时直接回 `response{error: E_METHOD_NOT_FOUND}`；端点断连自动摘除。
- 多端点提供同一能力时按策略选择（默认 round-robin，可配置）。

### 传输抽象 `Channel`（Rust trait）

只做 `send(BusFrame)` / `try_recv() -> Option<BusFrame>`，不携带语言信息：

| 实现                     | 用途                                              |
| ------------------------ | ------------------------------------------------- |
| 进程内 channel（`mpsc`） | Rust 端点、CLI 内嵌端点、单元测试                 |
| stdio JSON 行            | 任意语言子进程端点（Node / Python / Go / Dart …） |
| socket / WASM / 其它     | 后续扩展，实现同一 trait 即可                     |

---

## 4. 内置端点 adaptor（包内，桥接两端）

| 语言                          | adaptor                              | 形态                               |
| ----------------------------- | ------------------------------------ | ---------------------------------- |
| Rust                          | `lania-bridge-bus` 内的 Rust 端点    | 进程内 channel，直接连 Hub         |
| TS/JS（Node、Bun、Deno 共用） | `bridge/adaptors/ts/endpoint.ts`     | npm 包，stdio 或 socket 连入       |
| Python                        | `bridge/adaptors/py/endpoint.py`     | 单文件，可 `include_str!` 内嵌落盘 |
| Go                            | `bridge/adaptors/go/endpoint.go`     | 单文件 / 编译产物                  |
| Dart                          | `bridge/adaptors/dart/endpoint.dart` | 单文件 / AOT 产物                  |

每个 adaptor 对外只暴露：`announce` / `subscribe` / `publish` / `request` /
`onRequest` / `onEvent`。**帧编解码、id 管理、心跳、错误包装全部由生成器产出**，
语言侧业务只注册两个回调（`onRequest` / `onEvent`），不写一行传输代码。

内聚边界判定：**与语言无关的代码一行都不许出现在语言侧手写目录**。
`scripts/check-bridge-protocol.mjs` 扫描关键词阻断骨架回流。

**新语言接入**：实现 8 种帧的读写（或直接调用包里生成的骨架）连上 Hub 即可，
Hub 与其它端点无需任何改动。这就是「其它语言使用的时候直接调用这个包」。

---

## 5. 接入方式（后续开工时）

```rust
// CLI 或任意包：嵌入 Hub，注册 Rust 端点
let hub = BridgeHub::new();
let cli = hub.attach("cli");                 // opaque id，不含语言
cli.announce(vec!["tool.exec"], vec!["event.agent_*"]);

// 子进程端点（任意语言）用 stdio 通道接入
hub.attach_stdio("engine", child_stdout, child_stdin);

// 通信：只按能力/主题，不出现语言
cli.request("agent.prompt", json!({"text": "..."}));
cli.publish("event.diag", json!({"text": "..."}));
```

`lania-cli` 与 `pi` 适配层、其它 command 包直接依赖 `lania-bridge-host`，
不再 `import lania_py_bridge` / `lania_go_bridge` 等具体语言 crate。

### 与现有代码的过渡（开工时执行，不推翻 28 个消费点）

1. 现有 `NodeBridgeClient` 的 1:1 stdio 直连链路先用 `Channel` 适配成一个端点，
   行为不变，冒烟保持 8/8。
2. `BridgeRegistry` / `RuntimeKind` 从**通信 API** 中移除，退化为
   「启动哪个语言运行时」的装配细节（仅 CLI 组装时可见）。
3. `lania-host` 删掉 5 条语言 bridge 硬依赖，改为依赖 `bridge-host`；
   `Rc<RefCell<BridgeRegistry>>` 换 `Arc` + `RwLock`/`Mutex`。
4. 其余端点逐步迁到总线，`client_for(kind)` 兼容层最后删除。

---

## 6. 约束与未决问题（重新开工前需拍板）

- Hub 是唯一汇合点；纯无中心广播跨进程不可靠，不做。
- 多 provider 的能力冲突、负载均衡与优先级策略需定稿。
- 背压、超时、重连、端点鉴权（谁能声明哪些能力）待定。
- 能力命名空间与 topic 命名规范需与 `agent.*` / `event.agent_*` 现有词汇对齐。
- 「独立 Hub 进程」还是「嵌入 CLI 进程」需结合其它模块的重新设计结果决定。
- 不做 dylib 热插拔（Rust ABI 不稳定）；按需加载用 manifest + 资产落盘。
- 现有 `agent` 会话与总线的边界（谁负责会话持久化）待定。

---

## 7. 暂停前已验证的结论（经验保留，代码已回退）

- 协议单一事实源 + 五语言生成器可行：8 方法 / 29 事件 / envelope / 错误码，
  生成 12 个产物，`--check` 漂移校验、golden 契约、语言侧骨架扫描均通过。
- `Rc<RefCell<BridgeRegistry>>` 改为 `Arc` + 内部锁后，宿主 28 个消费点零改动，
  全 workspace `cargo check` 通过。
- `lania-host` 与 4 个语言 crate 解耦（Provider 全局注册表）可行，
  冒烟 `scripts/smoke-pi.mjs` 8/8 无退化。
- 环境坑（Windows）：`--check` 需按 LF 归一化；pre-commit 的 prettier 会改写
  golden JSON，生成物必须排除格式化；临时目录不能写系统 Temp。

> ⚠️ 以上实现**已全部回退**，仅作为后续开工的已验证依据；不要假设仓库里存在这些代码。

---

## 8. 重新开工检查清单

- [ ] 其它模块重设计定稿，确认不会与桥接层互相影响
- [ ] §6 未决问题拍板（尤其 Hub 部署形态、能力命名空间、鉴权）
- [ ] 按本文重建 `bridge/protocol/schema.json` 与生成器，先跑契约检查
- [ ] 实现 Rust Hub + `Channel`，进程内多端点广播/请求测试全绿
- [ ] 逐个接入内置端点 adaptor，边界扫描 0 违规
- [ ] 过渡现有直连链路，冒烟 `node scripts/smoke-pi.mjs` 保持 8/8
- [ ] `cargo check --workspace` / `cargo test --workspace` / `pnpm check` 全绿
