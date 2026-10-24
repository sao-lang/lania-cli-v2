# 输出与状态文件

<div class="lead">
本页说明 stdout/stderr 约定、输出 profile、退出码，以及 .lania 下的状态与报告文件。
</div>

## 1. stdout / stderr

```text
stdout  → 命令最终结果（统一 JSON envelope / 人类可读）
stderr  → 日志、错误提示、进度
```

设计目标：

- stdout 可被脚本稳定消费
- stderr 可被人阅读
- 失败时仍能拿到机器可读错误对象

## 2. 输出 profile

`OutputProfile` 由三部分合成：

1. 全局偏好 `~/.lania/preferences.json`
2. 项目配置 `ui.output` / `ui.progress`
3. 默认值

### 默认值

| 字段       | 默认      |
| ---------- | --------- |
| `mode`     | `json`    |
| `progress` | `spinner` |
| `locale`   | `en`      |

### 全局偏好映射

| `outputMode`     | 结果                                       |
| ---------------- | ------------------------------------------ |
| `json`           | `mode=json`                                |
| `human`          | `mode=human`                               |
| `stream`/`jsonl` | `mode=json`（旧的流式写法归一为普通 JSON） |

### 项目覆盖

只有显式出现的字段才会覆盖默认值，且 `mode` 只接受 `json` / `human`：

```ts
export default {
  ui: {
    output: {
      mode: 'json',
    },
    progress: {
      style: 'bar',
      grouping: 'operation',
    },
  },
};
```

## 3. 两种输出模式

### json

一次性输出统一 envelope（固定 4 空格缩进）：

```bash
lan config set output.mode json
lan template
```

```json
{
  "kind": "result",
  "command": { "name": "template", "args": {}, "options": {} },
  "cwd": "/path/to/project",
  "exitCode": 0,
  "result": {
    "templates": ["toolkit"]
  },
  "git": { "branch": "main" }
}
```

- `result` 是业务 payload：`template_info` 取模板输出，workflow 取 workflow execution，bridge 取 `response.result`。
- `error` 仅在失败时出现（`{ "message": "..." }`）；失败 envelope 的 `kind` 为 `error`。
- `git` 仅在 git 仓库且能确定分支时出现。

### human

人类可读文本：先输出一行命令摘要（`lan <name>` + args/options），再按 `result.kind` 渲染结果；不会打印 `cwd` / `git` / `exitCode`，也不会 dump `host_state` 或 bridge exchange。

```bash
lan config set output.mode human
```

失败时只输出 `错误: <message>`（英文为 `Error: <message>`）。

### 兼容说明

- `stream` / `jsonl` 是历史写法，会统一归一为 `json`，不再输出逐行事件。
- `config get locale` / `config get output.mode` 这类 `config_value` 结果仍输出裸标量，不包 envelope。

## 4. 退出码

| 退出码 | 含义                              |
| ------ | --------------------------------- |
| `0`    | 成功                              |
| `1`    | lint 失败 / 通用失败              |
| `2`    | 运行时错误、参数错误、bridge 错误 |
| `124`  | timeout                           |
| `130`  | cancelled                         |

工具链自身的退出码也可能透传，例如 lint 适配器返回 1。

## 5. 状态文件

### 全局偏好

```text
~/.lania/preferences.json
```

字段：

```json
{
  "locale": "zh",
  "outputMode": "human",
  "logTimestamps": false
}
```

### 动态命令与 schema

| 路径                      | 用途                   |
| ------------------------- | ---------------------- |
| `lania.schemas.ts/js/cjs` | 默认 manifest          |
| `.lania/schemas/`         | 额外 manifest 目录     |
| `lania.contract.yaml`     | `generate api` 输入    |
| `lania.module.yaml`       | `generate module` 输入 |

### 生成状态

| 路径                          | 用途             |
| ----------------------------- | ---------------- |
| `.lania/contracts.lock.json`  | 契约生成输出跟踪 |
| `.lania/module-gen.lock.json` | 模块生成输出跟踪 |

这些 manifest 用于：

- 判断文件是否由工具生成
- 计算 stale 文件
- 支持 `--check`
- 支持 `--clean`
- 避免覆盖未受管理文件

### 发布状态

| 路径                        | 用途                         |
| --------------------------- | ---------------------------- |
| `.lania/release-state.json` | release plan/run/resume 状态 |

记录：

- 当前阶段
- 已完成阶段
- 失败信息
- 版本、profile、env、channel
- 执行命令与结果

### 产品产物

| 路径                                                       | 用途              |
| ---------------------------------------------------------- | ----------------- |
| `.lania/build/product/`                                    | 产品构建快照      |
| `.lania/build/product/build-report.json`                   | 构建报告          |
| `.lania/pack/product/install-root/`                        | install-root 布局 |
| `.lania/pack/product/install-root/pack-report.json`        | 打包报告          |
| `.lania/publish/product/npm-package/`                      | npm 发布包        |
| `.lania/publish/product/npm-package/publish-report.json`   | 发布报告          |
| `.lania/publish/product/npm-package/publish-manifest.json` | 发布清单          |
| `.lania/inspect/product/compat-report.json`                | 兼容性报告        |

### 示例/项目报告

项目可能写入：

```text
.lania/reports/*.json
.lania/tmp/*
```

这些不是 CLI 的固定协议，由项目自己的动态命令决定。

## 6. 清理建议

以下目录通常不应提交：

```text
node_modules/
.lania/build/
.lania/pack/
.lania/publish/
.lania/tmp/
dist/
coverage/
```

以下文件通常应该提交：

```text
lania.schemas.*
lania.contract.yaml
lania.module.yaml
.lania/contracts.lock.json
.lania/module-gen.lock.json
```

发布状态 `.lania/release-state.json` 是否提交取决于团队策略，一般不建议。

## 下一步

- [配置文件](/guide/configuration)
- [配置字段参考](/reference/config)
- [发布与同步](/guide/release)
