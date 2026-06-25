# generate-module-grpc-demo

`lan generate module` 的单协议 `protobuf -> grpc` 示例。

这个 demo 只关注你当前要的这条链路：

- 完整走 `protobuf -> lania-g grpc` 生成
- 展示 unary / server stream / client stream / bidi stream 四种方法签名
- DTO 上的 `required` 规则直接落到 `ctx.ShouldBindReq(&req)` 使用场景
- `oneof` 生成 `ValidateOneof()` 校验辅助方法
- 根目录 `bootstrap` helper 提供可直接接入业务工程的 wiring helper
- 结构与 HTTP REST demo 对齐：根目录只放 `bootstrap.gen.go`，service 目录内拆 `dto / metadata / errors / register / <method>`

## 目录说明

- `lania.module.yaml`
  - `generate module` 输入配置，示例里通过 `output.grpcRootDir: generated/grpc` 把 gRPC 产物收敛到更浅的根目录
- `schemas/proto/user.proto`
  - 覆盖 unary / stream / deprecated / idempotency_level / oneof / required 的 Protobuf 示例
- `generated/grpc`
  - 实际生成结果目录

## 运行命令

在当前目录执行：

```bash
cargo run --manifest-path ../../rust/Cargo.toml -p lania-cli -- generate module
```

如果你本机已经装了 `lan`，也可以直接执行：

```bash
lan generate module
```

## 预期产物

成功后至少会生成：

- `generated/grpc/bootstrap.gen.go`
- `generated/grpc/user_service/dto.gen.go`
- `generated/grpc/user_service/metadata.gen.go`
- `generated/grpc/user_service/errors.gen.go`
- `generated/grpc/user_service/register.gen.go`
- `generated/grpc/user_service/create_user.gen.go`
- `generated/grpc/user_service/watch_users.gen.go`
- `generated/grpc/user_service/upload_users.gen.go`
- `generated/grpc/user_service/chat_users.gen.go`
- `.lania/module-gen.lock.json`

其中：

- `generated/grpc/user_service/dto.gen.go`
  - 包含 DTO 和 `ValidateOneof()` 等共享类型
- `generated/grpc/user_service/register.gen.go`
  - 包含 receiver stub 和 service 注册逻辑
- `generated/grpc/user_service/create_user.gen.go`
  - 展示 unary 方法里通过 `ctx.ShouldBindReq(&req)` 做请求绑定
- `generated/grpc/user_service/watch_users.gen.go`
  - 展示 server stream 绑定参数
- `generated/grpc/user_service/upload_users.gen.go`
  - 展示 client stream 绑定参数
- `generated/grpc/user_service/chat_users.gen.go`
  - 展示 bidi stream 绑定参数
- `generated/grpc/user_service/metadata.gen.go`
  - 包含 service / method metadata
- `generated/grpc/user_service/errors.gen.go`
  - 包含 gRPC status code / error helper
- `generated/grpc/bootstrap.gen.go`
  - 提供 `NewUserGrpcBootstrap()`、`Providers()`、`Register(api *grpcadapter.API)`，可直接被业务 `main.go` 接入
