# 命令总览

<div class="lead">
Lania CLI 的命令可以按执行路径分为四类：Bridge 型、混合型、Workflow 型和运行时扩展型。理解分类有助于判断一条命令出错时应该看 Rust 还是 Node。
</div>

## 命令地图

| 命令       | 别名 | 类型              | 作用                                  |
| ---------- | ---- | ----------------- | ------------------------------------- |
| `dev`      | `d`  | Bridge            | 启动项目开发流程                      |
| `build`    | `b`  | Bridge            | 触发构建或 watch 构建                 |
| `lint`     | `l`  | Bridge            | 执行 Lint 检查或修复                  |
| `tools`    | —    | 混合              | 枚举命令、运行文件、查看文件          |
| `create`   | `c`  | Workflow          | 从模板创建项目                        |
| `add`      | `a`  | Workflow          | 向现有项目补充模板内容                |
| `template` | `t`  | Bridge 查询       | 列出模板或查看详情                    |
| `generate` | `g`  | Workflow          | 契约与模块代码生成                    |
| `release`  | `r`  | Workflow          | 发布规划、执行、恢复与状态查询        |
| `git`      | `gt` | Workflow          | Git 查询、分支/标签、暂存、提交与推送 |
| `config`   | —    | Workflow          | 管理全局 CLI 偏好                     |
| `env`      | —    | 混合 / TUI        | 查看与管理多语言工具链版本            |
| `product`  | —    | Bridge / Workflow | 产品 CLI 生成、构建、打包、发布与诊断 |

## 四类执行路径

### Bridge 型

```text
Rust 解析命令 → Node Bridge → compiler / lint 插件 → JS 工具链
```

代表命令：`dev`、`build`、`lint`。

### 混合型

`tools` 是混合命令：

- `tools list` 走 Node Bridge，枚举终端可解析命令。
- `tools run` / `tools view` 走 Rust 宿主本地能力。

### Workflow 型

```text
Rust 解析命令 → lania-workflows → 宿主服务 / Node Bridge 能力
```

代表命令：`create`、`add`、`generate`、`release`、`git`。

### 运行时扩展型

```text
lan.config.* → Node Bridge 扫描 manifest → Rust 注册命令 → 执行时转发回 Node
```

代表能力：`lania.schemas.*` 中声明的动态命令。

## 全局行为

### 帮助与版本

```bash
lan --help
lan help
lan --version
```

### 输出模式

```bash
lan config set output.mode json
lan config set output.mode human
```

- `json`：统一结果 envelope，默认模式。
- `human`：更适合终端阅读的文本输出。
- 旧的 `stream` / `jsonl` 写法会归一为 `json`。

### 语言

```bash
lan config set locale zh
lan config set locale en
lan config get locale
```

### 进度展示

```bash
lan config set output.progress spinner
lan config set output.progress bar
lan config set output.progress none
```

具体字段以 `lan config get` 与 [配置字段参考](/reference/config) 为准。

## 按场景选择命令

<div class="command-card">
  <div>
    <strong>项目开发</strong>
    <code>lan dev</code> / <code>lan build</code> / <code>lan lint</code>
  </div>
  <div>
    <strong>脚手架</strong>
    <code>lan create</code> / <code>lan add</code> / <code>lan template</code>
  </div>
  <div>
    <strong>代码生成</strong>
    <code>lan generate api</code> / <code>lan generate module</code>
  </div>
  <div>
    <strong>发布</strong>
    <code>lan release</code> / <code>lan git sync</code>
  </div>
  <div>
    <strong>本地工具</strong>
    <code>lan tools list</code> / <code>lan tools run</code> / <code>lan tools view</code>
  </div>
  <div>
    <strong>语言环境</strong>
    <code>lan env list</code> / <code>lan env install</code> / <code>lan env ui</code>
  </div>
  <div>
    <strong>产品化</strong>
    <code>lan product generate</code> / <code>build</code> / <code>pack</code> / <code>publish</code>
  </div>
</div>

## 下一步

- [工具命令 tools](/guide/tools)
- [语言环境管理 env](/guide/env)
- [模板与脚手架](/guide/scaffolding)
- [代码生成](/guide/generate)
- [CLI 命令参考](/reference/cli)
