# 输出、语言与进度

<div class="lead">
Lania CLI 把“结果”和“日志”分开：stdout 输出结构化结果，stderr 承担人类可读日志与错误。输出模式、语言和进度都可以通过全局偏好或项目配置控制。
</div>

## stdout 与 stderr

```text
stdout  → 统一结果 envelope（JSON）或人类可读结果
stderr  → 友好错误、日志、进度
```

这样设计的好处：

- 脚本可以安全消费 stdout。
- 人类仍然能在终端看到可读日志。
- 失败时既有错误提示，也有机器可读错误对象。

## 输出模式

```bash
lan config set output.mode json
lan config set output.mode human
```

| 模式    | 行为                                  |
| ------- | ------------------------------------- |
| `json`  | 一次性输出统一 envelope（4 空格缩进） |
| `human` | 人类可读文本（命令摘要 + 结果）       |

旧的 `stream` / `jsonl` 写法会归一为 `json`，不会再有逐行事件输出。

查看当前配置：

```bash
lan config get
lan config get output.mode
```

## 统一结果 envelope

`json` 模式只输出一个 envelope，便于脚本稳定消费：

```json
{
  "kind": "result",
  "command": { "name": "env list", "args": {}, "options": {} },
  "cwd": "/path/to/project",
  "exitCode": 0,
  "result": {},
  "git": { "branch": "main" }
}
```

- `command.name` 是完整子命令路径（空格拼接）。
- `error` 仅在失败时出现，内容形如 `{ "message": "..." }`。
- `git` 仅在 git 仓库且能确定分支时出现。
- `result` 是命令的业务结果；`template_info` / `workflow` / bridge 调用都收敛到这里。

失败时 stderr 保留友好错误，stdout 输出：

```json
{
  "kind": "error",
  "cwd": "/path/to/project",
  "exitCode": 2,
  "error": { "message": "unknown command: `lan app`" }
}
```

例外：`config get locale` / `config get output.mode` 这类 `config_value` 结果仍直接输出裸标量，方便 shell 脚本读取。

## 语言

```bash
lan config get locale
lan config set locale zh
lan config set locale en
```

解析优先级：

1. 项目配置中的 locale（如果存在）
2. 全局偏好 `locale`
3. 默认 `en`

帮助文本、错误提示和部分日志会随语言切换。

## 进度

```bash
lan config set output.progress spinner
lan config set output.progress bar
lan config set output.progress none
```

项目配置：

```ts
export default {
  ui: {
    progress: {
      mode: 'spinner',
    },
  },
};
```

- `spinner`：单行动画，适合大多数命令。
- `bar`：进度条，适合文件写入、批量生成。
- `none`：关闭进度，适合 CI 或日志收集。

## 日志时间戳

```bash
lan config set log.timestamps true
lan config set log.timestamps false
```

时间戳只影响人类可读日志，不影响结构化结果。

## 推荐组合

### 本地开发

```bash
lan config set output.mode human
lan config set output.progress spinner
lan config set locale zh
```

### CI

```bash
lan config set output.mode json
lan config set output.progress none
```

## 下一步

- [配置字段参考](/reference/config)
- [常见问题](/guide/troubleshooting)
