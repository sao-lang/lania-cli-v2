# Bridge 方法目录

<div class="lead">
本页列出 Node Bridge 当前公开的全部 RPC 方法。方法名与参数以 Rust 请求构造器和 Node 插件实现为准；协议 envelope 见 [Rust ↔ Node 通信协议](/architecture/bridge-protocol)。
</div>

## 方法总表

```text
bridge.ping
bridge.shutdown
bridge.metrics
bridge.subscribe
bridge.heartbeat
plugins.resolve

config.loadLan
config.loadTool

commands.resolveDynamic
command.invokeDynamic

hooks.invoke
hooks.invokeInline

compiler.dev
compiler.build
compiler.stop

lint.run

system.listCommands

template.list
template.getQuestions
template.getDependencies
template.getOutputTasks
template.render
addTemplate.render

product.generate
product.inspect
product.build
product.pack
product.publish

commitizen.run
commitlint.run
```

## bridge.\*

### bridge.handshake

握手不是普通 request，而是 `type: "handshake"` 的独立 envelope：

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

响应会返回 `bridgeName`、`methods`、`events`、`heartbeatIntervalMs`、`maxPendingEvents`、`failureStrategy`。详见 [Bridge 通信协议](/architecture/bridge-protocol)。

### bridge.ping

```json
{ "id": "req-1", "method": "bridge.ping", "params": {} }
```

返回：

```json
{ "ok": true, "bridgeName": "@lania-cli/node-bridge" }
```

### bridge.heartbeat

返回：

```json
{ "ok": true, "ts": 1234567890 }
```

同时产生 `event.heartbeat`。

### bridge.metrics

参数：

| 字段  | 类型    | 说明                                |
| ----- | ------- | ----------------------------------- |
| `cwd` | string? | 项目目录，用于返回该 cwd 的插件快照 |

返回：

```json
{
  "requests": 10,
  "events": 42,
  "pluginLoads": 3,
  "pluginRejects": 0,
  "pluginErrors": 0,
  "heartbeats": 4,
  "plugins": ["config", "compiler", "..."],
  "rejectedPlugins": [{ "plugin": "...", "reason": "..." }]
}
```

### plugins.resolve

参数：

```json
{ "cwd": "/project" }
```

返回：

```json
{
  "ok": true,
  "cwd": "/project",
  "plugins": ["config", "dynamic-commands", "..."],
  "methods": ["config.loadLan", "..."],
  "rejectedPlugins": []
}
```

### bridge.subscribe

返回：

```json
{
  "accepted": true,
  "events": ["event.ready", "event.log", "..."],
  "mode": "request_response_stream"
}
```

### bridge.shutdown

返回：

```json
{ "accepted": true, "stopped": true }
```

同时产生 `event.shutdown`。

## config.\*

### config.loadLan

参数：

| 字段         | 类型      | 说明             |
| ------------ | --------- | ---------------- |
| `cwd`        | string    | 项目目录         |
| `searchFrom` | string?   | 搜索起点         |
| `candidates` | string[]? | 自定义候选文件名 |

返回：

```json
{
  "cwd": "/project",
  "configPath": "/project/lan.config.ts",
  "exists": true,
  "supportedExtensions": [".js", ".cjs", ".json", ".ts"],
  "config": {},
  "buildTool": "vite",
  "buildAdaptors": {},
  "lintAdaptors": {},
  "lintTools": []
}
```

`config.hooks` 中的函数会被替换成 inline hook 引用：

```json
{ "type": "inline", "id": "inline:lan.config:onSuccess:0", "kind": "parallel" }
```

### config.loadTool

参数：

| 字段   | 类型   |
| ------ | ------ |
| `cwd`  | string |
| `tool` | string |

返回：

```json
{
  "cwd": "/project",
  "tool": "eslint",
  "configPath": "/project/eslint.config.js",
  "exists": true,
  "config": {},
  "resolved": true
}
```

支持的工具键包括：

`vite`、`webpack`、`rollup`、`gulp`、`eslint`、`oxlint`、`prettier`、`oxfmt`、`stylelint`、`textlint`、`markdownlint`、`commitlint`、`commitizen`、`cz`。

## commands.resolveDynamic

参数：

```json
{
  "cwd": "/project",
  "workspaceRoot": "/project",
  "productRoot": "/project",
  "runtimeMode": "development"
}
```

返回：

```json
{
  "commands": [
    /* GeneratedCommandSpec */
  ],
  "handlers": [
    {
      "handlerId": "dynamic.manifest.ops.ping.1",
      "method": "command.invokeDynamic",
      "plugin": "dynamic-commands",
      "target": { "kind": "manifest_command", "...": "..." }
    }
  ],
  "mounts": [{ "mount": "ops", "rootHandlerId": "dynamic.mount.ops" }],
  "warnings": []
}
```

`target.kind` 可能是：

- `mount_summary`
- `group_summary`
- `manifest_command`

## command.invokeDynamic

参数：

```json
{
  "cwd": "/project",
  "workspaceRoot": "/project",
  "productRoot": "/project",
  "runtimeMode": "development",
  "handlerId": "dynamic.manifest.ops.ping.1",
  "traceId": "trace-1",
  "argv": {
    "args": {},
    "options": { "endpoint": "https://example.com" }
  },
  "target": { "kind": "manifest_command", "...": "..." }
}
```

返回：

```json
{
  "kind": "dynamic_manifest_command_result",
  "mount": "ops",
  "path": ["ping"],
  "executor": { "type": "local", "id": "fn:..." },
  "result": { "ok": true },
  "exitCode": 0
}
```

分组节点返回：

```json
{
  "kind": "dynamic_group_summary",
  "mount": "ops",
  "path": ["..."],
  "commands": ["ping", "deploy"],
  "exitCode": 0
}
```

## hooks.invoke / hooks.invokeInline

### hooks.invoke

参数：

| 字段      | 类型                     | 说明                |
| --------- | ------------------------ | ------------------- |
| `cwd`     | string                   | 项目目录            |
| `source`  | string                   | 调用来源            |
| `hook`    | string                   | 20 个 hook key 之一 |
| `plugin`  | string                   | 插件声明名/包名     |
| `handler` | string                   | 插件方法名          |
| `kind`    | `waterfall` / `parallel` | 执行语义            |
| `payload` | any                      | 当前上下文          |

返回：

```json
{
  "ok": true,
  "accepted": true,
  "hook": "onSuccess",
  "plugin": "./lania.plugin.ts",
  "handler": "hooks.audit",
  "payload": { "...": "..." }
}
```

`waterfall` 会返回新的 `payload`；`parallel` 只表示已执行。

### hooks.invokeInline

参数同上，但用 `id` 代替 `plugin/handler`：

```json
{
  "cwd": "/project",
  "source": "host-runtime",
  "hook": "onSuccess",
  "id": "inline:...",
  "kind": "parallel",
  "payload": {}
}
```

返回：

```json
{ "ok": true, "payload": { "...": "..." } }
```

## compiler.\*

### compiler.dev

参数：

| 字段   | 类型    |
| ------ | ------- |
| `cwd`  | string  |
| `port` | number? |
| `host` | string? |
| `hmr`  | bool?   |
| `mode` | string? |

返回：

```json
{
  "accepted": true,
  "tool": "vite",
  "url": "http://127.0.0.1:8089",
  "implementation": "runtime",
  "workerMode": "isolated_worker"
}
```

事件：`event.compiler_start`、`event.compiler_status`、`event.compiler_server_ready`、`event.dev_url`、`event.compiler_done`。

### compiler.build

参数：

| 字段        | 类型                          |
| ----------- | ----------------------------- |
| `cwd`       | string                        |
| `watch`     | bool                          |
| `mode`      | `development` / `production`? |
| `outputDir` | string?                       |
| `config`    | string?                       |
| `path`      | string?                       |

事件：`event.compiler_start`、`event.compiler_status`、`event.compiler_asset`、`event.build_asset`、`event.compiler_issue`、`event.compiler_done`。

### compiler.stop

停止当前编译器。

## lint.run

参数：

| 字段            | 类型            | 说明                                              |
| --------------- | --------------- | ------------------------------------------------- |
| `cwd`           | string          | 项目目录                                          |
| `fix`           | bool            | 是否修复                                          |
| `mode`          | `check` / `fix` | 模式                                              |
| `concurrency`   | number?         | 并发                                              |
| `linters`       | string[]?       | `oxlint,eslint,oxfmt,prettier,stylelint,textlint` |
| `groupedOutput` | bool?           | 分组输出                                          |

返回关键字段：

```json
{
  "accepted": true,
  "formatter": "lania.lint.formatter.v1",
  "normalizer": "lania.lint.normalizer.v1",
  "mode": "check",
  "concurrency": 4,
  "summaryText": "...",
  "summaryByAdaptor": {},
  "resultsByAdaptor": {},
  "exitCode": 1
}
```

事件：`event.lint_start`、`event.lint_file`、`event.lint_result`、`event.lint_summary`。

## system.listCommands

参数：

| 字段           | 类型    |
| -------------- | ------- |
| `cwd`          | string  |
| `filter`       | string? |
| `limit`        | number? |
| `allMatches`   | bool    |
| `includeShell` | bool    |

返回：

```json
{
  "accepted": true,
  "kind": "system_commands",
  "scope": "environment",
  "platform": "win32",
  "shell": "C:\\Program Files\\Git\\bin\\bash.exe",
  "shellName": "bash",
  "shellSupported": true,
  "includeShell": true,
  "cwd": "/project",
  "filter": null,
  "limit": null,
  "allMatches": false,
  "notes": [],
  "summary": {
    "pathEntries": 120,
    "scannedDirs": 50,
    "unique": 1039,
    "duplicates": 12,
    "shellBuiltins": 30,
    "shellAliases": 4,
    "shellFunctions": 0,
    "matched": 1039,
    "returned": 1039
  },
  "commands": [
    {
      "name": "node",
      "path": "/usr/bin/node",
      "source": "path"
    }
  ],
  "duplicates": [{ "name": "node", "paths": ["/a/node", "/b/node"] }]
}
```

## template.\*

### template.list

参数：

```json
{ "cwd": "/project" }
```

返回：

```json
{
  "templates": ["spa-react", "spa-vue", "toolkit", "toolkit-monorepo"],
  "metadata": [
    {
      "name": "spa-vue",
      "title": "spa-vue",
      "description": null,
      "tags": [],
      "schemaVersion": 1,
      "renderEngine": "rust_declarative",
      "legacyTemplateDir": "packages/templates/spa-vue",
      "ownership": "rust_preferred_node_fallback",
      "useCases": ["create", "add"],
      "migrationLayer": "rust_builtin_template"
    }
  ],
  "usage": { "list": "lan template", "detail": "lan template <name>" }
}
```

### template.getQuestions

参数：

```json
{ "template": "spa-vue", "options": {} }
```

返回：

```json
{ "template": "spa-vue", "questions": [] }
```

### template.getDependencies

```json
{ "template": "spa-vue", "options": {} }
```

返回：

```json
{
  "template": "spa-vue",
  "dependencies": {},
  "devDependencies": {}
}
```

### template.getOutputTasks

返回：

```json
{ "template": "spa-vue", "tasks": [] }
```

### template.render

参数：

```json
{
  "template": "spa-vue",
  "context": {},
  "options": {}
}
```

返回：

```json
{
  "template": "spa-vue",
  "schemaVersion": 1,
  "renderEngine": "node_bridge",
  "files": [{ "path": "src/main.ts", "content": "..." }]
}
```

模板渲染事件由 plugin wrapper 通过 `event.log` 追加，不在 result 内。

### addTemplate.render

参数：

```json
{
  "template": "v3",
  "context": {
    "projectName": "button",
    "language": "ts",
    "cssProcessor": "scss"
  }
}
```

返回：

```json
{
  "template": "v3",
  "schemaVersion": 1,
  "label": "vue3模板组件",
  "filename": null,
  "extname": "vue",
  "content": "..."
}
```

## product.\*

所有 `product.*` 共享：

| 字段    | 类型    | 说明                        |
| ------- | ------- | --------------------------- |
| `cwd`   | string  | 产品根目录                  |
| `path`  | string? | 原始路径参数                |
| `clean` | bool    | 是否清理输出目录，默认 true |

### product.generate

额外参数：

| 字段          | 类型               |
| ------------- | ------------------ |
| `preset`      | `demo` / `minimal` |
| `name`        | string?            |
| `binaryName`  | string?            |
| `packageName` | string?            |
| `outputDir`   | string?            |
| `force`       | bool?              |

返回包含：

- `accepted`
- `generatedFiles`
- `checks`
- `experimental.templateId`
- `experimental.binaryName`
- `experimental.packageName`

### product.inspect

额外参数：

| 字段          | 类型   |
| ------------- | ------ |
| `compat`      | bool?  |
| `doctor`      | bool?  |
| `hostVersion` | string |

返回包含：

- `accepted`
- `product`
- `schema`
- `templates`
- `artifacts`（build/pack/publish）
- `checks`
- `compat`（可选）

### product.build

额外参数：

| 字段        | 类型   | 默认                   |
| ----------- | ------ | ---------------------- |
| `outputDir` | string | `.lania/build/product` |

返回关键字段：

```json
{
  "accepted": true,
  "kind": "product_build",
  "mode": "snapshot",
  "outputRoot": ".lania/build/product",
  "reportPath": "build-report.json",
  "checks": {
    "hasConfigPath": true,
    "hasPackageJson": true,
    "hasSchemaEntries": true,
    "hasSchemaRoots": true,
    "hasTemplatesDir": true
  }
}
```

### product.pack

额外参数：

| 字段        | 类型   | 默认                               |
| ----------- | ------ | ---------------------------------- |
| `buildDir`  | string | `.lania/build/product`             |
| `outputDir` | string | `.lania/pack/product/install-root` |

返回关键字段：

```json
{
  "accepted": true,
  "kind": "product_pack",
  "mode": "install_root",
  "wrapper": "./bin/acme",
  "productRoot": "./lib/product",
  "nodeBridgeDir": "./lib/node-bridge",
  "checks": {
    "hasBuildReport": true,
    "hasNodeBridgePayload": true,
    "hasProductConfig": true,
    "hasWrapper": true
  }
}
```

### product.publish

额外参数：

| 字段                  | 类型         | 默认                                 |
| --------------------- | ------------ | ------------------------------------ |
| `packDir`             | string       | `.lania/pack/product/install-root`   |
| `outputDir`           | string       | `.lania/publish/product/npm-package` |
| `distTag`             | string?      |                                      |
| `channel`             | string?      |                                      |
| `registry`            | string?      |                                      |
| `platformBinariesDir` | string?      |                                      |
| `platformBinaryPaths` | JSON string? |                                      |
| `execute`             | bool         | false                                |
| `dryRun`              | bool         | false                                |
| `yes`                 | bool         | false                                |
| `resume`              | bool         | false                                |
| `otp`                 | string?      |                                      |
| `npmBin`              | string?      |                                      |
| `maxRetries`          | string?      |                                      |
| `retryDelayMs`        | string?      |                                      |
| `rollbackOnFailure`   | bool         | false                                |

返回关键字段：

```json
{
  "accepted": true,
  "kind": "product_publish",
  "mode": "npm_package",
  "wrapper": "./bin/acme.mjs",
  "tarball": "./acme-demo-1.2.3.tgz",
  "bundle": {
    "root": "./bundle",
    "cliTarball": "./bundle/cli-package/lania-cli-cli-0.1.0.tgz",
    "platformTarball": null,
    "platformTarballs": [],
    "platformMatrix": []
  },
  "checks": {},
  "experimental": {
    "packageName": "@acme/demo",
    "packageVersion": "1.2.3",
    "binaryName": "acme"
  }
}
```

## commitizen.run

参数：

| 字段      | 类型    |
| --------- | ------- |
| `cwd`     | string  |
| `kind`    | string  |
| `scope`   | string? |
| `subject` | string  |

返回包含最终的 `message` 与配置信息。

## commitlint.run

参数：

| 字段      | 类型   |
| --------- | ------ |
| `cwd`     | string |
| `message` | string |

返回：

```json
{
  "accepted": true,
  "tool": "commitlint",
  "message": "feat: add api",
  "valid": true,
  "configLoaded": true,
  "configPath": "/project/commitlint.config.cjs",
  "errors": []
}
```

## 源码位置

| 领域          | 文件                                                             |
| ------------- | ---------------------------------------------------------------- |
| Rust 请求构造 | `rust/crates/lania-node-bridge/src/client/transport/requests.rs` |
| 协议模型      | `rust/crates/lania-node-bridge/src/protocol.rs`                  |
| Node 分发     | `ts/packages/node-bridge/src/entry/index.ts`                     |
| 插件注册      | `ts/packages/node-bridge/src/core/plugin-registry.ts`            |
| 内建方法      | `ts/packages/node-bridge/src/entry/builtin-handlers.ts`          |
| 动态命令      | `ts/packages/node-bridge/src/plugins/dynamic-commands/**`        |
| 产品命令      | `ts/packages/node-bridge/src/plugins/product/**`                 |

## 下一步

- [Rust ↔ Node 通信协议](/architecture/bridge-protocol)
- [ctx.tools API](/reference/host-tools)
- [运行时清单参考](/reference/manifest)
