# Phase 0 Node Bridge 协议冻结

## 协议定位

Node Bridge 不是临时兼容层，而是 v2 正式运行时成员。它承接 Node 生态执行能力，Rust Host 只通过稳定协议调用。

## 传输与编码

- 传输方式：`STDIO`
- 编码格式：`JSON`
- 协议风格：类 `JSON-RPC`

## 运行时依赖模型

- `Node Bridge` 不内置固定版本的 `vite/webpack/rollup/eslint/prettier` 作为默认执行运行时。
- `compiler` / `lint` 相关方法优先从目标项目的 `cwd` 动态加载本地依赖。
- `Bridge` 只负责：
  - 读取项目配置
  - 定位本地工具模块
  - 调用工具 API
  - 把结果转换为稳定协议事件和响应
- 目标项目负责：
  - 声明并安装需要的构建 / lint 工具
  - 提供 `vite.config.*` / `webpack.config.*` / `eslint` / `prettier` 配置
- 这样可以避免 CLI 固定绑定单一工具版本，并保持与项目自身工具链一致。
- 当前仓库实现中，这个模型对应：
  - `packages/node-bridge/src/runtime.ts` 的 `resolveModuleFromCwd()`
  - `packages/node-bridge/src/plugins/compiler.ts`
  - `packages/node-bridge/src/plugins/lint.ts`

## 握手

Host 发起：

```json
{
  "protocolVersion": "0.1.0",
  "transport": "stdio",
  "encoding": "json",
  "hostName": "lania-host"
}
```

Bridge 返回：

```json
{
  "protocolVersion": "0.1.0",
  "bridgeName": "@lania/node-bridge",
  "methods": ["bridge.ping", "config.loadLan"],
  "events": ["event.ready", "event.log"]
}
```

## Request / Response

请求：

```json
{
  "id": "req-1",
  "method": "compiler.build",
  "params": {}
}
```

成功响应：

```json
{
  "id": "req-1",
  "result": {
    "accepted": true
  }
}
```

失败响应：

```json
{
  "id": "req-1",
  "error": {
    "code": "E_METHOD_NOT_FOUND",
    "message": "Unsupported method"
  }
}
```

## 冻结的方法集合

- `bridge.ping`
- `config.loadLan`
- `config.loadTool`
- `compiler.dev`
- `compiler.build`
- `compiler.stop`
- `lint.run`
- `system.listCommands`
- `template.list`
- `template.getQuestions`
- `template.getDependencies`
- `template.getOutputTasks`
- `template.render`
- `commitizen.run`
- `commitlint.run`

## 冻结的事件集合

- `event.ready`
- `event.log`
- `event.progress`
- `event.dev_url`
- `event.build_asset`
- `event.lint_result`
- `event.watch_change`
- `event.shutdown`

## 错误模型

- `code`
  - 机器可读错误码。
- `message`
  - 面向用户的错误说明。
- `data`
  - 可选调试上下文。

## 超时与取消策略

- Host 对每个 request 持有超时策略。
- Ctrl-C 第一次触发受控 `stop/shutdown`。
- Ctrl-C 第二次允许强制结束 Bridge 子进程。

## 当前骨架实现

当前仓库中的 `packages/node-bridge/src/index.ts` 提供：

- 握手响应构造 `createHandshakeResponse()`
- `bridge.ping` 的最小处理 `handleRequest()`
- `event.ready` 的静态事件构造 `readyEvent()`

这保证 Phase 0 先冻结协议和结构，再进入 Phase 1 的真实执行实现。
