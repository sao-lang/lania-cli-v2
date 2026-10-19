# 认识 Lania CLI

<div class="lead">
Lania CLI v2 是一个以 Rust 为宿主、以 Node Bridge 承接 JavaScript 生态能力的双运行时 CLI。
它既是一组开箱即用的工程命令，也是一个可以挂载团队自有命令的 CLI 宿主框架。
</div>

## 你将学到

- Lania CLI 解决什么问题
- 三个核心层次：安装层、宿主层、Bridge 层
- 内置命令与动态命令的区别
- 什么时候应该使用它

## 它解决什么问题

现代前端/全栈项目通常需要同时处理：

- 项目脚手架与模板
- 开发服务器、构建与 Lint
- 契约代码与模块代码生成
- Git 同步与发布编排
- 团队自己的领域命令
- 将内部 CLI 打包成可分发的产品

如果把这些能力分散到几十个 npm scripts 和 shell 脚本里，命令入口、错误处理、输出格式和权限边界都会逐渐失控。

Lania CLI 的做法是：

1. 用 Rust 提供稳定的命令树、执行编排和统一输出。
2. 用 Node Bridge 承接必须依赖 JavaScript 生态的能力。
3. 用配置文件和 schema manifest 允许项目扩展命令。
4. 用 `product` 命令族把同一套能力产品化。

## 双运行时模型

```text
用户执行 lan ...
        │
        ▼
npm/cli/bin/lan.mjs
  选择平台二进制 + 注入 LANIA_NODE_BRIDGE_DIR
        │
        ▼
rust/crates/lania-cli
  HostRuntime / 命令树 / 工作流
        │
        ├── 本地宿主能力：Git、文件系统、进程、日志、进度
        │
        └── 需要 JS 能力时 ── STDIO JSON ──▶ Node Bridge
                                              ├── 配置加载
                                              ├── 编译器 / Lint
                                              ├── 模板运行时
                                              └── 动态命令
```

一句话记忆：

- **Rust 决定命令如何被执行。**
- **Node 决定 JavaScript 能力如何被加载与调用。**
- **npm 决定 CLI 如何被安装与启动。**

## 名字与标志

<p align="left">
  <img src="/logo.svg" width="88" height="88" alt="Lania CLI logo">
</p>

**Lania** 来自 **Laniakea**（拉尼亚凯亚），天文学上指包含银河系在内的超星系团。这个名字想表达的是：很多分散的星系通过引力纤维连成一张巨大的网络，正如很多工程能力通过同一棵 CLI 命令树连接起来。

标志把两个意象压在一个图形里：

- 深空底色、弧形纤维与星点：超星系团的星系网络。
- 白色 `>` 与绿色光标：终端提示符与命令执行。
- 绿到青的渐变：Rust 宿主与 Node Bridge 之间的双向链路。

所以它不是一颗孤立的星球，而是一张**可执行**的星系网络：宇宙级的连接感，落在一条 `lan` 命令上。

## 四个层次

| 层次      | 目录                      | 职责                                           |
| --------- | ------------------------- | ---------------------------------------------- |
| 安装层    | `npm/`                    | `lan` 包装器、平台二进制选择、Bridge 路径注入  |
| 宿主层    | `rust/`                   | 命令注册、参数解析、工作流、宿主服务、统一输出 |
| Bridge 层 | `ts/packages/node-bridge` | 配置、插件、编译器、Lint、模板、动态命令       |
| 资产层    | `ts/packages/templates`   | 项目模板、add 模板、模板问题与渲染             |

## 内置命令与动态命令

### 内置命令

由 Rust 插件注册，随 CLI 一起发布：

`dev`、`build`、`lint`、`tools`、`create`、`add`、`template`、`generate`、`release`、`sync`、`config`、`product`。

### 动态命令

由项目自己的 `lania.schemas.*` 或 `.lania/schemas/` 声明，在启动阶段被 Node Bridge 解析并注册到同一棵命令树中。

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      command: { about: 'Ops tools', alias: 'o' },
      commands: [
        {
          name: 'ping',
          about: 'Ping endpoint',
          options: [{ long: 'endpoint', valueKind: 'string', required: true }],
          handler: async (ctx) => ({
            result: { ok: true, input: ctx.argv.options, exitCode: 0 },
            events: [],
          }),
        },
      ],
    },
  ],
};
```

动态命令让团队可以在**不修改 Rust 宿主**的前提下扩展 CLI。详细规则见 [动态命令与插件](/guide/dynamic-commands)。

## 什么场景适合使用

- 你希望用一个 `lan` 统一团队项目的开发、构建、生成和发布流程。
- 你有多套模板或代码生成配置，希望集中管理。
- 你想把内部工程能力封装成团队专属 CLI。
- 你最终希望把这些能力打包成可分发的 npm 产品。

如果你只需要一个简单的脚本运行器，`tools run` 已经够用；如果你需要完整的工程闭环，再逐步引入模板、生成与发布工作流。

## 下一步

- [快速上手](/guide/getting-started)：安装并运行第一条命令。
- [安装与运行原理](/guide/installation)：理解二进制与 Bridge 载荷如何协作。
- [命令总览](/guide/commands)：按命令族建立全局地图。
