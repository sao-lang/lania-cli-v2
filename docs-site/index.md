---
layout: home

hero:
  name: Lania CLI
  text: 双运行时 CLI 宿主框架
  tagline: Rust 负责命令树与执行编排，Node 负责 JavaScript 生态与模板运行时。一套 CLI，覆盖开发、生成、发布与产品化。
  image:
    src: /hero.svg
    alt: Lania CLI 双运行时架构
  actions:
    - theme: brand
      text: 开始学习
      link: /guide/introduction
    - theme: alt
      text: 查看命令
      link: /reference/cli
    - theme: alt
      text: 快速上手
      link: /guide/getting-started

features:
  - icon: 🦀
    title: Rust 宿主
    details: 命令注册、参数解析、工作流编排、Git、文件系统、日志、进度与统一输出全部由 Rust 承担。
  - icon: 🟢
    title: Node Bridge
    details: 配置加载、模板运行时、编译器与 Lint 适配器通过 STDIO JSON 协议接入，保持 JS 生态原生体验。
  - icon: 🧩
    title: 可扩展命令树
    details: 通过 lan.config.*、lania.schemas.* 与项目插件，在不修改 Rust 宿主的前提下挂载团队自己的命令。
  - icon: 🏗️
    title: 工程化闭环
    details: 脚手架、契约生成、模块生成、发布编排、Git 同步与产品 CLI 打包发布全部内置。
  - icon: 🚀
    title: 产品化能力
    details: product generate / build / pack / publish 将内部工程 CLI 变成可分发的 npm 产品。
  - icon: 🛠️
    title: 跨平台
    details: 支持 Windows、macOS 与 Linux，并兼容 .cmd、.bat 等 Windows 可执行入口。
---

## 从这里开始

<div class="command-card">
  <div>
    <strong>第一次接触</strong>
    先读 <a href="/guide/introduction">认识 Lania CLI</a>，建立“安装层 / 宿主层 / Bridge 层 / 资产层”的心智模型。
  </div>
  <div>
    <strong>想马上跑起来</strong>
    跟随 <a href="/guide/getting-started">快速上手</a>，用 5 分钟创建项目并执行 build / lint。
  </div>
  <div>
    <strong>要查具体命令</strong>
    直接打开 <a href="/reference/cli">CLI 命令参考</a>，按命令族查看语法、选项与示例。
  </div>
  <div>
    <strong>想理解内部实现</strong>
    从 <a href="/architecture/overview">整体架构</a> 进入，理解三个进程边界与请求流转。
  </div>
</div>

## 一条命令，四个层次

Lania CLI 不是一组零散脚本，而是一个分层的 CLI 宿主：

1. **安装层 `npm/`**：向用户暴露 `lan`，选择平台二进制并注入 Node Bridge 载荷路径。
2. **宿主层 `rust/`**：决定有哪些命令、如何解析参数、如何编排工作流和输出结果。
3. **Bridge 层 `ts/packages/node-bridge`**：加载配置与插件，承接编译器、Lint、模板与动态命令。
4. **资产层 `ts/packages/templates`**：提供项目模板、add 模板、模板问题和渲染运行时。

> Rust 决定命令如何被执行，Node 决定 JavaScript 能力如何被加载与调用，npm 决定 CLI 如何被安装与启动。

## 常用入口

| 我想……         | 运行                                                        |
| -------------- | ----------------------------------------------------------- |
| 查看帮助       | `lan --help`                                                |
| 启动开发服务器 | `lan dev`                                                   |
| 构建项目       | `lan build`                                                 |
| 执行 Lint      | `lan lint`                                                  |
| 创建项目       | `lan create --name demo-app --template spa-vue`             |
| 查看模板       | `lan template`                                              |
| 运行单文件     | `lan tools run ./demo.ts`                                   |
| 生成契约代码   | `lan generate api init`                                     |
| 规划发布       | `lan release plan --profile package --version 1.2.3`        |
| 生成产品 CLI   | `lan product generate --name "Acme CLI" --binary-name acme` |

接下来，建议从 [认识 Lania CLI](/guide/introduction) 开始。
