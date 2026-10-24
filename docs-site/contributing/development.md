# 本地开发

<div class="lead">
本页说明如何搭建 Lania CLI 的开发环境、构建 Rust 与 TypeScript 侧、以及日常修改时的推荐命令。
</div>

## 环境

- Node.js 20+
- pnpm 10.x
- Rust 1.80+
- Git

## 初始化

```bash
# 根目录安装依赖
pnpm install

# TypeScript 侧
cd ts
pnpm install
pnpm build

# Rust 侧
cd ..
cargo build --manifest-path rust/Cargo.toml -p lania-cli
```

根目录还提供：

```bash
pnpm bootstrap
```

它会安装依赖并执行 `cargo fetch`。

## 目录

```text
lania-cli-v2/
  docs/            维护者文档
  docs-site/       VitePress 学习文档
  examples/        示例工程
  npm/             npm 包装器与平台包
  rust/            Rust 宿主
  ts/              Node Bridge 与模板
  scripts/         工程脚本
```

## Rust 开发

```bash
cargo build --manifest-path rust/Cargo.toml --workspace
cargo test --manifest-path rust/Cargo.toml --workspace
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml --all
```

运行 CLI：

```bash
cargo run --manifest-path rust/Cargo.toml -p lania-cli -- --help
```

## TypeScript 开发

```bash
cd ts
pnpm typecheck
pnpm test
pnpm build
```

单独构建模板包：

```bash
pnpm --filter @lania-cli/templates build
```

## 常用根脚本

| 脚本              | 作用            |
| ----------------- | --------------- |
| `pnpm lint`       | TS + Rust Lint  |
| `pnpm typecheck`  | TS 类型检查     |
| `pnpm test`       | TS 与 Rust 测试 |
| `pnpm test:rust`  | cargo nextest   |
| `pnpm pack`       | 打包            |
| `pnpm docs:dev`   | 启动文档站      |
| `pnpm docs:build` | 构建文档站      |

## 修改建议

- 命令行为改动优先补对应 crate / package 测试。
- Bridge 方法改动同时检查 Rust 请求与 Node 插件。
- 模板资产改动后同步构建 `dist`。
- 输出模式改动检查 `json` / `human`。
- Windows 兼容改动优先在真实 Windows 环境验证。

## 下一步

- [测试与验收](/contributing/testing)
- [文档站维护](/contributing/docs)
- [整体架构](/architecture/overview)
