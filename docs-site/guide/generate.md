# 代码生成

<div class="lead">
generate 命令族负责两类生成：generate api 生成契约 DTO 与传输层代码，generate module 生成 lania-g 模块与 main.go 注入代码。
</div>

## generate api

### 命令结构

```text
lan generate api
  plan    预览生成计划
  diff    对比当前 manifest 与计划产物
  init    初始化 lania.contract.yaml 与示例 schema
```

### 初始化

```bash
lan generate api init
```

会生成：

- `lania.contract.yaml`
- `schemas/proto/greeter.proto`
- `contracts/generated/`
- `transport/generated/`
- `modules/`

### 规划与执行

```bash
# 生成全部配置
lan generate api

# 只预览
lan generate api plan

# 只看差异
lan generate api diff

# 按 source / target / entry 过滤
lan generate api --source proto --target grpc,http
lan generate api --entry user-service --target http

# 校验产物是否最新
lan generate api --check

# 清理 stale 生成文件
lan generate api --clean
```

### 常用选项

| 选项                    | 作用                                 |
| ----------------------- | ------------------------------------ |
| `-c, --config <path>`   | 契约配置，默认 `lania.contract.yaml` |
| `-s, --source <source>` | 源类型过滤，如 `proto` / `thrift`    |
| `-t, --target <target>` | 目标过滤，如 `grpc,http`             |
| `-e, --entry <entry>`   | 入口过滤，支持逗号分隔               |
| `-m, --manifest <path>` | manifest 路径覆盖                    |
| `-d, --dry-run`         | 只规划                               |
| `-k, --check`           | 只校验                               |
| `--clean`               | 清理 stale 文件                      |
| `-f, --force`           | 覆盖未受管理的冲突文件               |

## generate module

### 命令结构

```text
lan generate module
  plan    预览生成与注入计划
  diff    对比 manifest 与计划产物
  init    初始化 lania.module.yaml
  apply   写入产物并执行 main.go 注入
```

### 初始化

```bash
lan generate module init
```

### 生成与校验

```bash
# 生成全部配置
lan generate module

# 预览
lan generate module plan

# 按入口和目标校验
lan generate module --entry user --target grpc --check

# 写入但不注入 main.go
lan generate module apply --no-inject
```

### 支持链路

- `protobuf → grpc`
- `thrift → http rest`
- `json/yaml → ws`
- `graphql → graphql`

### 产物结构

以 gRPC 为例：

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

以 HTTP REST 为例：

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

### main.go 注入

`generate module` 会查找标记块并幂等注入注册代码。若标记块或目标文件不存在，会在计划中记录“跳过注入”，而不是直接失败。

## 状态文件

| 文件                          | 用途             |
| ----------------------------- | ---------------- |
| `.lania/contracts.lock.json`  | 契约生成输出跟踪 |
| `.lania/module-gen.lock.json` | 模块生成输出跟踪 |

这些 manifest 用于：

- 判断文件是否由工具生成
- 计算 stale 文件
- 支持 `--check` 与 `--clean`
- 避免覆盖未受管理的文件

## 配置细节

- `generate api` 读取 `lania.contract.yaml`
- `generate module` 读取 `lania.module.yaml`

详细字段与示例见 [Schema 与协议](/reference/schemas)。

## 下一步

- [生成 HTTP / gRPC 代码](/cookbook/generate-api)
- [生成 Go 模块](/cookbook/generate-module)
- [Schema 与协议](/reference/schemas)
