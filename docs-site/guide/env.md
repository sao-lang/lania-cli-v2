# 语言环境管理 env

<div class="lead">
`lan env` 是 lania **自己实现**的运行时管理器：安装、切换、卸载全部由 lania 完成，不再转发给 nvm / fnm / volta / pyenv / rustup 等外部工具。它只管理运行时本身（下载官方预编译产物、维护版本目录与 current 指针），不接管任何语言的全局包。
</div>

## 设计取舍

| 维度     | 行为                                                                        |
| -------- | --------------------------------------------------------------------------- |
| 安装来源 | 直接下载上游官方产物（nodejs.org / go.dev / static.rust-lang.org / …）      |
| 目录     | 全部落在 `~/.runex/`（可用 `RUNEX_HOME` 覆盖），与项目级 `.lania/` 完全分离 |
| 切换方式 | 修改 `state.json` 里的 current 指针 + 重写 shim，**不改用户 PATH**          |
| 全局包   | **不接管**。`npm i -g` / `pip install` 仍装在对应版本目录内，切版本即隔离   |
| 外部安装 | 只做探测展示（`source: "system"`），不引入、不修改                          |

## 两种使用模式

`lan env` 同时提供 **命令行** 与 **终端 TUI** 两种入口，二者共用同一套探测与执行逻辑：

| 模式     | 入口                   | 适合场景                           |
| -------- | ---------------------- | ---------------------------------- |
| 命令行   | `lan env <subcommand>` | 脚本、CI、远程终端、精确可控的操作 |
| 终端 TUI | `lan env ui`           | 一眼看全所有语言版本、键盘快速切换 |

```bash
lan env ui                                        # 启动 TUI（需要交互式终端）
lan env ui --snapshot --cols 100 --rows 28         # 非交互：输出一帧纯文本（CI / 调试）
```

### 独立 CLI：`runex`

同一套运行时管理能力还提供**独立可执行文件** `runex`（与 nvm / fnm 同类），不依赖 lania 宿主，
也不需要 Node 运行时：

```bash
cargo build --release -p runex     # 单文件产物：rust/target/release/runex
runex install node 22              # 下载 + 校验 + 解压 + 生成 shim
runex use node 22                  # 切换 current
runex list --installed             # 只看自管版本
runex ui                           # 终端 TUI（与 lan env ui 同一套实现）
```

`runex` 同时提供 **Rust API**，宿主 / GUI / IDE 插件可以直接嵌入，不需要 spawn 子进程：

```rust
use runex::api::Runex;

let api = Runex::resolve()?;
api.install("node", "22", &mut |event| { /* 进度 */ })?;
api.use_version("node", "22")?;
let tools = api.scan();
```

语义与 `lan env` 完全一致（共用同一套 `runex` API / Store / Provider），唯一差异是执行策略：
`lan env` 默认 dry-run、需要 `--yes`；`runex` 默认直接执行，`--dry-run` 才预览。
`--json` 供脚本与 GUI 使用，`--home` 可覆盖 `RUNEX_HOME`。`lan env` 的运行时管理动作也全部
委托给 `runex`，命令层只保留宿主注册、参数与输出。命令面、shim 与分发方式见设计文档 §12 / §13。

### 一次性接入 PATH

```bash
lan env init                # 按当前 shell 打印片段
lan env init --shell fish   # 指定 shell：bash / zsh / fish / powershell
```

它只做一件事：把 `~/.runex/shims` 放到 PATH 最前面。之后 `node` / `python3` / `go` 等命令会自动指向 current 版本。

### TUI 界面

界面风格参考 [yazi](https://github.com/sxyazi/yazi)：圆角边框、分组图标、分段状态栏、弹出式确认框。

```text
 lan env  · toolchains
╭ 工具 ──────────────────────────╮ ╭ Node.js ──────────────────────────────────────────────────────╮
│ ❯ ◈ Node.js    20.11.1         │ │                                                               │
│   ◈ Deno       未安装          │ │   20.11.1  Node.js                                            │
│   ◈ Bun        未安装          │ │                                                               │
│   ◈ Python     3.12.4          │ │   可执行文件        ~/.runex/runtimes/node/20.11.1/bin/node   │
│   ◈ Rust       未安装          │ │   当前来源          lania 自管                                │
│   ◈ Go         1.22.12         │ │   自管状态          nodejs.org/dist                           │
│   ◈ Zig        未安装          │ │   自管版本数        2                                         │
│   …                            │ │   项目版本约束      无                                        │
│                                │ │                                                               │
│                                │ │   已安装版本  (2)                                             │
│                                │ │      22.21.1 lania  20.11.1 lania                             │
│                                │ │                                                               │
│                                │ │   入口命令                                                    │
│                                │ │      node  npm  npx  corepack                                 │
╰────────────────────────────────╯ ╰───────────────────────────────────────────────────────────────╯
  i  install  u  use  d  remove  p  pin  v  versions  D  doctor  r  reload  q  quit
 NORMAL  78 个工具  /path/to/project
```

**右侧「已安装版本」默认就会加载**，只读 lania 自己的清单（`~/.runex/runtimes/<语言>/<版本>`），不联网、不调用外部命令。

界面顶部固定展示当前工作目录、git 仓库名与分支、设备系统/主机信息，并用加粗和不同颜色区分；状态栏显示当前模式（`NORMAL` / `INPUT` / `CONFIRM` / `RESULT`）与提示消息。包管理器（npm/pip/cargo 等）会紧跟在其所属语言下方。

### TUI 快捷键

| 按键                         | 作用                             |
| ---------------------------- | -------------------------------- |
| `↑` `↓` / `k` `j`            | 移动选择                         |
| `PgUp` `PgDn` / `Home` `End` | 快速跳转                         |
| `J` / `K`                    | 滚动右侧详情                     |
| `i`                          | 安装（弹窗输入版本）             |
| `u`                          | 切换 current 版本                |
| `d`                          | 卸载版本                         |
| `p`                          | 切换「写入项目版本文件」         |
| `v`                          | 重新读取该工具已安装的版本       |
| `D`                          | 体检（缺失运行时 / Shim / PATH） |
| `r`                          | 刷新全部状态                     |
| `q` / `Esc`                  | 退出（`Ctrl+C` 亦可）            |

所有变更操作都会先弹出 **确认框** 显示将执行的命令（等价 CLI 的 dry-run），按 `Enter` 才真正执行、按 `Esc` 取消；执行结果在 `RESULT` 弹窗中展示。

### 图标与字体

列表中的语言图标使用 [Nerd Font](https://www.nerdfonts.com/) 私有区字符，Deno/Bun 分别使用官方 Devicons `U+E7C0` / `U+E76F`。程序会自动探测：

- Windows Terminal / VS Code 终端等已知支持的环境 → 启用图标
- 其他环境 → 回退为 `◈`，不会出现方框乱码

字形大小由所安装的 Nerd Font 决定（Deno 的官方字形在部分字体里偏小）；如果图标缺字或大小不一致，建议安装 Nerd Fonts v3.4+ 的 patched 字体，或使用等宽的纯文本图标：

```bash
LANIA_ENV_ICONS=nerd  lan env ui   # 强制官方图标
LANIA_ENV_ICONS=ascii lan env ui   # 强制等宽纯文本 ◈
```

## 支持的语言

`lan env` 认识 **40 种**语言 / 运行时，但**当前只有第一批支持自管安装**：

### 第一批（已实现自管下载）

| 语言   | 上游                                 | 产物形态                            |
| ------ | ------------------------------------ | ----------------------------------- |
| Node   | nodejs.org/dist                      | `tar.gz` / `zip`（各平台矩阵）      |
| Python | python-build-standalone（astral-sh） | `tar.gz`（`install_only` 可重定位） |
| Go     | go.dev/dl                            | `tar.gz` / `zip`，固定前缀 `go/`    |
| Rust   | static.rust-lang.org                 | `tar.gz`，rust + cargo 两个组件包   |
| Deno   | github.com/denoland/deno/releases    | `zip` 单文件                        |
| Bun    | github.com/oven-sh/bun/releases      | `zip` 单文件                        |
| Zig    | ziglang.org/download                 | `tar.xz` 单目录                     |

其余语言仍可被 `lan env list` / `info` **探测**（显示本机系统安装的版本），但 `install` / `use` / `remove` 会明确报错：

```text
$ lan env install ruby 3.3.0 --yes
Error: ruby is not self-managed yet; available now: node, python, go, rust, deno, bun, zig
```

每种语言的入口命令、上游渠道与平台矩阵集中定义在 `runex-core/src/provider/` 下，新增语言只需实现一个 `RuntimeProvider`。

## 查询

### 查看全部工具链

```bash
lan env list
lan env list node            # 按名称过滤
lan env list --installed     # 只看 lania 自己安装的版本（不联网）
```

每个条目包含：当前版本、可执行文件路径、版本来源（`lania` / `system`）、是否自管、上游渠道、自管版本数、项目版本约束。

### 查看单个工具

```bash
lan env info node
```

额外返回 `provider`（该语言的上游能力与平台支持情况）与 `installedVersions`。

### 查看已安装版本

```bash
lan env versions node             # 只读 ~/.runex
lan env versions node --available # 额外查询上游可安装版本（需要网络）
```

### 解析入口真实路径

```bash
lan env which node            # shim 用它定位当前二进制
lan env which npm --resolve   # 只输出路径，便于脚本拼接
```

### 体检

```bash
lan env doctor
```

输出四部分：

- `managed`：已自管的语言及其 current 版本、上游、版本数
- `missing`：受支持但本机没有的语言运行时
- `external`：来自 lania 之外（系统包管理器 / 手动安装）的运行时
- `diagnostics`：`~/.runex/shims` 是否在 PATH、current 指针是否指向已删除目录、shim 是否缺失

## 变更操作

变更类命令默认 **只输出计划（dry-run）**，不会真正执行；确认无误后加 `--yes` 才执行。

```bash
# 1. 先看计划
lan env install node 20
lan env use python 3.12 --project
lan env remove node 18

# 2. 确认后执行
lan env install node 20 --yes
lan env use python 3.12 --project --yes
lan env remove node 18 --yes
```

### 安装

```bash
lan env install <tool> <version> [--yes]
```

版本支持前缀：`lan env install node 20` 会装 20.x 里最高的稳定版本。
带预发布后缀的版本（`1.22rc2`、`3.13.0b1`）只有在没有对应稳定版时才会被选中。

安装流程（所有语言一致）：

```text
查询上游版本清单 → 解析本平台产物 → 下载到 ~/.runex/cache/downloads（命中则跳过）
  → 校验官方 sha256（缺失时校验归档完整性）→ 解压到 cache/staging → 按 Provider 布局摊平
  → 写入 runtimes/<语言>/<版本>（含 .runex-meta.json）→ 更新 state.json → 生成 shims
```

首个安装的版本会自动成为 current，无需再执行 `use`。

### 切换 / 固定版本

```bash
lan env use node 20              # 全局切换（改 current 指针 + 重写 shim）
lan env use node 20 --project    # 同时写入项目版本文件
```

`--project` 写入的文件随语言不同：

| 语言   | 文件                  |
| ------ | --------------------- |
| Node   | `.nvmrc`              |
| Python | `.python-version`     |
| Rust   | `rust-toolchain.toml` |
| Go     | `go.mod`（读取 `go`） |
| Deno   | 无                    |
| Bun    | `.bun-version`        |
| Zig    | `.zigversion`         |

切换后 shim 立即生效，**不需要重开终端**。

### 卸载

```bash
lan env remove node 18 --yes
```

只删除 lania 自己管理的目录。如果删的是 current 版本，会自动回退到该语言剩下的最高版本；删到没有剩余版本时会清掉对应 shim。

## 目录布局

```text
~/.runex/                      # RUNEX_HOME 可覆盖
├── runtimes/<语言>/<版本>/     # 解压后的运行时 + .runex-meta.json
├── shims/                     # 注入 PATH 的入口（POSIX 脚本 + Windows .cmd）
├── cache/downloads/           # 产物缓存（按 URL 摘要命名）
├── cache/staging/             # 安装临时目录
└── state.json                 # current 指针 + 每个语言的入口与环境变量
```

`.runex-meta.json` 记录版本来源，便于审计：

```json
{
  "language": "node",
  "version": "20.11.1",
  "tag": "v20.11.1",
  "upstream": "nodejs.org/dist",
  "artifact": "node-v20.11.1-win-x64.zip",
  "sha256": "…",
  "binDir": ".",
  "env": [],
  "entrypoints": ["node", "npm", "npx", "corepack"],
  "platform": "windows-x64",
  "installedAt": "2026-10-25T02:00:00Z"
}
```

### 环境变量

| 变量                          | 作用                                                            |
| ----------------------------- | --------------------------------------------------------------- |
| `RUNEX_HOME`                  | 覆盖 `~/.runex` 根目录                                          |
| `RUNEX_OFFLINE=1`             | 禁用一切网络访问（已安装版本仍可列举/切换）                     |
| `RUNEX_HTTP_TIMEOUT_SECS`     | 连接/单次读取超时，默认 30                                      |
| `RUNEX_DOWNLOAD_TIMEOUT_SECS` | 单个产物下载总时限，默认 1800                                   |
| `GITHUB_TOKEN` / `GH_TOKEN`   | 提升 GitHub API 配额（60 → 5000 次/小时），Python/Deno/Bun 需要 |

## 从旧版本迁移

旧版 `lan env` 会转发给本机已有的 nvm / volta / pyenv 等。新版只有自管一条路径：

- `--manager` / `-m` 选项已移除，`lan env` 会直接报错提示
- 已由外部管理器安装的运行时仍会出现在 `lan env list`（`source: system`），但不会被接管
- 需要继续使用某个版本时，用 `lan env install <tool> <版本>` 让 lania 自己装一份

## 安全设计

- **dry-run 默认**：任何变更都先打印将要执行的计划（TUI 中同样先显示计划）。
- **版本号白名单**：版本参数只允许字母、数字与 `. - _ + @ /`，且拒绝 `..`，避免路径穿越与命令注入（`20;whoami`、`../../escape` 都会被直接拒绝）。
- **不拼接 shell**：安装/切换/卸载全部是 lania 内部动作，不经过 shell。
- **校验和不放水**：上游提供 sha256 时逐一校验；上游缺失校验和会给出 warning，并额外做归档完整性校验（防止 CDN 静默截断）。

## 下一步

- [命令总览](/guide/commands)
- [CLI 命令参考](/reference/cli)
- [环境变量](/reference/environment)
