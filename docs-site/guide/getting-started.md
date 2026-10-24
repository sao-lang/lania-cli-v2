# 快速上手

<div class="lead">
本页用最短路径跑通 Lania CLI：确认环境、查看帮助、创建项目、启动开发、构建、Lint，并了解下一步该读什么。
</div>

## 前置要求

| 工具    | 建议版本     | 用途                        |
| ------- | ------------ | --------------------------- |
| Node.js | 20+          | Node Bridge、前端工具链     |
| pnpm    | 10.x         | 仓库依赖与脚本              |
| Rust    | 1.80+        | 编译 CLI 宿主               |
| Git     | 任意较新版本 | `git`、`release` 与版本管理 |

验证环境：

```bash
node -v
pnpm -v
cargo -V
rustc -V
git --version
```

## 方式一：从仓库运行

如果你正在维护 Lania CLI 本身：

```bash
# 安装 TypeScript 侧依赖
cd ts
pnpm install
pnpm build

# 构建 Rust CLI
cd ../rust
cargo build -p lania-cli

# 使用 debug 二进制
./target/debug/lania-cli --help
```

Windows PowerShell：

```powershell
.\rust\target\debug\lania-cli.exe --help
```

## 方式二：使用已安装的 lan

如果已经通过 npm 安装：

```bash
lan --help
```

安装与分发细节见 [安装与运行原理](/guide/installation)。

## 第一条命令

查看命令树：

```bash
lan --help
```

你会看到：

```text
Usage: lan [COMMAND]

Commands:
  dev       Start the project development workflow [aliases: d]
  product   Product-oriented CLI workflows and distribution commands
  build     Run the project build workflow [aliases: b]
  lint      Run project lint checks [aliases: l]
  create    Create a new project from a template [aliases: c]
  add       Add generated content into an existing workspace [aliases: a]
  generate  Generate project artifacts [aliases: g]
  release   Project release orchestration [aliases: r]
  git       Inspect and sync git state [aliases: gt]
  template  List templates or inspect a template detail [aliases: t]
  tools     List commands, run files by type, or view local files
  config    Manage global CLI preferences
```

## 创建项目

先预览模板会生成什么：

```bash
lan create --name demo-app --template spa-vue --preview
```

确认无误后真正创建：

```bash
lan create --name demo-app --template spa-vue
cd demo-app
```

常用参数：

| 参数                | 作用                                                     |
| ------------------- | -------------------------------------------------------- |
| `--name`            | 项目名                                                   |
| `--template`        | `spa-react` / `spa-vue` / `toolkit` / `toolkit-monorepo` |
| `--package-manager` | `npm` / `pnpm` / `yarn` / `bun`                          |
| `--skip-install`    | 跳过依赖安装                                             |
| `--preview`         | 预览渲染结果并隐含 dry-run                               |
| `--dry-run`         | 只规划，不写文件                                         |

## 开发、构建与检查

进入项目后：

```bash
lan dev
lan build
lan lint
```

- `lan dev` 启动开发服务器，默认端口 `8089`。
- `lan build` 执行项目构建，可通过 `--watch`、`--mode`、`--output-dir` 调整。
- `lan lint` 执行已配置的 Lint 适配器；`lan lint fix` 进入修复模式。

## 查看模板

```bash
lan template
lan template toolkit
```

`template` 查询由 Rust 发起，通过 Node Bridge 读取模板元数据。

## 运行单个文件

```bash
lan tools run ./scripts/demo.py
lan tools run ./scripts/demo.ts arg1 arg2
lan tools run ./scripts/demo.rs
```

`tools run` 会优先读取 shebang，否则按扩展名选择运行时。更多规则见 [工具命令 tools](/guide/tools)。

## 结构化管理配置

查看当前全局配置：

```bash
lan config get
```

切换中文：

```bash
lan config set locale zh
```

切换输出模式：

```bash
lan config set output.mode human
```

## 推荐学习路径

1. [创建第一个项目](/guide/first-project)
2. [命令总览](/guide/commands)
3. [工具命令 tools](/guide/tools)
4. [配置文件](/guide/configuration)
5. [整体架构](/architecture/overview)

## 常见问题

### `lan` 找不到

确认 npm 全局 bin 目录在 `PATH` 中，或直接使用仓库内的 debug 二进制。

### Windows 下包管理器命令无法启动

当前版本已支持按 `PATHEXT` 解析 `npm.cmd` / `pnpm.cmd`。如果仍有问题，先确认：

```powershell
where.exe pnpm
where.exe npm
```

### 开发服务器端口被占用

```bash
lan dev --port 3001
```

如果固定端口不可用，CLI 会在实现允许时回退到安全端口范围。
