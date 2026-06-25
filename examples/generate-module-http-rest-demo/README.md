# generate-module-http-rest-demo

`lan generate module` 的单协议 `thrift -> http rest` 示例。

这个 demo 只关注你当前要的这条链路：

- `tree-sitter` 解析 Thrift
- 按 `handler_path` 分目录的 HTTP 控制器生成
- 根目录 `bootstrap.gen.go` 生成可直接接入业务工程的 wiring helper
- `include / typedef / const / enum / union / exception / throws / oneway / service extends`
- 分组产物：
  - `<group>/dto.gen.go`
  - `<group>/errors.gen.go`
  - `<group>/envelope.gen.go`
  - `<group>/register.gen.go`
  - `<group>/<method>.gen.go`
  - `httpRootDir/bootstrap.gen.go`

## 目录说明

- `lania.module.yaml`
  - `generate module` 输入配置，示例里通过 `output.httpRootDir: generated/http` 把 HTTP 单协议产物收敛到一个更浅的根目录
- `schemas/thrift/shared.thrift`
  - 公共类型、常量、枚举、分页参数
- `schemas/thrift/user.thrift`
  - 用户 HTTP Rest 示例
- `generated/lania/adapters/http`
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

- `generated/http/bootstrap.gen.go`
- `generated/http/users/dto.gen.go`
- `generated/http/users/errors.gen.go`
- `generated/http/users/envelope.gen.go`
- `generated/http/users/register.gen.go`
- `generated/http/users/create_user.gen.go`
- `generated/http/users/get_user.gen.go`
- `generated/http/users/list_users.gen.go`
- `.lania/module-gen.lock.json`

`generated/http/bootstrap.gen.go` 不再是 `package main` 形式的 demo 入口，而是一个可被业务 `main.go` 直接 import 的 `Bootstrap` helper，用来提供：

- `NewUserHttpBootstrap()`
- `(*UserHttpBootstrap).Providers()`
- `(*UserHttpBootstrap).Register(api *httpadapter.API)`
