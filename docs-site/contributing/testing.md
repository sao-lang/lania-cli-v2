# 测试与验收

<div class="lead">
本页给出功能改动后的最小验证路径，以及跨平台验收时的注意事项。
</div>

## 快速验证

```bash
# Rust 构建
cargo build --manifest-path rust/Cargo.toml --workspace

# Rust 测试
cargo test --manifest-path rust/Cargo.toml --workspace

# TypeScript 类型检查
pnpm -C ts typecheck

# TypeScript 测试
pnpm -C ts test

# 文档站
pnpm docs:build
```

## 目标测试

```bash
# 某个 crate
cargo test --manifest-path rust/Cargo.toml -p lania-exec

# 某个 TS 包
pnpm -C ts --filter @lania-cli/templates test

# 单个 Node 测试
cd ts/packages/node-bridge
node --import tsx --test --test-name-pattern "product.publish" src/plugins/product.test.ts
```

## CLI 手工验收

```bash
lan --help
lan template
lan tools --plain --names-only --unique --limit 8
lan create --name demo-app --template spa-vue --preview
lan add --name button --template v3
lan generate api init
lan generate module init
lan release plan --profile package --version 1.2.3
lan sync status
```

## 跨平台注意

### Windows

- 使用 `PATHEXT` 解析 `.exe` / `.cmd` / `.bat`。
- 不要假设只有 `.exe`。
- 路径断言使用 `PathBuf` 或 `MAIN_SEPARATOR`。
- 平台包测试要移除所有 staging binary，避免宿主平台差异。

### macOS / Linux

- 注意可执行权限。
- shell 行为差异用 `/bin/sh` 作为基线。
- 不要在测试中依赖本机已安装的 Java、Go 等运行时。

## 当前已修复的验收问题

- Windows 下 `npm` / `pnpm` shim 无法 spawn。
- Windows 下 `tools run` 找不到 `python` / `node`。
- `add --template v3` 的 EJS 语法错误。
- Rust 测试硬编码 Unix 路径。
- Node Bridge 平台包矩阵测试的宿主差异。

## 验收记录建议

记录：

- 命令
- 操作系统
- 退出码
- stdout 关键字段
- stderr 关键错误
- 相关配置与版本

## 下一步

- [本地开发](/contributing/development)
- [常见问题](/guide/troubleshooting)
