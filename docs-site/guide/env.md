# 语言环境管理 env

<div class="lead">
`lan env` 用一套统一的入口管理 Node、Python、Rust、Go、Dart、Kotlin、Java、C/C++ 等常用语言的工具链版本。它不自己造安装器，而是复用生态里成熟的版本管理器（nvm / fnm / volta / pyenv / uv / rustup / sdkman / scoop / winget / brew 等），因此已有的工作流不会被打破。
</div>

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

### TUI 界面

界面风格参考 [yazi](https://github.com/sxyazi/yazi)：圆角边框、分组图标、分段状态栏、弹出式确认框。

```text
 lan env  · toolchains
╭ 工具 ──────────────────────────╮ ╭ Node.js ──────────────────────────────────────────────────────╮
│ ❯ ◈ Node.js    24.12.0         │ │                                                               │
│   ◈ Deno       未安装          │ │   24.12.0  Node.js                                            │
│   ◈ Bun        1.3.0           │ │                                                               │
│   ◈ Python     3.14.2          │ │   可执行文件        /usr/local/bin/node                       │
│   ◈ Rust       1.86.0          │ │   当前来源          系统安装                                  │
│   ◈ Go         1.25.4          │ │   安装将使用        volta                                     │
│   ◈ Zig        未安装          │ │   安装根目录        ~/AppData/Local/Volta                     │
│   ◈ Dart       3.1.5           │ │   项目版本约束      无                                        │
│   ◈ Java       未安装          │ │                                                               │
│   ◈ Kotlin     未安装          │ │   已安装版本  (2)                                             │
│   ◈ Scala      未安装          │ │      18.20.8 volta  22.21.1 volta                             │
│   ◈ Groovy     未安装          │ │                                                               │
│   ◈ C          未安装          │ │   版本管理器                                                  │
│   ◈ C++        未安装          │ │     ○ nvm   ○ fnm   ● volta   ○ n                             │
│   …                            │ │     ○ asdf   ○ mise   ○ scoop   ● winget                      │
╰────────────────────────────────╯ ╰───────────────────────────────────────────────────────────────╯
  i  install  u  use  d  remove  m  manager  p  pin  v  versions  D  doctor  r  reload  q  quit
 NORMAL  78 个工具  /path/to/project
```

**右侧「已安装版本」默认就会加载**，无需手动按键；它会遍历该语言所有可用的版本管理器，
因此即使当前 `node` 来自系统安装，也能看到 nvm / volta 里装过的历史版本。

状态栏会显示当前模式（`NORMAL` / `INPUT` / `CONFIRM` / `RESULT`）、提示消息、工作目录与项目锁定状态。

### TUI 快捷键

| 按键                         | 作用                                     |
| ---------------------------- | ---------------------------------------- |
| `↑` `↓` / `k` `j`            | 移动选择                                 |
| `PgUp` `PgDn` / `Home` `End` | 快速跳转                                 |
| `J` / `K`                    | 滚动右侧详情                             |
| `i`                          | 安装（弹窗输入版本）                     |
| `u`                          | 切换 / 设为默认版本                      |
| `d`                          | 卸载版本                                 |
| `m`                          | 循环切换版本管理器（自动 → nvm → fnm …） |
| `p`                          | 切换「写入项目版本文件」                 |
| `v`                          | 查看该工具已安装的版本                   |
| `D`                          | 体检（缺失工具 / 非管理器来源）          |
| `r`                          | 刷新全部状态                             |
| `q` / `Esc`                  | 退出（`Ctrl+C` 亦可）                    |

所有变更操作都会先弹出 **确认框** 显示将执行的命令（等价 CLI 的 dry-run），按 `Enter` 才真正执行、按 `Esc` 取消；执行结果在 `RESULT` 弹窗中展示。

> 界面中「当前来源」与「安装将使用」是两件事：前者表示当前二进制实际由谁托管（系统安装会显示「系统安装」），后者表示执行 `i` / `u` 时会调用哪个管理器。

### 图标与字体

列表中的语言图标使用 [Nerd Font](https://www.nerdfonts.com/) 私有区字符。程序会自动探测：

- Windows Terminal / VS Code 终端等已知支持的环境 → 启用图标
- 其他环境 → 回退为 `◈`，不会出现方框乱码

也可以显式指定：

```bash
LANIA_ENV_ICONS=nerd  lan env ui   # 强制图标
LANIA_ENV_ICONS=ascii lan env ui   # 强制纯文本
```

## 支持的语言

当前支持 **40 种**语言 / 运行时：

| 分类         | 语言                                                             |
| ------------ | ---------------------------------------------------------------- |
| JS/TS 生态   | Node.js、Deno、Bun                                               |
| 系统与编译型 | Rust、Go、Zig、C、C++、Dart、D、V、Fortran                       |
| JVM          | Java、Kotlin、Scala、Groovy、Clojure                             |
| .NET         | C#、F#                                                           |
| 脚本         | Python、Ruby、PHP、Perl、Lua、Julia、R、Nim                      |
| 函数式       | Elixir、Erlang、Haskell、OCaml、Crystal、Swift、Gleam、Elm、Haxe |
| 区块链       | Solidity                                                         |
| 国产 / 新兴  | MoonBit、Cangjie（仓颉）、ArkTS（鸿蒙）                          |

每种语言的探测方式、候选管理器、附属工具与项目版本文件集中定义在
[lania-env/language.rs](https://github.com/) 的 `spec()` 中，新增语言只需补一处映射。

## 查询

### 查看全部工具链

```bash
lan env list
lan env list node          # 按名称过滤
lan env -m rustup list     # 只看由 rustup 管理的工具
```

每个条目包含：当前版本、可执行文件路径、命中的版本管理器、安装根目录、项目版本约束。

### 查看单个工具

```bash
lan env info node
```

会额外返回该工具已安装的版本列表。

### 查看已安装版本

```bash
lan env versions node
lan env versions python -m uv
```

### 体检

```bash
lan env doctor
```

输出三部分：

- `managers`：本机可用的版本管理器及其根目录
- `missing`：受支持但未安装的语言运行时
- `conflicts`：二进制来自已知管理器之外的路径（可能是 PATH 顺序问题）

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
lan env install <tool> <version> [--manager <m>] [--yes]
```

按语言选择管理器：

| 语言          | 默认管理器顺序                                                    |
| ------------- | ----------------------------------------------------------------- |
| Node          | nvm → fnm → volta → n → asdf → mise → scoop → winget → brew → apt |
| Python        | pyenv → uv → asdf → mise → scoop → winget → brew → apt            |
| Rust          | rustup                                                            |
| Go            | go（官方 toolchain）→ asdf → mise → scoop → winget → brew → apt   |
| Dart          | fvm → asdf → scoop → winget → brew → apt                          |
| Kotlin / Java | sdkman → asdf → mise → scoop → winget → brew → apt                |
| C/C++         | scoop → winget → brew → apt                                       |

### 切换 / 固定版本

```bash
lan env use node 20              # 全局切换
lan env use node 20 --project    # 写入项目版本文件（.nvmrc / .python-version / rust-toolchain…）
```

`--project` 写入的文件随语言不同：

| 语言          | 文件                       |
| ------------- | -------------------------- |
| Node          | `.nvmrc`                   |
| Python        | `.python-version`          |
| Rust          | `rust-toolchain.toml`      |
| Go            | `go.mod`（读取 `go` 指令） |
| Dart          | `.fvmrc`                   |
| Kotlin / Java | `.sdkmanrc`                |

切换版本后通常需要重开终端（或 `source` 对应初始化脚本）才能生效，计划输出里会给出提示。

### 卸载

```bash
lan env remove node 18 --yes
```

## 安全设计

- **dry-run 默认**：任何变更都先打印将要执行的命令（TUI 中同样先显示计划）。
- **版本号白名单**：版本参数只允许字母、数字与 `. - _ + @ /`，防止 `nvm` / `sdkman` 这类 shell 函数调用被注入（例如 `20;whoami` 会被直接拒绝）。
- **不走字符串拼接**：除 nvm / sdkman 的 shell 函数外，所有步骤都以 `program + args` 执行。

## 下一步

- [命令总览](/guide/commands)
- [CLI 命令参考](/reference/cli)
- [环境变量](/reference/environment)
