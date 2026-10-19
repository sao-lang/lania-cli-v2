# Schema 与协议

<div class="lead">
本页说明三类 schema：动态命令 manifest、契约生成配置、模块生成配置。
</div>

## 动态命令 manifest

> 本页只保留最小结构。完整字段、workflow、preset、hooks、prompt、插件与安全策略见 [运行时清单参考](/reference/manifest)。

默认文件名：

- `lania.schemas.ts`
- `lania.schemas.js`
- `lania.schemas.cjs`

目录：

- `.lania/schemas/`

### 最小结构

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      command: { about: 'Ops tools', alias: 'o' },
      commands: [
        {
          name: 'ping',
          about: 'Ping endpoint',
          options: [{ long: 'endpoint', valueKind: 'string', required: true }],
          handler: async (ctx) => ({
            result: { ok: true, input: ctx.argv.options, exitCode: 0 },
            events: [],
          }),
        },
      ],
    },
  ],
};
```

### runtimeCommands

| 字段       | 说明           |
| ---------- | -------------- |
| `mount`    | 父级命令名     |
| `command`  | 父级命令元信息 |
| `commands` | 子命令列表     |

### command

| 字段      | 说明          |
| --------- | ------------- |
| `name`    | 命令名        |
| `about`   | 描述          |
| `alias`   | 别名          |
| `args`    | 位置参数      |
| `options` | 选项          |
| `prompt`  | 补问          |
| `hooks`   | 生命周期 Hook |
| `handler` | 执行函数      |

### option

| 字段           | 说明                             |
| -------------- | -------------------------------- |
| `long`         | 长选项名                         |
| `valueKind`    | `string` / `bool` / `number` 等  |
| `help`         | 帮助文本                         |
| `required`     | 是否必填                         |
| `defaultValue` | 默认值（也兼容 `default_value`） |
| `choices`      | 可选值                           |
| `negatable`    | 是否支持 `--no-` 形式            |

### handler 上下文

```ts
handler: async (ctx) => ({
  result: {
    ok: true,
    input: ctx.argv.options,
    exitCode: 0,
  },
  events: [],
});
```

`ctx` 中包含：

- `cwd`
- `mount` / `path`
- `argv.args` / `argv.options`
- `traceId`
- `tools`
- `scaffold`
- `product`
- `runtime`

`ctx.tools` 的可用范围由 `lan.config.tools`（旧名 `tooling`）控制，而不是 manifest 字段。完整策略与 API 见 [ctx.tools API](/reference/host-tools)。

## 契约生成配置

文件：`lania.contract.yaml`

```yaml
version: 1

defaults:
  language: go
  output:
    contractDir: contracts/generated
    transportDir: transport/generated
    moduleFile: modules/generated_module.gen.go
    manifest: .lania/contracts.lock.json

entries:
  - name: greeter-service
    source:
      kind: proto
      inputs:
        - schemas/proto/greeter.proto
    targets:
      - grpc
      - http
```

### 字段

| 字段                      | 说明                  |
| ------------------------- | --------------------- |
| `version`                 | 配置版本              |
| `defaults.language`       | 目标语言              |
| `defaults.output`         | 输出目录与 manifest   |
| `entries[].name`          | 入口名                |
| `entries[].source.kind`   | `proto` / `thrift` 等 |
| `entries[].source.inputs` | 输入文件              |
| `entries[].targets`       | `grpc` / `http` 等    |

## 模块生成配置

文件：`lania.module.yaml`

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

### 字段

| 字段                 | 说明                                                |
| -------------------- | --------------------------------------------------- |
| `framework.name`     | 后端框架，如 `lania-g`                              |
| `framework.language` | 目标语言                                            |
| `inputs[].name`      | 输入名                                              |
| `inputs[].source`    | `protobuf` / `thrift` / `json` / `yaml` / `graphql` |
| `inputs[].path`      | schema 目录或文件                                   |
| `inputs[].include`   | glob                                                |
| `inputs[].targets`   | `grpc` / `http` / `ws` / `graphql`                  |
| `output.*`           | 输出目录                                            |
| `inject.enabled`     | 是否执行 main.go 注入                               |

## 协议 metadata

schema 内可声明 transport metadata：

- `proto` / `thrift`：单行注释 `// lania:...`
- `json` / `yaml`：`x-lania-operations`

## 下一步

- [代码生成](/guide/generate)
- [生成 HTTP / gRPC 代码](/cookbook/generate-api)
- [生成 Go 模块](/cookbook/generate-module)
