# 生成 Go 模块

<div class="lead">
目标：使用 lania-g 将 protobuf / thrift schema 生成 Go DTO、注册代码、方法骨架与 bootstrap helper。
</div>

## 步骤 1：初始化

```bash
lan generate module init
```

生成：

- `lania.module.yaml`
- 示例 schema
- 输出目录骨架

## 步骤 2：配置

gRPC 示例：

```yaml
version: 1

framework:
  name: lania-g
  language: go

inputs:
  - name: user-grpc
    source: protobuf
    path: schemas/proto
    include:
      - '**/*.proto'
    targets:
      - grpc

output:
  root: generated/lania
  moduleDir: generated/lania/modules
  adapterDir: generated/lania/adapters
  contractDir: generated/lania/contracts
  grpcRootDir: generated/grpc
  manifest: .lania/module-gen.lock.json

inject:
  enabled: false
```

HTTP REST 示例：

```yaml
inputs:
  - name: user-http
    source: thrift
    path: schemas/thrift
    targets:
      - http
```

## 步骤 3：规划

```bash
lan generate module plan
```

## 步骤 4：生成

```bash
lan generate module
```

## 步骤 5：校验

```bash
lan generate module --entry user --target grpc --check
```

## 步骤 6：不注入 main.go

```bash
lan generate module apply --no-inject
```

## 预期产物

gRPC：

```text
generated/grpc/
  bootstrap.gen.go
  user_service/
    dto.gen.go
    metadata.gen.go
    errors.gen.go
    register.gen.go
    create_user.gen.go
    watch_users.gen.go
    upload_users.gen.go
    chat_users.gen.go
```

HTTP：

```text
generated/http/
  bootstrap.gen.go
  users/
    dto.gen.go
    errors.gen.go
    envelope.gen.go
    register.gen.go
    create_user.gen.go
    get_user.gen.go
    list_users.gen.go
```

## 示例工程

```text
examples/generate-module-grpc-demo/
examples/generate-module-http-rest-demo/
```

## 下一步

- [代码生成指南](/guide/generate)
- [Schema 与协议](/reference/schemas)
- [生成 HTTP / gRPC 代码](/cookbook/generate-api)
