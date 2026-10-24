# 配置文件

<div class="lead">
lan.config.* 由 Node Bridge 加载，Rust 宿主消费配置快照。它决定构建与 Lint 适配器、插件、动态命令、输出模式、交互策略与产品化元信息。
</div>

## 支持的文件名

Node Bridge 按以下顺序查找第一个存在的文件：

```text
lan.config.js
lan.config.cjs
lan.config.mjs
lan.config.json
lan.config.ts
lan.config.mts
lan.config.cts
```

配置文件可以是对象、模块 default export，或返回对象的函数。

## 最小配置

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
};
```

## 常用字段

| 字段                          | 作用                       |
| ----------------------------- | -------------------------- |
| `buildTool` / `buildAdaptors` | 构建工具选择与适配器       |
| `lintTools` / `lintAdaptors`  | Lint 工具与适配器          |
| `plugins`                     | 项目级插件声明             |
| `release`                     | 发布默认配置               |
| `extensions.dynamicCommands`  | 是否启用动态命令           |
| `schemaDiscovery`             | 动态命令 manifest 发现策略 |
| `hooks`                       | 全局 Hook 绑定             |
| `ui.output`                   | 输出模式与事件输出策略     |
| `ui.progress`                 | 进度展示模式               |
| `ui.interaction`              | 交互模式、超时与默认值     |
| `pluginAllowlist`             | 允许加载的插件             |
| `pluginMethodAllowlist`       | 允许调用的插件方法         |
| `pluginTrustedSources`        | 允许的插件来源             |
| `pluginRequireSignature`      | 是否要求第三方插件签名     |
| `pluginSignatureAllowlist`    | 可跳过签名的包             |
| `tools`                       | ctx.tools 能力与安全策略   |
| `product`                     | 产品 CLI 元信息            |
| `schema`                      | schema 入口与 discovery    |

> `product`、`schema`、`tools` 会被 Node 运行时消费，但当前 Rust 配置白名单尚未包含它们，因此可能出现在 `validation_errors` 中。校验错误是诊断，不会阻断命令执行。

## tools

```ts
export default {
  tools: {
    allow: ['git', 'pm', 'exec', 'fs', 'log', 'text', 'bridge', 'workspace', 'path', 'result'],
    deny: [],
    exec: {
      allowShell: false,
      allowEnvWrite: false,
    },
    fs: {
      writeRoot: '.',
    },
    bridge: {
      allowMethods: ['bridge.*', 'config.*', 'compiler.*', 'lint.*', 'commit*'],
    },
  },
};
```

这决定动态命令能使用哪些宿主能力。详细规则见 [ctx.tools 宿主能力](/guide/host-tools)。

## schemaDiscovery

默认发现规则：

- `files`：`lania.schemas.ts`、`lania.schemas.js`、`lania.schemas.cjs`
- `dirs`：`.lania/schemas`
- `allowExtensions`：`.ts`、`.js`、`.cjs`、`.json`、`.yaml`、`.yml`

示例：

```ts
export default {
  extensions: { dynamicCommands: true },
  schemaDiscovery: {
    files: ['lania.schemas.ts', 'lania.schemas.js'],
    dirs: ['.lania/schemas'],
    allowExtensions: ['.ts', '.js', '.json', '.yaml', '.yml'],
  },
};
```

## 插件与安全

```ts
export default {
  plugins: [
    {
      package: './lania.plugin.js',
      methods: ['commands.deploy', 'hooks.audit'],
    },
  ],
  pluginAllowlist: ['./lania.plugin.js'],
  pluginMethodAllowlist: ['commands.deploy', 'hooks.audit'],
};
```

- `plugins` 声明项目插件。
- `pluginAllowlist` 限制可加载来源。
- `pluginMethodAllowlist` 限制可调用方法。
- 插件还可能受签名策略约束。

## UI 配置

```ts
export default {
  ui: {
    output: {
      mode: 'human',
    },
    progress: {
      mode: 'spinner',
    },
    interaction: {
      mode: 'auto',
      timeoutMs: 30000,
    },
  },
};
```

输出模式：`json` / `human`（`stream` / `jsonl` 归一为 `json`）。

进度模式：`spinner` / `bar` / `none`。

## commands.aliases / shortcuts

```ts
export default {
  commands: {
    aliases: { gu: 'api user get-user' },
    shortcuts: { ping: 'curl http://localhost:3000/ping' },
  },
};
```

当前实现会解析并校验这两个字段，但**没有接入命令解析或执行**。不要依赖它们改变 CLI 行为。

## 全局偏好

除了项目配置，CLI 还有全局偏好：

```bash
lan config get
lan config set locale zh
lan config set output.mode human
lan config set log.timestamps true
```

全局偏好会影响所有项目；项目配置可以覆盖部分运行时行为。

## 产品配置

```json
{
  "extensions": { "dynamicCommands": true },
  "product": {
    "name": "@acme/cli",
    "binaryName": "acme",
    "displayName": "Acme CLI",
    "templatesDir": "./product/templates",
    "compat": {
      "frameworkVersionRange": ">=0.1.0 <1.0.0",
      "protocolVersionRange": "0.1.0",
      "nodeBridgeVersionRange": "0.1.0"
    }
  },
  "schema": {
    "entry": "./product/lania.schemas.ts"
  }
}
```

## 生成与状态文件

| 文件                                 | 用途                   |
| ------------------------------------ | ---------------------- |
| `lania.contract.yaml`                | `generate api` 输入    |
| `lania.module.yaml`                  | `generate module` 输入 |
| `.lania/contracts.lock.json`         | 契约生成跟踪           |
| `.lania/module-gen.lock.json`        | 模块生成跟踪           |
| `.lania/release-state.json`          | 发布状态               |
| `.lania/build/product`               | 产品构建快照           |
| `.lania/pack/product/install-root`   | 产品 install-root      |
| `.lania/publish/product/npm-package` | 产品发布包             |

## 下一步

- [配置字段参考](/reference/config)
- [动态命令与插件](/guide/dynamic-commands)
- [输出、语言与进度](/guide/output-and-locale)
