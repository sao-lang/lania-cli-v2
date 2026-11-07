# 示例索引

当前 `examples/` 目录保留两个仍然有明确用途的示例：

## 1. `product-cli-demo`

用途：

- 演示如何把当前 runtime 贴牌成一个产品 CLI
- 演示 `product` 配置、产品命令、产品模板和 `build/pack/publish` 主链路

适合你在以下场景阅读：

- 想理解“如何封装其他 CLI”
- 想看最小产品 CLI 样板
- 想理解产品化构建和发布链路

## 2. `schema-tools-demo`

用途：

- 演示 dynamic commands 中 `ctx.tools` 的最小使用方式
- 演示 inline hook、handler、bridge、workspace、pm、git、exec、fs、log 等能力的组合

适合你在以下场景阅读：

- 想理解 `ctx.tools` 到底怎么用
- 想看动态命令而不是产品 CLI
- 想验证项目级扩展的最小闭环

## 已删除的示例

`generate-module-http-rest-demo` 与 `generate-module-grpc-demo` 随 `generate` 命令一起下线归档，已从本目录删除；该命令将在后续版本重新设计。

## 保留原则

- 保留“功能定位清晰、示例边界明确”的 demo
- 删除“已被更完整示例覆盖”的阶段性 demo
