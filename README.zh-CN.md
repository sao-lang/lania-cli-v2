<p align="center">
  <img src="./docs-site/public/logo.svg" width="96" height="96" alt="Lania CLI logo">
</p>

# Lania CLI v2

[English](./README.md) | [简体中文](./README.zh-CN.md)

`Lania CLI v2` 是一个以 Rust 为宿主、以 Node Bridge 承接 JavaScript 生态能力的双运行时 CLI。当前实现已经覆盖项目开发、多运行时模板脚手架、发布编排、Git 同步，以及项目级自定义指令扩展。

它不只是一个“内置命令集合”，也可以作为一个可扩展的 CLI 宿主框架使用。项目可以通过 `lan.config.*`、`lania.schemas.*` 和项目插件，在不修改 Rust 宿主的前提下，向同一棵命令树里挂载自己的工程化命令、产品命令或领域命令。

## 文档导航

- [文档索引](./docs/README.zh-CN.md)：整理后的总入口，按用途区分阅读顺序。
- [VitePress 学习文档站](./docs-site/index.md)：面向使用者的系统化学习文档，包含指南、命令参考、架构与实战。
- [在线文档](https://sao-lang.github.io/lania-cli-v2/)：由 GitHub Actions 自动构建并部署到 GitHub Pages。
- [学习地图](./docs/guides/learning-map.zh-CN.md)：面向首次接手仓库的开发者，解释目录结构、模块协作和关键链路。
- [命令与运行时 Roadmap](./docs/guides/command-runtime-roadmap.zh-CN.md)：按命令族梳理入口、handler、workflow / bridge / local 分支，以及关键函数追踪切口。
- [符号索引](./docs/README.zh-CN.md#guides)：补充 `create`、`release` 两条重链路的结构体、字段、函数和上下游调用索引。
- [模块设计与通信总览](./docs/architecture/模块设计与通信总览.zh-CN.md)：逐模块解释 npm wrapper、Rust host runtime、Node Bridge、插件系统、Dynamic Commands、Host RPC、workflow 与模板层之间的协作和通信。
- [框架化与产品化总览](./docs/architecture/框架化与产品化总览.zh-CN.md)：框架定位、作者 API 和产品化闭环。
- [专题设计与对账](./docs/design/专题设计与对账.zh-CN.md)：`ctx.tools`、模板边界、插件安全、类型契约与工程化增强收敛。
- [Node Bridge 协议](./docs/archive/phase0/node-bridge-protocol.md)：STDIO 通信协议与事件模型。
- [核心抽象](./docs/archive/phase0/core-abstractions.md)：Host、Command、Plugin、Workflow 等抽象定义。

## 当前能力

- 7 个命令子包 + 主包内建 config：`workspace`（dev/build/lint/create/add 的统一命名空间，template 挂 create 之下）、`product`（product dev/build/pack/publish/inspect/doctor，用法不变）、`release`、`git`、`tools`、`env`、`pi`；原 11 命令中 dev/build/lint/create/add 已合并进 workspace。`dev`、`build`、`lint`、`tools`、`env`、`pi`、`create`、`add`、`release`、`git`、`config`（模板查询是 `lan workspace template`）。
- 支持三类执行路径：
  - Bridge 型命令：`dev`、`build`、`lint`
  - 混合型命令：`tools`，其中 `tools list` 走 Node Bridge，`tools run/view` 走本地宿主能力
  - 混合型命令：`env`，探测走本地宿主能力，变更操作默认 dry-run；`env ui` 提供终端 TUI 双栏界面
  - Workflow 型命令：`create`、`add`、`release`、`git`
  - 运行时扩展命令：由项目通过 `lan.config.*` 和 `lania.schemas.*` 动态注入
- 支持项目级插件与方法白名单、来源约束、签名策略等安全机制。
- 支持 `json`、`human` 两种输出模式，以及 `spinner`、`bar`、`none` 三种进度渲染策略。
- 支持 `lan.config.js`、`lan.config.cjs`、`lan.config.json`、`lan.config.ts`。
- 支持模板元信息查询、多运行时项目脚手架生成、局部模板补充、发布状态恢复。
- 支持项目级动态命令与 Hook 绑定，允许在不修改 Rust 宿主的前提下扩展命令树。
- 支持将当前 CLI 作为项目级工程化 CLI 宿主进行二次开发：可以为团队封装自己的命令入口、交互流程、校验逻辑和宿主能力调用。

## 架构概览

仓库已按包拆分（阶段 1-9 完成），全部能力位于 `packages/`，旧顶层 `rust/`、`ts/`、`npm/` 目录不复存在。每包自带 `lania.package.json`（单一事实源，`description` 字段说明职责）：

1. 调度层 `packages/distribution/core`
   - 唯一 `lan` 入口（launcher）：按构建期静态表 `known-commands.json` 调度到已安装子包的平台二进制 `lan-<cmd>`，并注入 Bridge 载荷路径；同时提供子包管理（`lan package`）与聚合帮助。
2. 命令层 `packages/commands/<cmd>`（7 个：workspace/product/release/git/tools/env/pi）
   - 每个内置命令一个原子子包：Rust crate（spec/handler）+ npm 包 + `describes/` 快照 + 包级 `configSchema`；`config` 是总包管理面。
3. 宿主与基建 `packages/foundation/*`
   - `bridge-runtime`（stdio 协议核）+ `bridge-slices/*`（12 个 TS 切片）、`templates`（10 个模板 + EJS）、`cli-kit`（输出渲染）、`host-core`、`command`、`package-config`、`capabilities/*`（16 个 leaf crate）等。
4. 产品层 `packages/products/*`
   - `runex`（自管多运行时管理器）、`bridge-hub`（语言中立消息总线）、`agent`（pi 编码 agent）。

一句话概括：

- Rust（launcher + 子包二进制）负责命令树、执行编排和统一输出。
- Node 通过各包 payload 里的 bridge 核与切片提供配置加载、模板运行时、编译器、Lint 与动态插件执行。
- npm 包负责安装和分发体验（`npm i @lania-cli-v2/cmd-<cmd>` 独立安装单个命令）。

## 快速开始

### 使用 CLI

查看帮助：

```bash
lan --help
```

在当前项目中启动开发流程：

```bash
lan dev
```

创建新项目：

```bash
lan workspace create --name demo-app --template spa-react
```

查看模板信息（`template` 是 `create` 的二级命令）：

```bash
lan workspace template
lan workspace template toolkit
```

查看当前终端可直接运行的全局命令：

```bash
lan tools
lan tools --plain --names-only --unique
lan tools --group-by-source
```

运行代码文件或查看本地文件：

```bash
lan tools run ./scripts/demo.py
lan tools run ./scripts/demo.ts arg1 arg2
lan tools run ./scripts/demo.rs
lan tools run ./scripts/demo.go
lan tools view ./src/index.ts
lan tools view ./assets/logo.png
```

查看并管理语言环境（命令行与 GUI 两种模式）：

```bash
lan env list                     # 查看所有受支持工具链的当前版本
lan env info node                # 单个工具详情
lan env install node 20          # 默认 dry-run，只输出计划
lan env install node 20 --yes    # 确认后执行
lan env use python 3.12 --project
lan env doctor                   # 缺失工具与 PATH 冲突检查
lan env ui                       # 打开终端 TUI（键盘操作）
```

`lan env` 不自己造安装器，而是复用 nvm / fnm / volta / pyenv / uv / rustup / sdkman / scoop / winget / brew 等生态既有方案。`lan env ui` 提供双栏终端界面：左侧工具列表（`↑↓` 切换），右侧详情与操作（`i` 安装 / `u` 切换 / `d` 卸载），所有变更先显示计划再执行。详见 [语言环境管理 env](./docs-site/guide/env.md)。

规划一次发布：

```bash
lan release plan --profile package --version 1.2.3
```

规划一次产品包发布：

```bash
lan product build
lan product pack
lan product publish --dist-tag next --channel next
```

执行 `publish-manifest` 的 dry-run：

```bash
lan product publish --execute --dry-run
```

说明：

- 这一步会先对 `publish-manifest.json` 做发布前校验，而不仅仅是“本地模拟执行”。
- 当前默认校验包含 `npm whoami` 与版本冲突检查，因此即使是 `--dry-run`，也需要目标 registry 可访问，且当前环境已经完成 npm 登录。
- 如果本机尚未登录 npm，建议先执行认证配置，或直接走本地 registry 演练：`pnpm publish:verdaccio`。
- 如果你的目标只是验证“产品 CLI 是否已经成功产出发布物”，执行 `lan product publish` 即可；它会生成 tarball、wrapper、`publish-report.json` 和 `publish-manifest.json`，但不会真正执行 registry publish。

对真实 registry 执行发布：

```bash
NODE_AUTH_TOKEN=xxxxx node ./scripts/configure-npm-auth.mjs \
  --manifest ./.lania/publish/product/npm-package/publish-manifest.json \
  --output ./.lania/tmp/npmrc.publish

NPM_CONFIG_USERCONFIG=./.lania/tmp/npmrc.publish \
lan product publish --execute --yes \
  --max-retries 2 \
  --retry-delay-ms 2000
```

如果要先做本地 registry 演练：

```bash
pnpm publish:verdaccio
```

如果要走 GitHub Actions 的一体化流水线：

- `Publish Product`：串联 `lan product build`、`lan product pack`、`lan product publish`
- `Publish Manifest`：对已有 `publish-manifest.json` 做受控执行
- `Publish Verdaccio Smoke`：自动跑 Verdaccio 发布演练

可以直接参考仓库内置的产品 CLI 示例：

- `examples/product-cli-demo/README.md`
- `examples/product-cli-demo/lan.config.json`
- `examples/product-cli-demo/product/lania.schemas.ts`

在已有产品 CLI 工程（例如 `examples/product-cli-demo`）中，推荐的打包/发布验证顺序：

```bash
cd ./acme-cli

lan product build
lan product pack
lan product publish

# 如果已经完成 npm 登录，再继续：
lan product publish --execute --dry-run --yes
```

这个顺序分别对应：

- `product build`：产出 product snapshot
- `product pack`：产出 install-root 布局
- `product publish`：生成 npm 包、wrapper、bundle 和 `publish-manifest.json`
- `product publish --execute --dry-run --yes`：对 manifest 做真正的发布预检与 dry-run 执行

### 仓库开发环境

Rust workspace 与 pnpm workspace 的根都在**仓库根目录**：

```bash
pnpm install
pnpm -r build        # 构建全部 TS 包（bridge-runtime、切片、templates 等）
cargo build --workspace --bins   # 构建全部 Rust 二进制（lan-<cmd> 等）
```

本地验证 CLI（launcher 按 known-commands 调度，需要先完成上面两步构建）：

```bash
node packages/distribution/core/npm/core/bin/lan.mjs --help
node packages/distribution/core/npm/core/bin/lan.mjs create --preview --name demo-app
# 或直接运行总包二进制（仅注册 config，其余命令走子包二进制）
cargo run -p lania-cli -- config doctor
```

## 命令总览

| 命令        | 类型       | 作用                                                                                                                                                                                                                  |
| ----------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `workspace` | 命名空间   | 工作区开发全流程：`lan workspace dev/build/lint/create/add`（原 5 个顶级命令合并于此），template 挂在 create 之下（`lan workspace create template`）；`lan workspace config` 管理本包偏好                             |
| `product`   | 命名空间   | 产品化生命周期：`lan product dev/build/pack/publish/inspect/doctor`（用法与重构前一致）                                                                                                                               |
| `release`   | Workflow   | 规划、执行、恢复或查询一次发布流程                                                                                                                                                                                    |
| `git`       | Workflow   | 查看状态/日志/差异、管理分支与标签、暂存、同步（`git sync/commit/push`）                                                                                                                                              |
| `tools`     | 混合命令   | `list` 枚举环境命令；`run` 按文件类型匹配运行时执行代码；`view` 直接查看文本或调用系统查看器打开媒体文件                                                                                                              |
| `env`       | 混合 / TUI | 自管 18 种语言工具链（node/python/go/rust/deno/bun/zig/java/dart/kotlin/scala/julia/d/v/crystal/gleam/haxe/elixir），`lan env ui` 提供终端双栏界面                                                                    |
| `pi`        | Bridge     | 编码 agent（pi 引擎，交互 REPL 或一次性执行）                                                                                                                                                                         |
| `config`    | 主包内建   | 全局偏好、项目配置与跨包覆盖的统一管理（`packages` / `apply` / `--package`）。配置隔离：主包覆盖值写主包自己的 preferences（`packages.workspace` 段），永不写子包文件；子包按「主包覆盖 → 子包文件 → 默认值」取生效值 |

## 关键链路

### `dev` / `build` / `lint`

- launcher（`@lania-cli-v2/core`）按静态表调度到子包二进制（如 `lan-build`）。
- 子包内的 Rust Host 完成命令解析，并把请求转发给包内 payload 的 Node Bridge。
- Bridge 根据方法名分发到 `compiler` 或 `lint` 切片。
- `compiler` / `lint` 插件负责工具链执行。
- 运行事件和结果通过 Bridge 回传给 Rust，由 Host 统一渲染并生成最终输出。

### `tools list`

- 默认返回当前环境中“终端可直接解析”的命令集合，包含：
  - `PATH` 中的可执行文件与脚本
  - 当前 shell 的 `builtin`
  - 当前 shell 的 `alias`
  - 当前 shell 的 `function`
- 当前 shell 采集优先支持 `zsh` / `bash`；其他 shell 会自动降级为只返回 `PATH` 命令。
- 常用参数：
  - `--filter <text>`：按命令名子串过滤
  - `--all-matches`：展开 `PATH` 中同名命令的多路径命中
  - `--no-shell`：只返回 `PATH` 命令
  - `--names-only`：只返回命令名列表
  - `--group-by-source`：按 `PATH` / `builtin` / `alias` / `function` 分组
  - `--plain`：输出纯文本
  - `--unique`：按首次出现顺序去重命令名

### `tools run`

- 对输入文件优先解析 shebang；若文件头已声明运行时，将优先按 shebang 执行。
- 支持常见的 shebang 形式，包括：
  - `#!/usr/bin/env python3`
  - `#!/usr/bin/env -S node --no-warnings`
  - `#!/usr/bin/env FOO=bar python3 -u`
- JS/TS 运行时会优先查找项目本地 `node_modules/.bin`，再回退到系统 `PATH`。
- 若没有 shebang，则按扩展名匹配运行时：
  - `js` / `mjs` / `cjs`：优先 `node`，回退 `bun`
  - `ts` / `tsx` / `jsx`：优先 `tsx`，回退 `bun`、`ts-node`
  - `py`：优先 `python3`，回退 `python`
  - `sh` / `bash` / `zsh`：优先扩展名对应 shell，再回退 `bash`、`sh`
  - `rb`：`ruby`
  - `php`：`php`
  - `go`：`go run`
  - `java`：`java <file>`
  - `lua`：`lua`
  - `dart`：`dart run`
  - `nim`：`nim r`
  - `zig`：`zig run`
  - `kotlin` / `kts`：优先 `kotlin`，回退 `kotlinc -script`
  - `swift`：`swift`
- 对需要编译后执行的单文件语言，当前策略为：
  - `rs`：`rustc <file> -o <tmp-bin>` 后执行
  - `c`：`cc/clang/gcc <file> -o <tmp-bin>` 后执行
- 编译型语言会先执行 prepare 阶段；如果编译失败，会直接返回编译器错误，不再继续执行临时产物。
- 文件后的额外参数会透传给目标脚本，例如：`lan tools run ./demo.py a b`

### `tools view`

- 文本文件会直接输出内容，并带行号、字节大小与实际行范围。
- 可通过以下参数增强文本查看：
  - `--lines <N>`：限制最多输出多少行
  - `--head <N>`：查看前 N 行
  - `--start <N>`：从第 N 行开始查看
  - `--end <N>`：查看到第 N 行结束
  - `--tail <N>`：查看最后 N 行
- `--grep <text>`：按子串过滤文本行或目录项
- `--regex <pattern>`：按正则过滤文本行或目录项
- `--ignore-case`：对子串或正则搜索启用忽略大小写
- 目录路径会直接列出目录内容，并支持 `--head/--tail/--grep`
- `--tree`：以递归树形方式查看目录内容
- `--files-only`：目录查看时只保留文件项
- `--dirs-only`：目录查看时只保留目录项
- 树模式过滤命中子项时，会自动保留父目录上下文
- `--max-depth <N>`：限制目录树向下递归的层级深度
- `--sort name|size|time|ext`：按名称、大小、修改时间或扩展名排序目录项
- `--reverse`：反转目录排序结果
- `--hidden`：显示隐藏文件和隐藏目录
- `--tree` 现在使用更接近 `tree` 命令的连接线样式显示目录结构
- 图片、视频、音频、PDF 会识别媒体类型并调用系统默认查看器打开。
- 未知二进制文件默认输出十六进制预览，而不是直接拉起外部查看器。
- `--hex-bytes <N>`：限制二进制十六进制预览的字节数
- 可通过 `--meta-only` 只查看文件元信息，不真正打开外部查看器。

### `create` / `add`

- Rust 命令插件先把 CLI 参数转成工作流输入。
- `lania-workflows` 负责问题补全、任务编排、文件计划、冲突检测与写入事务。
- 需要模板目录、模板问题或渲染内容时，工作流通过 Node Bridge 访问 `template` 插件。
- 模板运行时由 `@lania-cli/templates` 提供，最终由 Rust 完成文件落盘、依赖安装、Git 初始化等宿主动作。

### 多语言运行时（`dev` / `build` / `create` / `add`）

`lan dev` / `lan build` 会先探测项目运行时，再分发到对应工具链。探测优先级：`--runtime` > `lan.config.runtime` > 特征文件 > `web` 兜底。

特征文件顺序：`go.mod` > `Cargo.toml` > `pyproject.toml` > `pubspec.yaml` > `package.json`。

支持的运行时：`web`（vite/webpack/rollup）、`go`、`rust`、`python`、`dart`。

```js
// lan.config.js
module.exports = {
  runtime: 'go',
  runtimes: {
    go: {
      dev: ['go', 'run', '.'],
      build: ['go', 'build', '-o', 'bin/app', '.'],
    },
    python: {
      // python dev 无通用默认命令，必须显式配置
      dev: ['python', '-m', 'uvicorn', 'app.main:app', '--reload'],
      build: ['python', '-m', 'build'],
    },
  },
};
```

- build 命令中的 `{output}` 会替换为 `--output-dir`（默认 `bin/app`）。
- dev 子进程会收到 `PORT` / `HOST` 环境变量；build 子进程会收到 `OUTPUT_DIR`。
- 非 web runtime 构建失败会以非零退出码返回；工具链缺失时直接失败，不伪造 dev server。
- `lan build --watch` 暂不支持非 web runtime，会明确报错。
- `lan dev` / `lan build` 的构建日志与运行期 stdout/stderr 实时流式输出，统一为 compiler 事件格式（`▶ start` / `• status` / `✖ issue` / `✔ done`）；Ctrl-C 停止时也会输出收尾日志。
- `lan create --template <name> --skip-install` 可跳过 npm/pnpm 安装；新语言模板的 `skipInstall` 默认即为 true。

内置模板（create 共 10 个，覆盖 Node.js / Python / Go / Rust / Dart 五种运行时）：

- `spa-react`（React SPA）、`spa-vue`（Vue 3 SPA）、`toolkit`（Node 工具库）、`toolkit-monorepo`（pnpm monorepo）
- `nestjs`（NestJS 服务端）、`express`（Express 服务端）
- `go-service`（Go 服务/CLI）、`rust-lib`（Rust 库）、`python-cli`（Python CLI）、`dart-cli`（Dart CLI）
- add：`go-handler`、`python-module`、`rust-module`、`dart-class`

`--package-manager` 共 11 个候选：`npm` / `pnpm` / `yarn` / `bun` / `pip` / `uv` / `poetry` / `go` / `cargo` / `dart` / `pub`。

创建项目时可以选择包管理器、lint 工具与测试工具：

| 模板                 | 包管理器                        | lint                                 | 测试           |
| -------------------- | ------------------------------- | ------------------------------------ | -------------- |
| `python-cli`         | `pip` / `uv` / `poetry`         | `ruff` / `black` / `mypy` / `flake8` | `pytest`       |
| `go-service`         | `go modules`                    | `golangci-lint` / `gofmt` / `go vet` | `go test`      |
| `rust-lib`           | `cargo`                         | `clippy` / `rustfmt`                 | `cargo test`   |
| `dart-cli`           | `dart pub`                      | `lints`                              | `package:test` |
| `nestjs` / `express` | `npm` / `pnpm` / `yarn` / `bun` | `eslint` / `prettier` / `oxlint`     | `vitest`       |

```bash
lan create --template python-cli --package-manager uv --lint-tools ruff,mypy
lan create --template go-service --lint-tools golangci-lint,go-vet
lan create --template nestjs --package-manager pnpm --lint-tools eslint,prettier
lan create --template go-service --skip-install
lan add --name Widget --template rust-module --target src
```

安装命令按运行时选择：Node 走 `npm/pnpm/yarn/bun install`；Python 走 `pip/uv/poetry`；Go 走 `go mod tidy`；Rust 走 `cargo fetch`；Dart 走 `dart pub get`。语言模板默认 `skipInstall: true`，Node 框架模板默认执行安装。

`lan lint` 已接入语言 linter：`ruff`、`black`、`mypy`、`flake8`、`golangci-lint`、`go-vet`、`clippy`、`dart-analyze`。调用前会先探测项目虚拟环境或 PATH；工具缺失时返回 fallback 结果并通过统一的 `lint_file` / `lint_result` / `lint_summary` 事件输出，不影响其它 adaptor。

```bash
lan lint --linters ruff,mypy
lan lint --linters clippy
lan lint --linters dart-analyze
```

Node 框架项目通过 `lan.config.js` 的 `buildTool` 选择构建工具：`nest` 调用本地 `@nestjs/cli`，`tsc` 调用本地 `typescript`；工具缺失时 fail-fast。生成模板时写入的 `lintTools` 也会被 `lan lint` 直接使用。Python 的生成文件优先用 `ruff format`，不可用时回退 `black`；Dart 使用 `dart format`。

### `generate api` / `generate module`（已下线归档）

> `generate` 相关命令（`generate api` / `generate module` / `lan product generate`）已下线归档，将在后续版本重新设计；完整代码保存在本地分支 `archive/generate-command`（指向删除前提交 `61d47b9`），便于恢复。相关 YAML 配置（`lania.contract.yaml` / `lania.module.yaml`）与锁文件不再由内置命令维护；历史设计可参见 [模块设计与通信总览](./docs/architecture/模块设计与通信总览.zh-CN.md) 与 [专题设计与对账](./docs/design/专题设计与对账.zh-CN.md)。

### `release` / `git`

- 统一由 Rust 工作流执行。
- 通过 `lania-config`、`lania-git`、`lania-exec`、`lania-pm`、`lania-task`、`lania-progress` 等服务完成配置解析、Git 查询、命令执行、任务编排与状态持久化。
- `release` 使用 `.lania/release-state.json` 进行状态恢复。
- `git` 覆盖只读查询（`status` / `log` / `diff` / `branch list` / `tag list`）、分支与标签管理、暂存/恢复，以及原 `lan sync` 的 `sync` / `commit` / `push` 三个动作。
- 破坏性操作（`branch delete` / `tag delete` / `restore`）默认只输出命令计划，需要 `--yes` 才真正执行。

### 自定义指令

- 项目在 `lan.config.*` 中开启 `extensions.dynamicCommands`。
- Node Bridge 扫描 `lania.schemas.*` 或 `.lania/schemas/` 下的 manifest，生成动态命令树与 handler 绑定。
- Rust Host 在启动阶段注册这些命令，并在运行时把执行请求转发回 Node Bridge。
- 动态 handler 可执行本地函数或项目插件方法；Hook 绑定则由 Rust Hook Runtime 统一调度。

### 自定义指令配置示例

最小 `lan.config.ts`：

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
};
```

更完整的 `lan.config.ts`：

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
  schemaDiscovery: {
    files: ['lania.schemas.ts', 'lania.schemas.js'],
    dirs: ['.lania/schemas'],
    allowExtensions: ['.ts', '.js', '.json', '.yaml', '.yml'],
  },
  plugins: [
    {
      package: './lania.plugin.js',
      methods: ['commands.deploy', 'hooks.audit'],
    },
  ],
  hooks: {
    onSuccess: [
      {
        plugin: './lania.plugin.js',
        handler: 'hooks.audit',
      },
    ],
  },
  ui: {
    output: { mode: 'human' },
    progress: { mode: 'spinner' },
  },
  pluginAllowlist: ['./lania.plugin.js'],
  pluginMethodAllowlist: ['commands.deploy', 'hooks.audit'],
};
```

说明：

- `extensions.dynamicCommands`：开启运行时动态命令发现与注册。
- `schemaDiscovery`：控制去哪里扫描 `lania.schemas.*` manifest。默认会扫描仓库根的 `lania.schemas.ts/js/cjs`，以及 `.lania/schemas/` 目录。
- `plugins`：声明项目插件；如果 manifest handler 指向插件方法，插件必须先在这里声明。
- `hooks`：全局 Hook 绑定，作用于内置命令和动态命令的统一生命周期。
- `pluginAllowlist` / `pluginMethodAllowlist`：限制哪些插件和方法可以被加载/调用。

### 自定义指令 Manifest 示例

最小 `lania.schemas.ts`：

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      command: { about: 'Ops tools', alias: 'o' },
      commands: [
        {
          name: 'ping',
          about: 'Ping endpoint',
          options: [{ long: 'endpoint', valueKind: 'string', help: 'Endpoint', required: true }],
          prompt: [
            {
              field: 'endpoint',
              message: 'Endpoint?',
              kind: 'input',
              whenMissing: ['endpoint'],
            },
          ],
          hooks: {
            preRun: [
              async () => ({
                events: [
                  { method: 'event.log', params: { level: 'info', message: 'preRun hook' } },
                ],
              }),
            ],
          },
          handler: async (ctx) => ({
            result: { ok: true, input: ctx.argv.options, exitCode: 0 },
            events: [],
          }),
        },
      ],
    },
  ],
};
```

完整流程版 `lania.schemas.ts`（含 `when/goto/onAnswered`）：

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      command: { about: 'Ops tools', alias: 'o' },
      commands: [
        {
          name: 'deploy',
          about: 'Deploy service',
          args: [{ name: 'service', required: true, multiple: false, help: 'service name' }],
          options: [
            { long: 'project', valueKind: 'string', help: 'project key' },
            { long: 'mode', valueKind: 'string', help: 'deploy mode', required: true },
            { long: 'region', valueKind: 'string', help: 'deploy region' },
            { long: 'confirm', valueKind: 'bool', help: 'need confirm' },
          ],
          prompt: [
            {
              id: 'mode',
              field: 'mode',
              message: { zh: '请选择部署模式', en: 'Select deploy mode' },
              kind: 'select',
              choices: [
                { label: 'simple', value: 'simple' },
                { label: 'advanced', value: 'advanced' },
              ],
              when: { type: 'truthy', key: 'project' },
              goto: 'confirm',
              validate: ['required', { type: 'one_of', values: ['simple', 'advanced'] }],
              mapFunctions: ['trim', { type: 'lowercase' }],
              onAnswered: [
                {
                  type: 'set_context_from_answer',
                  key: 'isAdvanced',
                  mapFunctions: [{ type: 'split', separator: ',' }],
                },
                {
                  type: 'goto_if',
                  when: { type: 'equals', key: 'mode', value: 'advanced' },
                  target: 'region',
                },
              ],
            },
            {
              id: 'region',
              field: 'region',
              message: { zh: '请输入 region', en: 'Input region' },
              kind: 'input',
              when: { type: 'equals', key: 'mode', value: 'advanced' },
              validate: [{ type: 'min_length', min: 2 }],
              timeoutMs: 10000,
              contextKey: 'deployRegion',
            },
            {
              id: 'confirm',
              field: 'confirm',
              message: { zh: '确认执行部署？', en: 'Confirm deploy?' },
              kind: 'confirm',
              defaultValue: false,
              returnable: true,
            },
          ],
          hooks: {
            onArgsParsed: [
              {
                type: 'plugin',
                kind: 'waterfall',
                plugin: './lania.plugin.js',
                handler: 'commands.normalizeDeployArgs',
              },
            ],
          },
          handler: async (ctx) => ({
            result: {
              ok: true,
              traceId: ctx.traceId,
              mount: ctx.mount,
              path: ctx.path,
              service: ctx.argv.args.service,
              options: ctx.argv.options,
              exitCode: 0,
            },
            events: [],
          }),
        },
      ],
    },
  ],
};
```

如果要调用项目插件方法而不是内联函数：

```ts
export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'deploy',
          about: 'Deploy project',
          handler: {
            plugin: './lania.plugin.js',
            method: 'commands.deploy',
          },
        },
      ],
    },
  ],
};
```

说明：

- `mount` 会成为顶级命令，例如上例生成 `lan ops ping`。
- `commands` 支持多级 `subcommands`。
- `handler` 可以是内联函数，也可以是 `{ plugin, method }`。
- `prompt` 用于缺参补问；`required: true` 会让 Rust Host 在执行前校验必填选项。
- `prompt` 现在也支持 `lania-prompt` 的流程字段子集：
  - `when`
  - `goto`
  - `validate`
  - `timeoutMs`
  - `contextKey`
  - `accumulation`
  - `returnable`
  - `mapFunctions`
  - `onAnswered`
- `prompt.whenMissing` 仍然可用，但它只是“缺参才问”的兼容写法；复杂分支建议直接用 `when/goto/onAnswered`。
- `hooks.preRun` / `hooks.postRun` 会分别映射到 `onCommandPreInit` / `onSuccess`。
- 顶级保留命令如 `dev`、`build`、`release` 不能被 `mount` 覆盖。

## 用当前 CLI 开发项目自己的 CLI

如果你想做的不只是“给现有命令加一点配置”，而是希望基于当前 CLI 开发团队自己的工程化 CLI，这个仓库已经具备完整路径：

- 用 `lan.config.*` 开启扩展能力、限制插件来源和宿主权限
- 用 `lania.schemas.*` 描述命令树、参数、交互流程和 Hook
- 用 inline handler 或项目插件实现真正的业务逻辑
- 在 handler 里通过 `ctx.tools` 调用 `git`、`fs`、`exec`、`pm`、`workspace`、`log` 等宿主能力
- 继续复用现有 CLI 的统一输出、进度、日志、审计和策略边界

这意味着你可以把 `lan` 看作“团队 CLI 的运行时底座”，而不是只能使用内置的 `dev/build/lint/create`。

### 一个最小 demo

下面这个 demo 会给当前项目挂载一个 `lan team hello` 命令：

- 读取当前工作区的 `package.json`
- 获取当前 Git 分支
- 在 `.lania/reports/team-cli.json` 中写一份报告
- 返回结构化结果，并沿用现有 CLI 的输出模式

`lan.config.ts`：

```ts
export default {
  extensions: {
    dynamicCommands: true,
  },
  schemaDiscovery: {
    files: ['lania.schemas.ts'],
  },
  tools: {
    allow: ['workspace', 'git', 'fs', 'path', 'log', 'result'],
    fs: {
      writeRoot: '.',
    },
  },
};
```

`lania.schemas.ts`：

```ts
export default {
  runtimeCommands: [
    {
      mount: 'team',
      command: {
        about: 'Team engineering commands',
      },
      commands: [
        {
          name: 'hello',
          about: 'Generate a simple team CLI report',
          options: [
            {
              long: 'name',
              valueKind: 'string',
              help: 'Your name',
            },
          ],
          prompt: [
            {
              field: 'name',
              message: 'Your name?',
              kind: 'input',
              whenMissing: ['name'],
            },
          ],
          handler: async (ctx) => {
            const name = ctx.argv.options.name ?? 'anonymous';
            const reportPath = ctx.tools.path.resolve(
              ctx.cwd,
              '.lania',
              'reports',
              'team-cli.json',
            );
            const pkg = await ctx.tools.workspace.packageJson();
            const branch = await ctx.tools.git.branch.current();

            await ctx.tools.fs.ensureDir(ctx.tools.path.dirname(reportPath));
            await ctx.tools.fs.writeJson(reportPath, {
              name,
              packageName: pkg?.name ?? null,
              branch,
            });

            await ctx.tools.log.success(`team report written: ${reportPath}`);

            return ctx.tools.result.ok({
              greeting: `hello, ${name}`,
              packageName: pkg?.name ?? null,
              branch,
              reportPath,
            });
          },
        },
      ],
    },
  ],
};
```

运行方式：

```bash
lan team hello --name alice
```

或者直接走交互：

```bash
lan team hello
```

执行完成后，你会得到两类结果：

- 终端里看到统一渲染的命令结果
- 工作区里生成 `.lania/reports/team-cli.json`

当你想让业务逻辑更复杂时，再把 inline `handler` 提取到项目插件文件中即可。

### 从零到可用的完整流程

如果你的目标是“为团队开发一个真正可落地的项目 CLI”，推荐按下面顺序推进：

1. 明确命令边界
   - 先定义你要做的是“项目命令”还是“通用工具命令”。
   - 例如：`lan app deploy`、`lan ops inspect`、`lan team bootstrap`。

2. 打开扩展能力
   - 在 `lan.config.ts` 里开启 `extensions.dynamicCommands: true`。
   - 同时配置 `schemaDiscovery`，明确从哪些文件或目录发现命令清单。

3. 先写命令树，不急着写实现
   - 在 `lania.schemas.ts` 里先写出 `mount`、子命令、参数、选项、`prompt`、Hook。
   - 先把命令的使用方式和交互体验定下来，再补真正逻辑，后续维护会轻松很多。

4. 用 inline handler 快速验证
   - 在 manifest 里先直接写 `handler: async (ctx) => { ... }`。
   - 适合验证命令模型、参数补问、结果结构和 `ctx.tools` 是否够用。

5. 复杂后再迁移到项目插件
   - 当逻辑开始变长，或需要多人协作维护时，把实现迁移到 `lania.plugin.ts/js`。
   - 然后在 `lan.config.ts` 的 `plugins` 中声明插件，在 `handler` 里改成 `{ plugin, method }` 形式。

6. 给命令补上 Hook 和治理策略
   - 用 `onArgsParsed`、`preRun`、`postRun` 等 Hook 处理补参、校验、审计和收尾动作。
   - 用 `pluginAllowlist`、`pluginMethodAllowlist`、`tools.allow` 等策略限制能力边界。

7. 复用宿主能力，而不是重复造轮子
   - 文件写入优先走 `ctx.tools.fs`
   - 命令执行优先走 `ctx.tools.exec`
   - Git 查询优先走 `ctx.tools.git`
   - 工作区信息优先走 `ctx.tools.workspace`
   - 这样可以持续复用当前 CLI 的统一治理和输出能力。

8. 固化为团队 CLI 约定
   - 把 `lan.config.ts`、`lania.schemas.ts`、`lania.plugin.ts` 放进项目模板或基建仓库。
   - 让业务仓库只关心“声明自己的命令和逻辑”，而不是反复搭一套新的 CLI 外壳。

### 从 demo 到生产化的演进方式

一个比较自然的演进路径是：

1. 用 inline `handler` 跑通一个命令
2. 把共享逻辑收敛进 `lania.plugin.ts`
3. 把命令定义沉淀进 `lania.schemas.ts`
4. 把默认策略和 UI 体验沉淀进 `lan.config.ts`
5. 最后把这三类文件放进模板，让新项目一键拥有团队 CLI

如果你想看一个更完整、偏运行时能力演示的示例，可以直接参考仓库内置的：

- `examples/schema-tools-demo/lan.config.ts`
- `examples/schema-tools-demo/lania.schemas.ts`
- `examples/schema-tools-demo/README.md`
- `examples/product-cli-demo/README.md`
- `examples/product-cli-demo/lan.config.json`
- `examples/product-cli-demo/product/lania.schemas.ts`

### 动态命令执行时序（交互 -> 参数 -> handler）

执行顺序（简化）：

1. 解析 CLI 参数（得到 `argv.args` / `argv.options`）
2. `onArgsParsed`（可改写参数）
3. 执行 `prompt`（支持 `when/goto/onAnswered`；已有参数会注入 prompt context）
4. 合并交互答案到 `argv.options`
5. 调用真正的 `handler(ctx)`
6. `onSuccess` / `onError`

`handler` 入参 `ctx` 关键字段：

- `cwd`
- `mount`
- `path`
- `traceId`
- `argv.args`
- `argv.options`

其中 `argv.*` 是最终值：已包含参数解析、Hook 改写和 prompt 结果。

### `ctx.tools` 与示例仓库

动态命令 handler 和 inline hook 现在都会注入统一的 `ctx.tools` 运行时对象。常用能力包括：

- 本地能力：`bridge`、`config`、`text`、`path`、`workspace`、`env`、`json`、`result`
- Host-backed 能力：`host`、`git`、`pm`、`exec`、`fs`、`log`、`tasks`、`progress`、`interaction`

可以直接参考仓库内置示例：

- `examples/schema-tools-demo/lan.config.ts`
- `examples/schema-tools-demo/lania.schemas.ts`
- `examples/schema-tools-demo/README.md`

这个示例仓库演示了两条典型链路：

1. inline `onArgsParsed` hook 使用 `ctx.tools.text` 和 `ctx.tools.bridge.call('bridge.ping')`
2. handler 使用 `ctx.tools.workspace`、`ctx.tools.pm`、`ctx.tools.git`、`ctx.tools.exec`、`ctx.tools.fs`、`ctx.tools.log` 与 `ctx.tools.result`

最小示例：

```ts
handler: async (ctx) => {
  const branch = await ctx.tools.git.branch.current();
  const manager = await ctx.tools.pm.detect();
  const out = ctx.tools.path.resolve(ctx.cwd, '.lania', 'report.json');

  await ctx.tools.fs.writeJson(out, { branch, manager });
  await ctx.tools.log.success(`report written: ${out}`);

  return ctx.tools.result.ok({ branch, manager, out });
};
```

## 配置文件

### `lan.config.*`

主配置文件由 Node Bridge 负责加载，当前支持：

- `lan.config.js`
- `lan.config.cjs`
- `lan.config.json`
- `lan.config.ts`

当前运行时重点消费以下字段：

| 字段                                           | 作用                         |
| ---------------------------------------------- | ---------------------------- |
| `buildTool` / `buildAdaptors`                  | 构建工具选择与适配器配置     |
| `lintTools` / `lintAdaptors`                   | Lint 工具与适配器配置        |
| `plugins`                                      | 项目级插件声明               |
| `release`                                      | 发布默认配置                 |
| `extensions.dynamicCommands`                   | 是否启用运行时动态命令       |
| `schemaDiscovery`                              | 动态命令 manifest 的发现策略 |
| `hooks`                                        | 全局 Hook 绑定               |
| `ui.output`                                    | 输出模式（`json` / `human`） |
| `ui.progress`                                  | 进度展示模式                 |
| `ui.interaction`                               | 交互模式、超时和默认值策略   |
| `pluginAllowlist` / `pluginMethodAllowlist` 等 | 插件安全边界                 |

动态命令相关的默认发现规则：

- `schemaDiscovery.files`
  - `lania.schemas.ts`
  - `lania.schemas.js`
  - `lania.schemas.cjs`
- `schemaDiscovery.dirs`
  - `.lania/schemas`
- `schemaDiscovery.allowExtensions`
  - `.ts`
  - `.js`
  - `.cjs`
  - `.json`
  - `.yaml`
  - `.yml`

### 三层配置统一管理（config 统一管理面）

配置分三层：项目级 `lan.config.*`、全局偏好 `~/.lania/preferences.json`、包级偏好 `~/.lania/config/<pkg>.json`。`lan config` 是统一管理面（`config` 自身不算命令子包），`generate` 归档后共 11 个命令。

10 个命令子包（`add` / `build` / `create` / `dev` / `env` / `git` / `lint` / `pi` / `release` / `tools`）全部带 `config` 二级命令：`bare` / `get` / `set` / `init` / `doctor` / `sync`，中英双语文案齐全，`get` / `set` 支持 `--global` 切到全局作用域。包级键由各包 `lania.package.json` 的 `configSchema` 声明：通用 `locale`（`zh` / `en`）、`output.mode`（`json` / `human`）、`log.timestamps`（bool），`env` 额外 `shim.host`（`lan` / `runex`）。共用实现收敛在 `packages/foundation/package-config/rust/lania-command-config`，不再每包写一遍。

```bash
# 读写单个子包（--package 支持短名 / npm 全名 / bin 名 / crate 名，统一归一化）
lan config get locale --package env
lan config set locale zh --package @lania-cli-v2/cmd-env
lan config set shim.host runex --package lan-env

# 列出所有已安装子包声明的键、默认值与当前值
lan config packages --json

# 把全局偏好一键下发到所有已安装且启用的子包
lan config apply
lan config apply --package env --set locale=zh --set output.mode=human
```

`--package` 统一归一化后按目标包 `configSchema` 校验写入；`apply` 只覆盖子包已声明的键（点分键与 camelCase 全局键有映射）。静态表 `known-commands.json` 现携带每个子包的 `configSchema`，launcher 运行时不再解析 npm manifest；独立二进制 `lan-config packages` / `lan-config apply`（扫描 `~/.lania/config/*.json`）也可用，但主路径是 `lan`。详见 [配置文件指南](./docs-site/guide/configuration.md) 与 [配置字段参考](./docs-site/reference/config.md)。

### 状态文件

| 文件                        | 用途             |
| --------------------------- | ---------------- |
| `.lania/release-state.json` | 发布流程状态快照 |

## 输出与交互

- `stdout` 以结构化结果为核心，适合自动化和二次消费。
- `stderr` 承担友好错误提示与人类可读日志输出。
- 可通过 `lan config set output.mode <json|human>` 设置默认输出风格；旧的 `stream` / `jsonl` 会归一为 `json`。
- 交互链路由 Rust PromptService 统一控制；非交互环境可通过默认值策略或脚本化答案运行。

## 安装与分发

最终安装形态要求子包二进制和 Node Bridge 载荷共同存在。典型布局如下：

```text
<install-root>/
  bin/
    lan                    # launcher（@lania-cli-v2/core）
    lan-<cmd>              # 各命令子包二进制
  lib/
    bridge/                # 「核 + 切片」payload（阶段 7a 起的统一布局）
      package.json
      dist/
      node_modules/        # bridge-runtime + bridge-slice-* + templates + agent
```

Bridge 目录查找顺序如下：

1. `LANIA_NODE_BRIDGE_DIR`
2. `<current-exe>/../lib/bridge`（阶段 7a 后 payload 统一组装为「核 + 切片」）
3. `<current-exe>/../lib/node-bridge`（旧布局，兼容已安装的旧版本）
4. `<current-exe>/../node-bridge`
5. 开发态工作区 `packages/foundation/bridge-runtime/ts`

## 仓库结构（按包拆分）

全部能力按 `packages/` 布局拆分，每包自带 `lania.package.json`（单一事实源，
`description` 字段说明该包职责）、`rust/`（Rust crate）、`npm/`（可安装的 npm 包）、
`describes/`（命令规格快照）。各包职责一览：

| 包                                                       | 类别            | 职责                                                                                                                      |
| -------------------------------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `packages/commands/{workspace,release,git,tools,env,pi}` | command         | 6 个命令子包：workspace 合并原 dev/build/lint/create/add（含 product 子树与 config）；config 为主包内建，由总包二进制承载 |
| `packages/distribution/cli`                              | distribution    | 总包：聚合全部命令子包与全量 bridge 切片（不再静态链接命令）                                                              |
| `packages/distribution/core`                             | distribution    | 核心调度器：唯一 `lan` 入口，按 `known-commands.json` 静态表调度子包二进制                                                |
| `packages/foundation/cli-kit`                            | foundation      | 输出渲染与单命令执行支点（envelope/本地化/人类可读渲染 + 模板文案目录）                                                   |
| `packages/foundation/host-core`                          | foundation      | 宿主层共享接缝（`BridgeProvider` 抽象、product root 等内建定义）                                                          |
| `packages/foundation/package-config`                     | foundation      | 包级偏好引擎 `lania-package-config` 与共享 config 二级命令实现 `lania-command-config`                                     |
| `packages/foundation/runex-home`                         | foundation      | runex 自管运行时的极小 PATH 前置（`RunexHome` / `ShimHost`）                                                              |
| `packages/foundation/bridge-runtime`                     | foundation      | Bridge 运行时核：stdio 协议、插件注册表与切片动态装配                                                                     |
| `packages/foundation/bridge-slices/*`                    | foundation      | 12 个 TS bridge 切片（compiler/lint/template/product/toolchain/…）                                                        |
| `packages/foundation/templates`                          | foundation      | 脚手架资产：10 个内置项目模板、add 模板与 EJS 渲染运行时                                                                  |
| `packages/foundation/lania`                              | foundation      | 作者侧 DSL：`lan.config.*` 类型化配置与 schema 工具                                                                       |
| `packages/foundation/command`                            | foundation      | 命令规格/解析/本地化基建（`lania-command`）                                                                               |
| `packages/foundation/capabilities/*`                     | foundation      | 16 个宿主能力 leaf crate（git/exec/fs/logger/workflows/…）                                                                |
| `packages/products/agent`                                | product         | 编码 agent：pi 引擎（Rust lib API + npm TS 引擎双形态）                                                                   |
| `packages/products/bridge-hub`                           | product         | 语言中立消息总线：BusFrame 协议、进程内 Hub 与 5 语言端点 adaptor                                                         |
| `packages/products/runex`                                | product         | 自管多运行时管理器（独立 CLI 双形态，`runex` bin）                                                                        |
| `packages/runtime`                                       | command-runtime | runtime 设施：项目级动态命令与脚手架封装命令的执行宿主（`lan-runtime`）                                                   |
| `packages/schemas`                                       | —               | `lania.package.json` 的 JSON Schema                                                                                       |

更详细的目录说明见 [目录结构参考](./docs/references/ai/directory-structure.md)。

## 推荐阅读顺序

- 先看 [学习地图](./docs/guides/learning-map.zh-CN.md)，建立整体心智模型。
- 再看 [模块设计与通信总览](./docs/architecture/模块设计与通信总览.zh-CN.md)，理解执行边界、模块职责与通信模型。
- 如果关心框架化和产品化方向，再看 [框架化与产品化总览](./docs/architecture/框架化与产品化总览.zh-CN.md)。
- 如果要研究 Bridge 协议，再看 [Node Bridge 协议](./docs/archive/phase0/node-bridge-protocol.md)。

## 当前建议的验证命令

```bash
# Rust workspace 根在仓库根（Cargo.toml 在根目录）
cargo test --workspace
cargo check --workspace --all-targets
pnpm known:check && pnpm packages:check && pnpm bridge:check
node scripts/smoke-core-dispatch.mjs
```
