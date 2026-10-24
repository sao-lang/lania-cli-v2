# CLI 命令参考

<div class="lead">
本页按命令族列出 Lania CLI 的语法、常用选项与示例。所有命令都支持 `--help`。
</div>

## 全局

```bash
lan --help
lan help
lan --version
```

全局输出与语言由 `lan config` 控制。

## dev

启动开发服务器。

```bash
lan dev [options]
```

| 选项                | 说明              |
| ------------------- | ----------------- |
| `-p, --port <port>` | 端口，默认 `8089` |
| `-H, --host <host>` | 主机              |
| `--hmr`             | 开启/关闭 HMR     |
| `-o, --open`        | 就绪后打开浏览器  |
| `-m, --mode <mode>` | dev 模式          |
| `--config <path>`   | 兼容旧配置参数    |
| `--path <path>`     | 兼容旧项目路径    |

## build

```bash
lan build [options]
```

| 选项                     | 说明                         |
| ------------------------ | ---------------------------- |
| `-w, --watch`            | watch 构建                   |
| `-m, --mode <mode>`      | `development` / `production` |
| `-o, --output-dir <dir>` | 输出目录                     |
| `--config <path>`        | 兼容参数                     |
| `--path <path>`          | 兼容参数                     |

## lint

```bash
lan lint [check|fix] [options]
```

| 选项                    | 说明                                              |
| ----------------------- | ------------------------------------------------- |
| `-f, --fix`             | 修复模式                                          |
| `--linters <list>`      | `oxlint,eslint,oxfmt,prettier,stylelint,textlint` |
| `-c, --concurrency <n>` | 并发，默认 4                                      |
| `--grouped-output`      | 按适配器分组输出                                  |

## tools

### tools list

```bash
lan tools [list] [options]
```

| 选项                    | 说明                              |
| ----------------------- | --------------------------------- |
| `-f, --filter <text>`   | 名称过滤                          |
| `-n, --limit <n>`       | 数量限制                          |
| `--shell`               | 包含 shell builtin/alias/function |
| `-a, --all-matches`     | 展开同名多路径                    |
| `--names-only`          | 只返回名称                        |
| `-g, --group-by-source` | 分组                              |
| `-p, --plain`           | 纯文本                            |
| `-u, --unique`          | 去重                              |

### tools run

```bash
lan tools run <file> [args...]
```

按 shebang 或扩展名选择运行时。

### tools view

```bash
lan tools view <path> [options]
```

支持文本、目录、媒体与二进制预览。详见 [工具命令 tools](/guide/tools)。

## env

管理多语言工具链版本，默认 dry-run，`--yes` 才真正执行。详见 [语言环境管理 env](/guide/env)。

| 子命令                                     | 说明                                      |
| ------------------------------------------ | ----------------------------------------- |
| `lan env list [tool]`                      | 列出工具链及当前版本                      |
| `lan env info <tool>`                      | 单个工具详情与已装版本                    |
| `lan env versions <tool>`                  | 已安装版本列表                            |
| `lan env install <tool> <version>`         | 安装                                      |
| `lan env use <tool> <version> [--project]` | 切换 / 固定版本                           |
| `lan env remove <tool> <version>`          | 卸载                                      |
| `lan env doctor`                           | 缺失工具与 PATH 冲突检查                  |
| `lan env ui`                               | 打开终端 TUI（`--snapshot` 输出单帧文本） |

通用选项：

| 选项                 | 说明                       |
| -------------------- | -------------------------- |
| `-m, --manager <id>` | 指定版本管理器             |
| `-y, --yes`          | 实际执行（否则只输出计划） |
| `-p, --project`      | 将版本写入项目版本文件     |

`lan env ui` 的选项：

| 选项         | 说明                                        |
| ------------ | ------------------------------------------- |
| `--snapshot` | 不进入交互，直接输出一帧纯文本（CI / 调试） |
| `--cols <n>` | 快照宽度，默认 100                          |
| `--rows <n>` | 快照高度，默认 28                           |

支持的 `tool`：`node` `python` `rust` `go` `dart` `kotlin` `java` `c`。

## create

```bash
lan create [path] [options]
```

| 选项                         | 说明                                                     |
| ---------------------------- | -------------------------------------------------------- |
| `-n, --name <name>`          | 项目名                                                   |
| `-t, --template <template>`  | `spa-react` / `spa-vue` / `toolkit` / `toolkit-monorepo` |
| `-p, --package-manager <pm>` | npm/pnpm/yarn/bun                                        |
| `--directory <dir>`          | 旧版路径                                                 |
| `-g, --git`                  | 初始化 Git                                               |
| `--skip-git`                 | 跳过 Git                                                 |
| `--skip-install`             | 跳过安装                                                 |
| `--language <lang>`          | 模板语言                                                 |
| `-d, --dry-run`              | 只规划                                                   |
| `-P, --preview`              | 预览并隐含 dry-run                                       |

## add

```bash
lan add [options]
```

| 选项                        | 说明                                                                                                                                              |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-n, --name <name>`         | 生成物名称                                                                                                                                        |
| `-t, --template <template>` | `v2` / `v3` / `rcc` / `rfc` / `svelte` / `astro` / `prettier` / `eslint` / `stylelint` / `editorconfig` / `gitignore` / `tsconfig` / `commitizen` |
| `-d, --target <target>`     | 目标目录                                                                                                                                          |
| `--filepath <path>`         | 旧版目标路径                                                                                                                                      |
| `-f, --force`               | 覆盖冲突                                                                                                                                          |

## template

```bash
lan template
lan template <name>
```

## generate api

```bash
lan generate api [plan|diff|init] [options]
```

| 选项                    | 说明                       |
| ----------------------- | -------------------------- |
| `-c, --config <path>`   | 默认 `lania.contract.yaml` |
| `-s, --source <source>` | 源过滤                     |
| `-t, --target <target>` | 目标过滤                   |
| `-e, --entry <entry>`   | 入口过滤                   |
| `-m, --manifest <path>` | manifest                   |
| `-d, --dry-run`         | 只规划                     |
| `-k, --check`           | 只校验                     |
| `--clean`               | 清理 stale                 |
| `-f, --force`           | 覆盖冲突                   |

## generate module

```bash
lan generate module [plan|diff|init|apply] [options]
```

| 选项                    | 说明                     |
| ----------------------- | ------------------------ |
| `-c, --config <path>`   | 默认 `lania.module.yaml` |
| `-i, --input <path>`    | 限定输入                 |
| `-s, --source <source>` | 源过滤                   |
| `-t, --target <target>` | 目标过滤                 |
| `--framework <name>`    | 后端框架                 |
| `-e, --entry <entry>`   | 入口过滤                 |
| `--main <path>`         | main.go 注入目标         |
| `--module-name <name>`  | 模块名覆盖               |
| `--package <path>`      | Go import path           |
| `-m, --manifest <path>` | manifest                 |
| `-d, --dry-run`         | 只规划                   |
| `-k, --check`           | 只校验                   |
| `--clean`               | 清理 stale               |
| `-f, --force`           | 覆盖冲突                 |
| `--no-inject`           | 跳过 main.go 注入        |

## release

```bash
lan release [options] [plan|run|resume|status]
```

| 选项                      | 说明                           |
| ------------------------- | ------------------------------ |
| `-v, --version <version>` | 版本                           |
| `-t, --tag <tag>`         | dist-tag                       |
| `-P, --profile <profile>` | package/web-app/service/custom |
| `-e, --env <env>`         | 环境                           |
| `-C, --channel <channel>` | 通道                           |
| `-f, --from <stage>`      | 起始阶段                       |
| `-T, --to <stage>`        | 结束阶段                       |
| `-s, --skip <stages>`     | 跳过阶段                       |
| `-S, --state-file <path>` | 状态文件                       |
| `-p, --publish`           | 启用发布阶段                   |
| `-c, --changelog`         | 启用 changelog                 |
| `--skip-git`              | 跳过 Git 收尾                  |
| `-a, --apply`             | 真正执行                       |
| `-d, --dry-run`           | 强制规划                       |
| `-y, --yes`               | 免确认                         |

## git

```bash
lan git status                 # 工作区状态
lan git log [-n 10]            # 最近提交
lan git diff [--staged] [-P path]
lan git branch list            # 本地 + 远程分支
lan git branch create <name>
lan git branch switch <name>
lan git branch delete <name> [--force] [--yes]
lan git tag list
lan git tag create <name> [-m message]
lan git tag delete <name> [--yes]
lan git add [--dry-run]
lan git restore [paths...] [--staged] [--yes]
lan git sync|commit|push [options]
```

`sync` / `commit` / `push` 的选项：

| 选项                      | 说明              |
| ------------------------- | ----------------- |
| `-r, --remote <remote>`   | 远端，默认 origin |
| `-b, --branch <branch>`   | 分支              |
| `-m, --message <message>` | 提交信息          |
| `-a, --amend`             | amend             |
| `-f, --force-with-lease`  | 强制推送保护      |
| `-d, --dry-run`           | 只规划            |
| `-i, --interactive`       | commitizen        |
| `-p, --push`              | 提交后推送        |

> 破坏性操作（`branch delete` / `tag delete` / `restore`）不带 `--yes` 时只输出命令计划，不会真正执行。

## config

```bash
lan config
lan config get <key>
lan config set <key> <value>
```

支持的 key：

- `locale`
- `log.timestamps`
- `output.mode`

## product

```bash
lan product generate
lan product dev <command>
lan product build
lan product pack
lan product publish
lan product inspect
lan product doctor
```

### product generate

| 选项                        | 说明                            |
| --------------------------- | ------------------------------- |
| `--preset <preset>`         | `demo` / `minimal`，默认 `demo` |
| `-i, --interactive`         | 交互补问                        |
| `-n, --name <name>`         | 展示名                          |
| `-b, --binary-name <name>`  | 二进制名                        |
| `-p, --package-name <name>` | npm 包名                        |
| `-o, --output-dir <dir>`    | 输出目录                        |
| `-f, --force`               | 覆盖已有目录                    |

### product dev

| 选项                     | 说明                       |
| ------------------------ | -------------------------- |
| `--path <path>`          | 产品根目录                 |
| `-w, --watch`            | 监听产品文件并重启转发命令 |
| `--poll-interval-ms <n>` | watch 轮询间隔，默认 500   |

### product build

| 选项                     | 说明                        |
| ------------------------ | --------------------------- |
| `--path <path>`          | 产品根目录                  |
| `-o, --output-dir <dir>` | 默认 `.lania/build/product` |
| `--clean`                | 写入前清理输出目录          |

### product pack

| 选项                     | 说明                                    |
| ------------------------ | --------------------------------------- |
| `--path <path>`          | 产品根目录                              |
| `--build-dir <dir>`      | 默认 `.lania/build/product`             |
| `-o, --output-dir <dir>` | 默认 `.lania/pack/product/install-root` |
| `--clean`                | 写入前清理输出目录                      |

### product publish

| 选项                             | 说明                                      |
| -------------------------------- | ----------------------------------------- |
| `--path <path>`                  | 产品根目录                                |
| `--pack-dir <dir>`               | 默认 `.lania/pack/product/install-root`   |
| `-o, --output-dir <dir>`         | 默认 `.lania/publish/product/npm-package` |
| `-t, --dist-tag <tag>`           | npm dist-tag                              |
| `-C, --channel <channel>`        | 发布通道                                  |
| `--registry <url>`               | npm registry                              |
| `--platform-binaries-dir <dir>`  | 平台二进制目录                            |
| `--platform-binary-paths <json>` | 平台到二进制路径的 JSON 映射              |
| `--execute`                      | 真正执行 npm publish                      |
| `--dry-run`                      | 用 `npm --dry-run` 执行                   |
| `-y, --yes`                      | 确认真实发布                              |
| `--resume`                       | 从已完成步骤继续                          |
| `--otp <code>`                   | npm OTP                                   |
| `--npm-bin <path>`               | 自定义 npm 可执行文件                     |
| `--max-retries <n>`              | 瞬时失败重试次数                          |
| `--rollback-on-failure`          | 部分失败后执行 unpublish                  |
| `--clean`                        | 写入前清理输出目录                        |

### product inspect / doctor

| 命令              | 选项                 | 说明                                                 |
| ----------------- | -------------------- | ---------------------------------------------------- |
| `product inspect` | `--path`, `--compat` | 检查配置、schema 与产物；`--compat` 额外写兼容性报告 |
| `product doctor`  | `--path`             | 兼容性、产物与 schema 诊断                           |

详见 [产品 CLI](/guide/product-cli)。

## 退出码

| 退出码 | 含义                   |
| ------ | ---------------------- |
| `0`    | 成功                   |
| `1`    | 通用失败 / lint 失败   |
| `2`    | 参数、配置或运行时错误 |
| `124`  | 超时                   |
| `130`  | 取消                   |

具体命令可能返回工具链自身的退出码。

## 下一步

- [配置字段参考](/reference/config)
- [模板参考](/reference/templates)
- [Schema 与协议](/reference/schemas)
