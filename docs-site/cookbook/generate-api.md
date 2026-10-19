# 生成 HTTP / gRPC 代码

<div class="lead">
目标：从 proto / thrift schema 生成契约 DTO 与传输层代码，并维护生成 manifest。
</div>

## 步骤 1：初始化

```bash
mkdir my-service
cd my-service
lan generate api init
```

生成：

```text
lania.contract.yaml
schemas/proto/greeter.proto
contracts/generated/
transport/generated/
modules/
```

## 步骤 2：编辑配置

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

## 步骤 3：规划

```bash
lan generate api plan
```

输出会说明：

- 选中入口
- 目标
- 写入 / 未变化 / 冲突 / stale 文件数量
- manifest 位置

## 步骤 4：生成

```bash
lan generate api
```

## 步骤 5：按条件生成

```bash
lan generate api --source proto --target grpc
lan generate api --entry greeter-service --target http
```

## 步骤 6：校验

```bash
lan generate api --check
```

适合放入 CI，检查生成物是否落后于 schema。

## 步骤 7：清理

```bash
lan generate api --clean
```

只清理 manifest 中记录、且已不再需要的 stale 文件。

## 预期结果

- 生成文件写入配置目录。
- `.lania/contracts.lock.json` 记录产物。
- `--check` 在无差异时成功，有差异时失败。
- `--clean` 不误删未受管理文件。

## 下一步

- [生成 Go 模块](/cookbook/generate-module)
- [代码生成指南](/guide/generate)
- [Schema 与协议](/reference/schemas)
