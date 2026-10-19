# 安装与运行原理

<div class="lead">
Lania CLI 的发布形态不是一个单独的可执行文件，而是“平台二进制 + Node Bridge 载荷 + npm 包装器”的组合。理解这一点，能快速定位安装与启动问题。
</div>

## 分发布局

```text
<install-root>/
  bin/
    lan
    lania-cli
  lib/
    node-bridge/
      package.json
      dist/
      node_modules/
```

- `bin/lan`：用户入口包装器。
- `bin/lania-cli`：Rust 宿主二进制。
- `lib/node-bridge`：Node 侧运行时载荷。

## npm 包装器做了什么

`npm/cli/bin/lan.mjs` 负责：

1. 识别当前平台与架构。
2. 选择对应的平台二进制包。
3. 启动 Rust 二进制。
4. 注入 `LANIA_NODE_BRIDGE_DIR`，让宿主找到 Node Bridge。

平台包示例：

- `@lania-cli/cli-darwin-arm64`
- `@lania-cli/cli-linux-x64`
- `@lania-cli/cli-win32-x64`

## Bridge 目录查找顺序

Rust 宿主按以下顺序查找 Node Bridge：

1. `LANIA_NODE_BRIDGE_DIR`
2. `<current-exe>/../lib/node-bridge`
3. `<current-exe>/../node-bridge`
4. `<current-exe>/../Resources/node-bridge`
5. 开发态工作区 `ts/packages/node-bridge`

因此在开发态直接运行 debug 二进制时，只要工作区结构完整，通常不需要额外设置环境变量。

## 从源码构建

### 1. 安装 TypeScript 依赖

```bash
cd ts
pnpm install
pnpm build
```

### 2. 构建 Rust CLI

```bash
cd ../rust
cargo build -p lania-cli
```

### 3. 验证

```bash
cargo run -p lania-cli -- help
cargo run -p lania-cli -- template
cargo run -p lania-cli -- tools --plain --names-only --unique
```

## 环境变量

| 变量                               | 作用                       |
| ---------------------------------- | -------------------------- |
| `LANIA_NODE_BRIDGE_DIR`            | 强制指定 Node Bridge 目录  |
| `LANIA_PRODUCT_ROOT`               | 产品运行时根目录           |
| `LANIA_RUNTIME_MODE`               | 产品运行时模式             |
| `LANIA_PRODUCT_HOST_BINARY_SOURCE` | 产品发布时的宿主二进制来源 |
| `LANIA_PRODUCT_HOST_BINARY`        | 产品发布时的宿主二进制路径 |
| `LANIA_HOST_BINARY`                | 宿主二进制路径候选         |

## 发布与分发

Lania CLI 自身的产品化链路也使用同一套命令：

```bash
lan product build
lan product pack
lan product publish --dist-tag next --channel next
```

更多细节见 [产品 CLI](/guide/product-cli) 与 [打包并发布产品 CLI](/cookbook/product-publish)。

## 排查清单

- `lan --help` 能运行，但 `template` 报错：优先检查 Node Bridge 目录与 `ts/packages/node-bridge/dist`。
- 只有 `lan` 没有 `lania-cli`：检查平台包是否正确安装。
- Windows 下包管理器启动失败：确认 `PATHEXT` 包含 `.CMD`，且 PATH 中能找到 `npm.cmd` / `pnpm.cmd`。
- 产品 CLI 找不到 Bridge：检查 `LANIA_NODE_BRIDGE_DIR` 是否指向 `lib/node-bridge`。

## 下一步

- [创建第一个项目](/guide/first-project)
- [命令总览](/guide/commands)
- [常见问题](/guide/troubleshooting)
