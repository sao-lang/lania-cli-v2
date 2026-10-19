# Node Bridge

<div class="lead">
Node Bridge 是 Rust 宿主与 JavaScript 生态之间的运行容器。它通过 STDIO JSON 协议接收请求，分发到插件，并回传结果与事件。
</div>

## 协议模型

```text
Rust  ── request(JSON) ─────▶  Node Bridge
Rust  ◀── response(JSON) ────  Node Bridge
Rust  ◀── event(JSON) ───────  Node Bridge
Rust  ◀── host_request(JSON) ─ Node Bridge
Rust  ── host_response(JSON) ▶ Node Bridge
```

协议是逐行 JSON 信封：

```json
{ "type": "request", "payload": { "id": "req-1", "method": "compiler.build", "params": {} } }
```

要点：

- `id` 由 Rust 生成，格式 `req-N`。
- 事件先于对应响应到达。
- 全局事件使用固定 `requestId: "bridge"`。
- Node 串行写 stdout，业务日志不得写入 stdout。
- 握手版本、心跳、超时与背压见 [Bridge 通信协议](/architecture/bridge-protocol)。

## 入口

`ts/packages/node-bridge/src/index.ts` 负责：

- 读取 STDIO 请求。
- 根据 method 找到插件。
- 调用插件 handler。
- 收集结果与事件。
- 写回响应。

## 插件注册

`plugin-registry.ts` 负责：

- 注册内置插件。
- 按 `cwd` 加载项目插件。
- 执行安全策略校验。

内置插件包括：

- compiler
- lint
- template
- dynamic-commands
- product
- system
- contract
- commitlint / commitizen

## 配置加载

Bridge 支持：

- `lan.config.js`
- `lan.config.cjs`
- `lan.config.mjs`
- `lan.config.json`
- `lan.config.ts`
- `lan.config.mts`
- `lan.config.cts`

JS/TS 配置允许对象、default export 或返回对象的函数。加载结果会形成配置快照，供 Rust 消费。字段全集见 [配置参考](/reference/config)。

## 编译器适配

compiler 插件支持：

- Vite
- Webpack
- Rollup

事件示例：

- `event.compiler_start`
- `event.compiler_status`
- `event.compiler_asset`
- `event.compiler_issue`
- `event.compiler_done`

Rust 侧统一消费这些事件，并渲染为进度与日志。

## Lint 适配

lint 插件支持：

- ESLint
- Prettier
- Stylelint
- Textlint
- oxlint / oxfmt

支持 check 与 fix 两种模式，并可限制并发。

## 模板运行时

template 插件提供：

- `template.list`
- `template.getQuestions`
- `template.getDependencies`
- `template.getOutputTasks`
- `template.render`
- `addTemplate.render`

模板资产来自 `@lania-cli/templates`。

## 动态命令

dynamic-commands 插件：

1. 扫描 `lania.schemas.*` 与 `.lania/schemas/`。
2. 解析 `runtimeCommands`。
3. 生成命令规格与 handler 绑定。

完整加载原理见 [命令模型与加载原理](/guide/command-model)，命令声明的全部字段见 [运行时清单参考](/reference/manifest)。

## ctx.tools 能力层

每次调用动态命令或项目插件时，Bridge 会构造一个 `ctx.tools`：

```text
ctx.tools
  ├─ config / workspace / env / json / text / result / path   （Node 本地）
  ├─ bridge / compiler / lint / scaffold                      （Node 内部分发）
  └─ host / exec / git / pm / fs / log / tasks / progress / interaction
                                                             （host_request → Rust）
```

`ctx.tools` 由 `__toolsPolicy` 控制：Node 侧先过滤，Rust 侧对 `exec`、`fs` 等域再次校验。因此即使插件伪造策略，也不能越过宿主边界。

`ctx.tools.bridge` 与 Bridge 的 `request` 不同：它不经过 STDIO，不产生新的 `req-N`，只在 Node 进程内分发，并且只覆盖部分命名空间。4. 在执行时调用项目 handler 或插件方法。

## 产品化

product 插件负责：

- `product.build`
- `product.pack`
- `product.publish`
- `product.inspect`
- `product.doctor`
- `product.generate`

它会生成 build-report、pack-report、publish-report 与 publish-manifest。

## 错误与事件

Bridge 错误会转换为 Rust 侧的 `BridgeResponse.error`。事件则按类型分发：

- 日志
- 进度
- 编译器状态
- Lint 结果
- 开发服务器 URL
- 构建资产

## 下一步

- [插件与 Hook 系统](/architecture/plugin-system)
- [动态命令与插件](/guide/dynamic-commands)
- [Schema 与协议](/reference/schemas)
