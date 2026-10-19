# 配置字段参考

<div class="lead">
本页覆盖 lan.config.* 的加载顺序、Rust 快照字段、Node 额外读取字段、默认值、校验行为，以及全局偏好与工具配置。字段以当前实现为准。
</div>

## 1. 文件发现

Node Bridge 的 `loadLanConfig` 按以下顺序查找第一个存在的文件：

```text
lan.config.js
lan.config.cjs
lan.config.mjs
lan.config.json
lan.config.ts
lan.config.mts
lan.config.cts
```

### 解析方式

| 扩展名                                            | 解析                 |
| ------------------------------------------------- | -------------------- |
| `.json` / `.jsonc`                                | 先 JSON，失败再 YAML |
| `.yaml` / `.yml` / `.rc` / 无扩展名               | JSON → YAML          |
| `.js` / `.cjs` / `.mjs` / `.ts` / `.mts` / `.cts` | 动态 import          |

模块可以：

- `export default { ... }`
- 导出配置对象
- 导出函数：`({}) => ({ ... })`，CLI 会调用它

> 加载可执行配置等同于执行项目代码，只应用于可信项目。

## 2. Rust 快照

`LanConfigSnapshot` 是 Rust 侧消费的强类型视图：

| 字段                         | 类型              | 说明                              |
| ---------------------------- | ----------------- | --------------------------------- |
| `cwd`                        | string            | 加载目录                          |
| `configPath`                 | string?           | 命中的配置文件                    |
| `exists`                     | bool              | 是否存在                          |
| `supportedExtensions`        | string[]          | 声明支持的扩展名                  |
| `buildTool`                  | string            | 构建工具，默认 `vite`             |
| `buildAdaptors`              | object            | 构建适配器配置                    |
| `lintAdaptors`               | object            | Lint 适配器配置                   |
| `lintTools`                  | string[]          | Lint 工具列表                     |
| `plugins`                    | ConfigPluginRef[] | 插件声明                          |
| `extensions.dynamicCommands` | bool              | 动态命令开关                      |
| `ui.locale`                  | string?           | 项目语言                          |
| `ui.output`                  | object            | 输出配置                          |
| `ui.progress`                | object            | 进度配置                          |
| `ui.interaction`             | object            | 交互配置                          |
| `schemaDiscovery`            | object            | manifest 发现                     |
| `commands`                   | object            | aliases / shortcuts（当前未接线） |
| `hooks`                      | object            | 全局 Hook                         |
| `release`                    | object?           | 发布配置                          |
| `custom`                     | object            | 自定义扩展字段                    |
| `schemaVersion`              | u32               | 配置版本                          |
| `versionStrategy`            | object            | 版本兼容策略                      |
| `validationErrors`           | array             | 校验诊断                          |
| `raw`                        | any               | 原始配置                          |

## 3. 顶层白名单

Rust 校验器当前接受的顶层字段：

```text
version
buildTool
buildAdaptors
lintAdaptors
lintTools
plugins
extensions
ui
schemaDiscovery
commands
hooks
release
custom
pluginAllowlist
pluginMethodAllowlist
pluginTrustedSources
pluginRequireSignature
pluginSignatureAllowlist
```

### 重要：Node 额外读取但不在 Rust 白名单中的字段

以下字段会被 Node Bridge 读取并生效：

```text
product
schema
tools（或旧名 tooling）
templateDirs
templateRuntimeDirs
config.release（兼容形态）
```

由于 Rust 校验器未知这些字段，`validation_errors` 中可能出现 `unknown lan.config field`。**校验不会阻断运行**，它是诊断信息，不是硬失败。

### 未知字段

未知顶层字段会产生：

```json
{
  "code": "unknown_field",
  "path": "$.foo",
  "message": "unknown lan.config field `foo`"
}
```

不会阻止命令执行。

## 4. version

```ts
export default {
  version: 1,
};
```

当前支持版本：`1`。

大于当前版本会产生 `unsupported_version` 诊断。

## 5. buildTool / buildAdaptors / lintTools / lintAdaptors

```ts
export default {
  buildTool: 'vite',
  buildAdaptors: {
    // 适配器自定义配置
  },
  lintTools: ['eslint', 'prettier'],
  lintAdaptors: {
    // 适配器自定义配置
  },
};
```

`buildTool` 默认 `vite`。`lintTools` 必须是字符串数组。

## 6. extensions

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
};
```

| 字段              | 类型 | 说明               |
| ----------------- | ---- | ------------------ |
| `dynamicCommands` | bool | 是否启用运行时命令 |

## 7. schemaDiscovery

```ts
export default {
  schemaDiscovery: {
    files: ['lania.schemas.ts', 'lania.schemas.js', 'lania.schemas.cjs'],
    dirs: ['.lania/schemas'],
    allowExtensions: ['.ts', '.js', '.cjs', '.json', '.yaml', '.yml'],
  },
};
```

默认值：

| 字段              | 默认                                                |
| ----------------- | --------------------------------------------------- |
| `files`           | `lania.schemas.ts` / `.js` / `.cjs`                 |
| `dirs`            | `.lania/schemas`                                    |
| `allowExtensions` | `.ts` / `.js` / `.cjs` / `.json` / `.yaml` / `.yml` |

## 8. schema / product

```ts
export default {
  schema: {
    entry: './lania.schemas.ts',
    discovery: {
      files: ['lania.schemas.ts'],
      dirs: ['.lania/schemas'],
      allowExtensions: ['.ts', '.js'],
    },
  },
  product: {
    name: '@acme/cli',
    binaryName: 'acme',
    displayName: 'Acme CLI',
    version: '1.0.0',
    versionStrategy: 'package_json',
    schemaEntry: './product/lania.schemas.ts',
    templatesDir: './product/templates',
    compat: {
      frameworkVersionRange: '>=0.1.0 <1.0.0',
      protocolVersionRange: '0.1.0',
      nodeBridgeVersionRange: '0.1.0',
      productVersionRange: '>=1.0.0',
    },
    publish: {},
  },
};
```

| 字段                      | 说明                                      |
| ------------------------- | ----------------------------------------- |
| `schema.entry`            | 显式 manifest 入口，string 或 string[]    |
| `schema.discovery`        | 覆盖顶层 `schemaDiscovery`                |
| `product.name`            | npm 包名                                  |
| `product.binaryName`      | 产品命令名                                |
| `product.displayName`     | 展示名                                    |
| `product.version`         | 显式版本                                  |
| `product.versionStrategy` | `package_json` 时从 package.json 回填版本 |
| `product.schemaEntry`     | 产品 schema 入口                          |
| `product.templatesDir`    | 产品模板根目录                            |
| `product.compat`          | 兼容范围                                  |
| `product.publish`         | 产品发布配置                              |

## 9. tools

`tools`（旧名 `tooling`）控制 ctx.tools 策略：

```ts
export default {
  tools: {
    allow: [
      'config',
      'bridge',
      'compiler',
      'lint',
      'path',
      'workspace',
      'env',
      'json',
      'result',
      'text',
      'host',
      'exec',
      'git',
      'pm',
      'scaffold',
      'fs',
      'log',
      'tasks',
      'progress',
      'interaction',
    ],
    deny: [],
    exec: {
      allowShell: true,
      allowEnvWrite: true,
    },
    fs: {
      writeRoot: '.',
    },
    bridge: {
      allowMethods: ['bridge.*', 'config.*', 'compiler.*', 'lint.*'],
    },
  },
};
```

| 字段                  | 默认   | 说明                             |
| --------------------- | ------ | -------------------------------- |
| `allow`               | 不限制 | 允许的命名空间                   |
| `deny`                | 空     | 禁止的命名空间，优先级高于 allow |
| `exec.allowShell`     | true   | 是否允许 shell                   |
| `exec.allowEnvWrite`  | true   | 是否允许注入 env                 |
| `fs.writeRoot`        | `.`    | 写操作根目录                     |
| `bridge.allowMethods` | 不限制 | Bridge 方法 glob 白名单          |

`path`、`env`、`text`、`result` 不经过 allow 校验。

## 10. plugins 与安全

```ts
export default {
  plugins: [
    './lania.plugin.ts',
    {
      name: 'team-plugin',
      package: './lania.plugin.ts',
      methods: ['commands.deploy', 'hooks.audit'],
      signature: 'optional-string',
    },
  ],
  pluginAllowlist: ['@scope/plugin'],
  pluginMethodAllowlist: ['commands.deploy', 'hooks.audit'],
  pluginTrustedSources: ['package', 'local_path'],
  pluginRequireSignature: false,
  pluginSignatureAllowlist: ['@scope/plugin'],
};
```

### plugins

声明可以是字符串或对象：

```ts
plugins: ['./lania.plugin.ts'];
```

```ts
plugins: [
  {
    name?: string,
    package: string,
    methods?: string[],
    signature?: string
  }
]
```

### 校验规则

| 字段                       | 说明                       |
| -------------------------- | -------------------------- |
| `pluginAllowlist`          | 允许的第三方包名           |
| `pluginMethodAllowlist`    | 全局方法白名单             |
| `pluginTrustedSources`     | `package` / `local_path`   |
| `pluginRequireSignature`   | 第三方包是否要求 signature |
| `pluginSignatureAllowlist` | 可跳过 signature 的包      |

本地插件：

- 必须相对路径
- 必须位于 workspace root 内
- 扩展名 `.js` / `.cjs` / `.mjs` / `.ts`

包插件：

- 默认信任 `@lania/plugin-`、`lania-plugin-` 前缀
- 其他包必须在 `pluginAllowlist`

最终方法：

```text
插件导出方法 ∩ 声明方法 ∩ pluginMethodAllowlist
```

## 11. ui

```ts
export default {
  ui: {
    locale: 'zh',
    output: {
      mode: 'json',
      events: 'buffered',
      pretty: false,
      includeHostState: true,
      includeBridgeExchange: true,
    },
    progress: {
      style: 'spinner',
      grouping: 'command',
    },
    interaction: {
      mode: 'auto',
      timeoutMs: 30000,
      defaultStrategy: 'use_defaults',
    },
  },
};
```

### ui.locale

优先级：

1. `ui.locale`
2. 全局 `~/.lania/preferences.json` 的 `locale`
3. `en`

### ui.output

| 字段                    | 可选值                     | 默认       |
| ----------------------- | -------------------------- | ---------- |
| `mode`                  | `json` / `jsonl` / `human` | `json`     |
| `events`                | `buffered` / `stream`      | `buffered` |
| `pretty`                | bool                       | false      |
| `includeHostState`      | bool                       | true       |
| `includeBridgeExchange` | bool                       | true       |

> `ui.output.mode = stream` 不是项目配置的合法枚举；`stream` 是全局偏好 `output.mode` 的写法，会被归一成 `jsonl` + `events=stream`。

### ui.progress

| 字段       | 可选值                           | 默认      |
| ---------- | -------------------------------- | --------- |
| `style`    | `spinner` / `bar` / `none`       | `spinner` |
| `grouping` | `command` / `task` / `operation` | `command` |

`grouping` 影响同一 command 内多个 bridge 事件归到哪个进度条：

- `command`：按 request id
- `task`：按 method
- `operation`：按 operationId/taskId/file/adaptor/tool

### ui.interaction

| 字段              | 可选值                                     | 默认           |
| ----------------- | ------------------------------------------ | -------------- |
| `mode`            | `auto` / `interactive` / `non_interactive` | `auto`         |
| `timeoutMs`       | number?                                    | 无             |
| `defaultStrategy` | `use_defaults` / `fail`                    | `use_defaults` |

行为：

- `non_interactive`：绝不弹问题
- `auto` + 非 TTY + `fail`：不弹问题
- `auto` + 非 TTY + `use_defaults`：使用默认值
- `interactive` + 非 TTY：报错

## 12. commands

```ts
export default {
  commands: {
    aliases: {
      gu: 'api user get-user',
    },
    shortcuts: {
      ping: 'curl http://localhost:3000/ping',
    },
  },
};
```

当前状态：

- 会被解析、校验、进入 `commands` 快照
- **没有接入实际命令解析或执行**
- 不要依赖它改变 CLI 行为

## 13. hooks

```ts
export default {
  hooks: {
    onCommandPreInit: [
      {
        type: 'plugin',
        kind: 'parallel',
        plugin: './lania.plugin.ts',
        handler: 'validateEnv',
      },
    ],
    onSuccess: [
      {
        type: 'plugin',
        kind: 'parallel',
        plugin: './lania.plugin.ts',
        handler: 'report',
        timeoutMs: 5000,
        onError: 'collect',
      },
    ],
  },
};
```

支持的 hook key 见 [插件与 Hook 系统](/architecture/plugin-system)。

配置中的函数型 hook 会被 `config.loadLan` 转成 inline hook 引用。

## 14. release

```ts
export default {
  release: {
    profile: 'package',
    env: 'prod',
    channel: 'stable',
    stateFile: '.lania/release-state.json',
    verify: {
      lint: true,
      test: { enabled: true, command: 'pnpm test:ci' },
      build: false,
      smoke: false,
    },
    versioning: {
      enabled: true,
      source: 'package.json',
      tagPrefix: 'v',
      command: 'pnpm version',
    },
    changelog: 'pnpm changelog',
    artifact: { enabled: true, command: 'pnpm build' },
    deploy: {
      provider: 'custom',
      command: 'pnpm deploy:prod',
    },
    postCheck: {
      url: 'https://example.com/healthz',
      command: 'curl -f https://example.com/healthz',
    },
    git: {
      commit: true,
      tag: true,
      push: false,
      remote: 'origin',
      branch: 'main',
    },
  },
};
```

### profile

- `package`
- `web-app`
- `service`
- `custom`

### step 形态

`verify.lint/test/build/smoke`、`changelog`、`artifact` 支持：

```ts
true
false
"pnpm test"
{ enabled: true, command: "pnpm test" }
```

### 默认值

| 字段                   | 默认                        |
| ---------------------- | --------------------------- |
| `profile`              | `package`                   |
| `stateFile`            | `.lania/release-state.json` |
| `verify.*`             | disabled                    |
| `versioning.enabled`   | true                        |
| `versioning.source`    | `package.json`              |
| `versioning.tagPrefix` | `v`                         |
| `deploy.provider`      | `custom`                    |
| `git.commit`           | true                        |
| `git.tag`              | true                        |
| `git.push`             | false                       |
| `git.remote`           | `origin`                    |

`config.release` 也兼容。

## 15. custom

```ts
export default {
  custom: {
    team: 'demo',
  },
};
```

必须是对象；会原样保留在快照中。

## 16. 工具配置

`config.loadTool` 支持：

- `vite`
- `webpack`
- `rollup`
- `gulp`
- `eslint`
- `oxlint`
- `prettier`
- `oxfmt`
- `stylelint`
- `textlint`
- `markdownlint`
- `commitlint`
- `commitizen` / `cz`

查找顺序：

1. 工具配置文件（如 `vite.config.ts`、`.eslintrc.cjs`）
2. `package.json` 中指向的配置
3. `package.json` 内联配置

## 17. 全局偏好

文件：

```text
~/.lania/preferences.json
```

结构：

```json
{
  "locale": "zh",
  "outputMode": "human",
  "logTimestamps": false
}
```

| 字段            | 可选值                      |
| --------------- | --------------------------- |
| `locale`        | `zh` / `en`                 |
| `outputMode`    | `json` / `stream` / `human` |
| `logTimestamps` | bool                        |

归一化：

- `zh-CN` → `zh`
- `en-US` → `en`
- `jsonl` → `stream`
- 非法值 → 默认

> `preferences_path()` 只读取 `HOME`。在 Windows 上如果 `HOME` 未设置，偏好读写会变成 no-op，使用默认值。

## 18. 校验行为

`validation_errors` 是诊断，不是硬失败。构建 snapshot 仍会成功，命令仍可运行。

常见错误：

| code                  | 说明         |
| --------------------- | ------------ |
| `invalid_type`        | 类型不对     |
| `unknown_field`       | 未知字段     |
| `invalid_value`       | 值不合法     |
| `unsupported_version` | 配置版本过新 |

## 下一步

- [配置文件指南](/guide/configuration)
- [输出与状态文件](/reference/output-and-state)
- [Schema 与协议](/reference/schemas)
