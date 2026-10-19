# 输出、语言与进度

<div class="lead">
Lania CLI 把“结果”和“日志”分开：stdout 输出结构化结果，stderr 承担人类可读日志与错误。输出模式、语言和进度都可以通过全局偏好或项目配置控制。
</div>

## stdout 与 stderr

```text
stdout  → 结构化结果 / JSON / JSONL
stderr  → 友好错误、日志、进度
```

这样设计的好处：

- 脚本可以安全消费 stdout。
- 人类仍然能在终端看到可读日志。
- 失败时既有错误提示，也有机器可读错误对象。

## 输出模式

```bash
lan config set output.mode json
lan config set output.mode jsonl
lan config set output.mode human
lan config set output.mode stream
```

| 模式     | 行为                              |
| -------- | --------------------------------- |
| `json`   | 一次性输出 pretty JSON            |
| `jsonl`  | 逐行输出事件与最终结果            |
| `human`  | 人类可读文本                      |
| `stream` | 事件流模式，等价于 JSONL 事件输出 |

查看当前配置：

```bash
lan config get
lan config get output.mode
```

## 事件模式

项目配置还可以控制事件是 buffered 还是 stream：

```ts
export default {
  ui: {
    output: {
      mode: 'jsonl',
      events: 'stream',
    },
  },
};
```

- `buffered`：命令结束后统一输出。
- `stream`：执行过程中持续输出事件。

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

## 调试信息

项目配置可以控制是否在输出中包含宿主状态或 Bridge 交换信息：

```ts
export default {
  ui: {
    output: {
      includeHostState: false,
      includeBridgeExchange: false,
    },
  },
};
```

排查问题时可临时打开，但不要在生产流水线中长期开启。

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

### 流式消费

```bash
lan config set output.mode jsonl
```

## 下一步

- [配置字段参考](/reference/config)
- [常见问题](/guide/troubleshooting)
