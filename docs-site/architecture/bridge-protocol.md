# Rust ↔ Node 通信协议

<div class="lead">
Rust 宿主与 Node Bridge 是两个独立进程，通过 stdin/stdout 上的 JSONL envelope 通信。除了普通的 request/response，还有三类消息：事件、心跳/握手、以及 Node 反向调用 Rust 的 Host RPC。
</div>

## 1. 进程生命周期

### 谁启动谁

```text
npm/cli/bin/lan.mjs
  → 启动 Rust 二进制 lania-cli
  → 注入 LANIA_NODE_BRIDGE_DIR

Rust HostRuntime
  → 首次需要 JS 能力时启动 Node 子进程
  → node --import tsx src/entry/stdio.ts
    或 node --import tsx dist/entry/stdio.js
```

Node 子进程的工作目录是 Node Bridge 包目录，stdin/stdout/stderr 全部由 Rust 接管。

### Bridge 目录解析

Rust 按以下顺序查找 Bridge：

1. `LANIA_NODE_BRIDGE_DIR`
2. `<current-exe>/../lib/node-bridge`
3. `<current-exe>/../node-bridge`
4. `<current-exe>/../Resources/node-bridge`
5. 开发态 `ts/packages/node-bridge`

### 启动模式

| 模式             | 条件                       | 入口           |
| ---------------- | -------------------------- | -------------- |
| `dev_source`     | `src/entry/stdio.ts` 存在  | 直接跑 TS 源码 |
| `installed_dist` | `dist/entry/stdio.js` 存在 | 跑构建产物     |

两种模式都使用 `--import tsx`，因为模板资源仍可能以 `.ts` 形式存在。

### 关闭

Rust 发送：

```json
{ "type": "request", "payload": { "id": "req-N", "method": "bridge.shutdown", "params": {} } }
```

然后：

1. 失败所有 in-flight 请求
2. kill 子进程
3. 回收 reader 线程

shutdown 是 best-effort：即使 `bridge.shutdown` 请求失败，也会继续 kill 进程。

## 2. 传输格式

### 一行一个 envelope

stdout / stdin 都遵循 JSONL：

```text
<一行 JSON>\n
<一行 JSON>\n
```

任何 `console.log` 都可能污染协议。Node 侧必须保证 stdout 只输出协议 envelope。

### 六种 envelope

#### handshake

```json
{
  "type": "handshake",
  "payload": {
    "protocolVersion": "0.1.0",
    "transport": "stdio",
    "encoding": "json",
    "hostName": "lania-host"
  }
}
```

Node 返回：

```json
{
  "type": "handshake",
  "payload": {
    "protocolVersion": "0.1.0",
    "bridgeName": "@lania-cli/node-bridge",
    "methods": ["bridge.ping", "..."],
    "events": ["event.ready", "..."],
    "heartbeatIntervalMs": 10000,
    "maxPendingEvents": 32,
    "failureStrategy": "reconnect"
  }
}
```

#### request（Rust → Node）

```json
{
  "type": "request",
  "payload": {
    "id": "req-1",
    "method": "compiler.build",
    "params": { "cwd": "...", "watch": false }
  }
}
```

#### response（Node → Rust）

```json
{
  "type": "response",
  "payload": {
    "id": "req-1",
    "result": { "...": "..." },
    "error": null
  }
}
```

#### event（Node → Rust）

```json
{
  "type": "event",
  "requestId": "req-1",
  "payload": {
    "method": "event.progress",
    "params": { "current": 1, "total": 4, "message": "Building" }
  }
}
```

`requestId` 为 `bridge` 时表示全局事件（`event.ready`、`event.heartbeat`）。

#### host_request（Node → Rust）

```json
{
  "type": "host_request",
  "payload": {
    "id": "host-req-...",
    "method": "host.fs.write",
    "params": { "cwd": "...", "path": "a.txt", "content": "..." }
  }
}
```

#### host_response（Rust → Node）

```json
{
  "type": "host_response",
  "payload": {
    "id": "host-req-...",
    "result": { "written": true },
    "error": null,
    "events": []
  }
}
```

## 3. 一次请求的完整流程

```text
Rust:
  1. ensure_process_locked()
     - 进程不存在/已退出/心跳超时 → 重新 spawn + handshake
  2. 检查 pending 数是否达到 max_pending_requests
  3. 创建 event channel + response oneshot
  4. 把 request.id 写入 pending
  5. 写 {"type":"request","payload":...}

Node:
  6. readline 读一行
  7. handleExchange(request)
  8. 先写所有 events
  9. 最后写 response

Rust:
 10. stdout reader 按 request.id 路由 event
 11. response 到达时完成 oneshot
 12. collect_exchange() 收尾
```

### 顺序保证

- 同一个 request 的 events **必须早于** response。
- Node 侧写 envelope 通过串行 `writeChain`，避免并发 handler 交错写坏 JSONL。
- 多个 request 可以并发处理；每个 request 的事件按 `requestId` 隔离。
- Rust 在 response 到达后仍会尝试排空剩余 events，避免最后的尾流丢失。

## 4. 并发、超时与重连

### Rust 客户端默认配置

| 配置                       | 默认值  |
| -------------------------- | ------- |
| `protocolVersion`          | `0.1.0` |
| `transport`                | `stdio` |
| `encoding`                 | `json`  |
| `timeout`                  | 30s     |
| `prefer_process_transport` | true    |
| `event_buffer_capacity`    | 32      |
| `max_pending_requests`     | 32      |
| `heartbeat_timeout`        | 45s     |

### Node 心跳

Node 每 10s 发送：

```json
{ "method": "event.heartbeat", "params": { "ts": 1234567890 } }
```

Rust 更新 `last_heartbeat`。如果 `failureStrategy = reconnect` 且超过 45s 没收到心跳，下次请求前会重启 bridge。

### 背压

pending 请求数达到上限时：

```text
bridge backpressure limit reached: 32 pending requests
```

请求不会写入 stdin，也不会留下半残 pending 记录。

### 重试

上层 `CommandExecutionContext` 会根据 `ExecutionPolicy` 对 bridge request 做重试：

- response.error 且还有重试次数 → 重试
- 请求在收到 response 前失败 → 重试
- 超时 → 重试
- 用尽重试 → `EXIT_RUNTIME_ERROR` / `EXIT_TIMEOUT`

### Mock 回退

`NodeBridgeClient::call()` / `call_async()` 在 process transport 不可用时可能回退到 mock：

- `call()`：同步入口，错误时回退 mock
- `call_async()`：`open_call` 失败时回退 mock

因此看到结构稳定但明显“假”的结果时，先确认 `using_process_transport()` 和 `bridge.metrics`。

## 5. Node 侧请求分发

```text
handleExchange(request)
  → registry.metrics.recordRequest()
  → handleBuiltinRequest()
      命中 bridge.* / plugins.resolve → 直接返回
  → handlePluginRequest()
      1. registry.resolve(cwd)
      2. 检查 method 是否在握手方法列表中
      3. registry.resolvePlugin(method, cwd)
      4. plugin.handle(method, params, { cwd })
```

### 方法不存在

```json
{
  "id": "req-1",
  "error": {
    "code": "E_METHOD_NOT_FOUND",
    "message": "Unsupported method: foo.bar"
  }
}
```

### 插件运行时异常

```json
{
  "id": "req-1",
  "error": {
    "code": "E_PLUGIN_RUNTIME",
    "message": "..."
  }
}
```

### 插件返回空

如果插件返回 `null/undefined`，Bridge 会返回：

```json
{
  "result": {
    "accepted": true,
    "method": "foo.bar",
    "params": {}
  }
}
```

## 6. 事件模型

| 事件                          | 关键参数                                           | Rust 行为            |
| ----------------------------- | -------------------------------------------------- | -------------------- |
| `event.ready`                 | `plugins`                                          | 记录已加载插件       |
| `event.log`                   | `level`, `message`, `source`                       | 写入 logger          |
| `event.progress`              | `current`, `total`, `message`                      | 更新进度             |
| `event.dev_url`               | `url`                                              | 输出 dev server 地址 |
| `event.build_asset`           | `file`, `bytes`                                    | 记录构建产物         |
| `event.compiler_start`        | `tool`, `action`, `workerMode`, `isolated`         | 启动进度             |
| `event.compiler_status`       | `stage`, `message`                                 | 更新进度 + 日志      |
| `event.compiler_server_ready` | `url`, `host`, `port`                              | 记录 server ready    |
| `event.compiler_asset`        | `file`, `bytes`, `outputDir`                       | 记录产物             |
| `event.compiler_issue`        | `severity`, `message`                              | error/warn 日志      |
| `event.compiler_watch_change` | `path`                                             | 记录 watch 变化      |
| `event.compiler_done`         | `tool`, `action`, `success`, `implementation`, ... | 结束进度             |
| `event.lint_start`            | `adaptors`, `cwd`, `mode`, `concurrency`           | 记录 lint 开始       |
| `event.lint_file`             | `adaptor`, `filePath`, `errors`, `warnings`        | 记录单文件结果       |
| `event.lint_result`           | `adaptor`, `result`                                | 记录适配器结果       |
| `event.lint_summary`          | `errors`, `warnings`, `exitCode`                   | 汇总                 |
| `event.watch_change`          | `path`                                             | watch 变化           |
| `event.shutdown`              | `reason`                                           | 关闭事件             |
| `event.heartbeat`             | `ts`                                               | 心跳                 |

### 事件输出

`ui.output.events` 已移除：CLI 不再把 bridge 事件实时转发到终端，只在命令结束时输出一次结果。
事件仍会进入宿主日志/进度等内部状态，但不会出现在最终 stdout/stderr。

## 7. Host RPC：Node 反调 Rust

`ctx.tools` 的很多能力最终由 Rust 实现，走 Host RPC。

### 传输

Node 侧 `core/host-rpc.ts`：

- 写 `host_request` 到 stdout
- 等待 stdin 上的 `host_response`
- 默认 30s 超时
- 超时错误：`E_HOST_TIMEOUT`

Rust 侧 `runtime/host_rpc.rs`：

- reader 线程收到 `host_request`
- 调用 `HostRpcHandler`
- 写回 `host_response`
- 如果 handler 不存在：`E_HOST_RPC_UNAVAILABLE`
- 如果 handler 抛错：`E_HOST_RPC`

### 方法域

| 前缀                 | 能力                                                  |
| -------------------- | ----------------------------------------------------- |
| `host.exec.*`        | 执行命令、shell、历史                                 |
| `host.fs.*`          | 读写、目录、stat、remove                              |
| `host.git.*`         | status、branch、stage、commit、remote、tag、revert 等 |
| `host.pm.*`          | 包管理器检测、spec、脚本存在性、命令规划              |
| `host.log.*`         | emit、entries、clear、ascii                           |
| `host.tasks.*`       | 任务注册与状态                                        |
| `host.progress.*`    | 进度开始/推进/完成/渲染                               |
| `host.interaction.*` | input/confirm/select/editor/flow                      |

### 策略透传

Node 调用 Host RPC 时会附带：

```json
{
  "__toolsPolicy": {
    "exec": { "allowShell": true, "allowEnvWrite": true },
    "fs": { "writeRoot": "." }
  }
}
```

Rust 侧再次强制执行：

- `exec.allowShell=false` 时拒绝 shell
- `exec.allowEnvWrite=false` 时拒绝 env
- `fs.writeRoot` 之外拒绝写入

完整方法列表见 [Host RPC 方法目录](/reference/host-rpc)、[Bridge 方法目录](/reference/bridge-methods) 与 [ctx.tools API](/reference/host-tools)。

## 8. 错误码与退出码

### Bridge 错误码

| 错误码                   | 来源                                  |
| ------------------------ | ------------------------------------- |
| `E_METHOD_NOT_FOUND`     | Bridge 不支持该方法                   |
| `E_PLUGIN_RUNTIME`       | 插件执行异常                          |
| `E_HOST_RPC`             | Host RPC handler 执行失败             |
| `E_HOST_RPC_UNAVAILABLE` | 没有安装 Host RPC handler             |
| `E_BRIDGE_CLOSED`        | stdout reader 终止，所有 pending 失败 |
| `E_TIMEOUT`              | Bridge 错误对象中的超时               |
| `E_HOST_TIMEOUT`         | Node Host RPC 超时                    |
| `E_TOOLS_DENIED`         | ctx.tools 策略拒绝                    |

### 进程退出码

| 退出码 | 含义                              |
| ------ | --------------------------------- |
| `0`    | 成功                              |
| `1`    | 通用失败 / lint 失败              |
| `2`    | 运行时错误、参数错误、bridge 错误 |
| `124`  | timeout（`EXIT_TIMEOUT`）         |
| `130`  | cancelled（`EXIT_CANCELLED`）     |

具体以 `lania-host::execution` 中的常量为准。

## 9. stderr 处理

默认情况下 Rust 会消费但丢弃 Node stderr，避免 pipe 满导致子进程阻塞。

设置：

```powershell
$env:LANIA_BRIDGE_PASSTHROUGH_STDERR = "1"
```

后，stderr 每一行会被降级为 `event.log`：

- 包含 `ERROR` / 以 `error` 开头 → error
- 包含 `WARN` / 以 `warn` 开头 → warn
- 其他 → info

这些事件会广播给所有 in-flight 请求，因为 stderr 没有 requestId。

## 10. 关键环境变量

| 变量                              | 作用                            |
| --------------------------------- | ------------------------------- |
| `LANIA_NODE_BRIDGE_DIR`           | 强制 Bridge 目录                |
| `LANIA_NODE_BIN`                  | 指定 Node 可执行文件            |
| `LANIA_BRIDGE_PASSTHROUGH_STDERR` | 是否把 Node stderr 转成日志事件 |
| `LANIA_PROMPT_ANSWERS_JSON`       | CI 场景下的脚本化 prompt 答案   |
| `LANIA_TRACE` / `LANIA_LOG_LEVEL` | 打开 trace 级宿主日志           |
| `LANIA_PRODUCT_ROOT`              | 产品运行时根目录                |
| `LANIA_RUNTIME_MODE`              | `development` / `installed`     |

完整列表见 [环境变量参考](/reference/environment)。

## 下一步

- [Host RPC 方法目录](/reference/host-rpc)
- [Bridge 方法目录](/reference/bridge-methods)
- [ctx.tools API](/reference/host-tools)
- [Node Bridge 模块](/architecture/node-bridge)
