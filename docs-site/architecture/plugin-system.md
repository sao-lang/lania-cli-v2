# 插件与 Hook 系统

<div class="lead">
插件系统让 Rust 宿主、Node Bridge 和项目代码都能以受控方式参与命令生命周期。Hook 则提供跨命令的统一扩展点。
</div>

## 三类插件

| 类型          | 运行位置    | 注册方式                                  | 能做什么                                 |
| ------------- | ----------- | ----------------------------------------- | ---------------------------------------- |
| Rust 插件     | 宿主进程    | `host.register_plugin()`                  | 注册命令、handler、Capability、Hook      |
| Node 内置插件 | Node Bridge | `plugin-registry` 内置                    | 编译、Lint、模板、动态命令、产品化       |
| 项目插件      | Node Bridge | `lan.config.plugins` / manifest `plugins` | 提供命令 handler、Hook、schema converter |

### Rust 插件

Rust 插件实现 `Plugin` trait：

- `meta()`：名称、版本、依赖、顺序。
- `setup()`：注册命令、handler、能力与 Hook。

示例：

```rust
impl Plugin for BuildCommandPlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "command-build".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![
                CapabilityName::Logger,
                CapabilityName::NodeBridge,
                CapabilityName::Task,
                CapabilityName::Progress,
            ],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        register_build_commands(ctx)
    }
}
```

### Node 内置插件

Node Bridge 启动时注册：

| 插件               | 方法前缀                                           |
| ------------------ | -------------------------------------------------- |
| `config`           | `config.*`                                         |
| `dynamic-commands` | `commands.resolveDynamic`、`command.invokeDynamic` |
| `compiler`         | `compiler.*`                                       |
| `lint`             | `lint.*`                                           |
| `template`         | `template.*`、`addTemplate.render`                 |
| `product`          | `product.*`                                        |
| `system`           | `system.listCommands`                              |
| `commitizen`       | `commitizen.run`                                   |
| `commitlint`       | `commitlint.run`                                   |

内置插件不经过项目插件安全策略，但它们暴露的方法仍受 `ctx.tools.bridge.allowMethods` 约束。

### 项目插件

项目插件由 `lan.config.*` 声明：

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

项目插件运行在 Node Bridge 中，通过动态命令 manifest 绑定 handler。

## 插件安全

加载项目插件前会校验：

1. 插件来源是否在允许范围内。
2. 声明的方法是否在 allowlist 中。
3. 是否满足签名策略。
4. 方法是否属于插件允许的方法集合。

这避免了“配置文件里写一个路径就任意执行代码”的风险。

## Hook 生命周期

当前实现共有 20 个 hook key：

| Hook                    | 时机                    | 默认 kind |
| ----------------------- | ----------------------- | --------- |
| `onInitialize`          | 宿主初始化              | parallel  |
| `onCommandPreInit`      | 命令执行前，可改写 argv | waterfall |
| `onArgsParsed`          | 参数解析后，可改写 argv | waterfall |
| `onFilesPrepare`        | 文件计划生成前          | waterfall |
| `onConfigGet`           | 读取配置前              | waterfall |
| `onConfigResolve`       | 配置解析后              | waterfall |
| `onFileWrite`           | 文件写入前后            | parallel  |
| `onTemplateParse`       | 模板解析时              | waterfall |
| `onDependenciesModify`  | 依赖计划修改            | waterfall |
| `onDependenciesInstall` | 依赖安装前后            | parallel  |
| `onInteractionPrompt`   | 交互补问时              | waterfall |
| `onShellCommand`        | 执行 shell 前后         | parallel  |
| `onPluginApiCall`       | 调用插件 API 时         | parallel  |
| `onSuccess`             | 命令成功                | parallel  |
| `onError`               | 命令失败                | parallel  |
| `onPluginLoaded`        | 插件加载后              | parallel  |
| `onCommandRegister`     | 命令注册时              | parallel  |
| `onWorkflowStart`       | 工作流开始              | parallel  |
| `onWorkflowComplete`    | 工作流结束              | parallel  |
| `onShutdown`            | 宿主关闭                | parallel  |

`preRun` / `postRun` 是旧写法，会分别映射到 `onCommandPreInit` / `onSuccess`。

### waterfall 与 parallel

- `waterfall`：按顺序执行，后一个 hook 接收前一个的返回值。
- `parallel`：并发执行，结果收集后统一返回。
- Hook 返回的 `events` 会进入 Bridge 事件流，由宿主统一渲染。
- `timeoutMs` 控制单个 hook 超时；`onError` 支持 `throw` / `collect`。

## 动态命令中的 Hook

```ts
hooks: {
  preRun: [
    async () => ({
      events: [
        {
          method: 'event.log',
          params: { level: 'info', message: 'preRun hook' },
        },
      ],
    }),
  ];
}
```

## 全局 Hook

```ts
export default {
  hooks: {
    onSuccess: [
      {
        plugin: './lania.plugin.js',
        handler: 'hooks.audit',
      },
    ],
  },
};
```

全局 Hook 同时作用于内置命令与动态命令。

## 设计原则

- 命令注册与执行解耦。
- 能力通过 CapabilityContainer 注入，而不是全局单例。
- 插件声明依赖与顺序，宿主负责装配。
- 项目扩展必须经过策略校验。
- Hook 只产生事件或结果，不直接接管输出。

## 下一步

- [动态命令与插件](/guide/dynamic-commands)
- [开发运行时命令](/guide/runtime-commands)
- [运行时清单参考](/reference/manifest)
- [Rust 宿主运行时](/architecture/rust-host)
- [Node Bridge](/architecture/node-bridge)
