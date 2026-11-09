# 命令总览

<div class="lead">
Lania CLI 的命令可以按执行路径分为四类：Bridge 型、混合型、Workflow 型和运行时扩展型。理解分类有助于判断一条命令出错时应该看 Rust 还是 Node。
</div>

## 命令地图

| 命令      | 别名 | 类型           | 作用                                     |
| --------- | ---- | -------------- | ---------------------------------------- |
| `dev`     | `d`  | Bridge         | 启动项目开发流程（含 `product` 子命令）  |
| `build`   | `b`  | Bridge         | 触发构建或 watch 构建                    |
| `lint`    | `l`  | Bridge         | 执行 Lint 检查或修复                     |
| `tools`   | —    | 混合           | 枚举命令、运行文件、查看文件             |
| `create`  | `c`  | Workflow       | 从模板创建项目（含 `template` 二级命令） |
| `add`     | `a`  | Workflow       | 向现有项目补充模板内容                   |
| `release` | `r`  | Workflow       | 发布规划、执行、恢复与状态查询           |
| `git`     | `gt` | Workflow       | Git 查询、分支/标签、暂存、提交与推送    |
| `config`  | —    | 管理面         | 全局偏好、项目配置与跨包配置统一管理     |
| `env`     | —    | 混合 / TUI     | 查看与管理多语言工具链版本               |
| `pi`      | —    | Bridge / Agent | 编码 agent（交互 REPL 或一次性执行）     |

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

代表命令：`create`、`add`、`release`、`git`。

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

### 三层配置统一管理

配置分三层：项目级 `lan.config.*`、全局偏好 `~/.lania/preferences.json`、包级偏好 `~/.lania/config/<pkg>.json`。`lan config` 是统一管理面（`config` 自身不是命令子包），`generate` 归档后共 11 个命令。

10 个命令子包（add / build / create / dev / env / git / lint / pi / release / tools）各自带 `config` 二级命令 `bare` / `get` / `set` / `init` / `doctor` / `sync`，`get` / `set` 可用 `--global` 切到全局作用域，均有中英双语文案。包级键由各包 `configSchema` 声明：通用 `locale` / `output.mode` / `log.timestamps`，`env` 额外 `shim.host`。

```bash
# 跨包读写、查看、一键下发
lan config get locale --package env
lan config set locale zh --package @lania-cli-v2/cmd-env
lan config packages --json
lan config apply
lan config apply --package env --set output.mode=human
```

`--package` 统一归一化短名 / npm 全名 / bin 名 / crate 名；`apply` 只覆盖已声明的键，且只作用于已安装且启用的子包。详见 [配置文件指南](/guide/configuration) 与 [CLI 命令参考](/reference/cli#config)。

## 按场景选择命令

<div class="command-card">
  <div>
    <strong>项目开发</strong>
    <code>lan workspace dev</code> / <code>lan workspace build</code> / <code>lan workspace lint</code>
  </div>
  <div>
    <strong>脚手架</strong>
    <code>lan workspace create</code> / <code>lan workspace add</code> / <code>lan workspace create template</code>
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
    <code>lan product dev</code> / <code>build</code> / <code>pack</code> / <code>publish</code>
  </div>
</div>

> `generate` 相关命令（`generate api` / `generate module` / `lan product generate`）已下线归档，将在后续版本重新设计；完整代码保存在本地分支 `archive/generate-command`。

## 下一步

- [工具命令 tools](/guide/tools)
- [语言环境管理 env](/guide/env)
- [模板与脚手架](/guide/scaffolding)
- [模板参考](/reference/templates)
- [CLI 命令参考](/reference/cli)
