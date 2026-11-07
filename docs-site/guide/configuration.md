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

## 三层配置统一管理

除了项目配置与全局偏好，CLI 还有**包级偏好**：每个已安装命令子包的配置保存在 `~/.lania/config/<pkg>.json`，可用键由该包 `lania.package.json` 的 `configSchema` 声明。`lan config` 是统一管理面，`config` 自身不算命令子包；`generate` 归档后共 11 个命令。

| 层       | 位置                         | 说明                                      |
| -------- | ---------------------------- | ----------------------------------------- |
| 项目配置 | `lan.config.*`               | 构建 / Lint 适配器、插件、动态命令、UI    |
| 全局偏好 | `~/.lania/preferences.json`  | `locale` / `outputMode` / `logTimestamps` |
| 包级偏好 | `~/.lania/config/<pkg>.json` | 10 个命令子包各自的偏好                   |

10 个命令子包（`add` / `build` / `create` / `dev` / `env` / `git` / `lint` / `pi` / `release` / `tools`）全部带 `config` 二级命令：`bare` / `get` / `set` / `init` / `doctor` / `sync`，中英双语文案齐全，`get` / `set` 支持 `--global` 切到全局作用域。通用键为 `locale`（`zh` / `en`）、`output.mode`（`json` / `human`）、`log.timestamps`（bool），`env` 额外声明 `shim.host`（`lan` / `runex`）。共用实现收敛在 `packages/foundation/package-config/rust/lania-command-config`。

主包 `lan config` 可跨包统一管理：

```bash
# 读写单个子包（--package 支持短名 / npm 全名 / bin 名 / crate 名，统一归一化）
lan config get locale --package env
lan config set locale zh --package @lania-cli-v2/cmd-env

# 列出所有已安装子包声明的键、默认值与当前值
lan config packages --json

# 把全局偏好一键下发到所有已安装且启用的子包
lan config apply
lan config apply --package env --set locale=zh --set output.mode=human
```

`--package` 归一化后按目标包 `configSchema` 校验写入；`apply` 只覆盖子包已声明的键（点分键与 camelCase 全局键有映射）。静态表 `known-commands.json` 现携带各子包 `configSchema`，launcher 不再在运行时解析 npm manifest。独立二进制 `lan-config packages` / `lan-config apply` 也可用，但主路径是 `lan`。字段细节见 [配置字段参考](/reference/config)。

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

## 状态文件

| 文件                                 | 用途              |
| ------------------------------------ | ----------------- |
| `.lania/release-state.json`          | 发布状态          |
| `.lania/build/product`               | 产品构建快照      |
| `.lania/pack/product/install-root`   | 产品 install-root |
| `.lania/publish/product/npm-package` | 产品发布包        |

> `generate` 相关命令（`generate api` / `generate module` / `lan product generate`）已下线归档，将在后续版本重新设计；完整代码保存在本地分支 `archive/generate-command`（指向提交 `61d47b9`）。`lania.contract.yaml`、`lania.module.yaml` 与对应的 lock 文件不再由内置命令维护。

## 下一步

- [配置字段参考](/reference/config)
- [动态命令与插件](/guide/dynamic-commands)
- [输出、语言与进度](/guide/output-and-locale)
