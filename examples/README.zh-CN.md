# 示例索引

当前 `examples/` 目录保留四个仍然有明确用途的示例：

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

## 3. `generate-module-http-rest-demo`

用途：

- 演示 `lan generate module` 在 `thrift -> http rest` 单协议模式下的最终输出
- 演示单文件 HTTP 控制器代码、错误辅助代码、envelope 和 demo `main.go`

适合你在以下场景阅读：

- 只关心 `thrift -> http rest`
- 想看最接近 `lania-g/cmd/http-demo` 风格的生成结果
- 想直接复制一个最小 demo 来跑生成命令

## 4. `generate-module-grpc-demo`

用途：

- 演示 `lan generate module` 在 `protobuf -> grpc` 单协议模式下的最终输出
- 演示 unary / server stream / client stream / bidi stream 四类 gRPC 方法生成
- 演示 `protobuf -> grpc` 已经可以生成 adapter 文件、metadata/errors helper 和 `Bootstrap` 接线 helper

适合你在以下场景阅读：

- 只关心 `protobuf -> grpc`
- 想看最接近 `lania-g/cmd/grpc-demo` 风格的生成结果
- 想直接复制一个最小 demo 来跑生成命令

## 保留原则

- 保留“功能定位清晰、示例边界明确”的 demo
- 删除“已被更完整示例覆盖”的阶段性 demo
