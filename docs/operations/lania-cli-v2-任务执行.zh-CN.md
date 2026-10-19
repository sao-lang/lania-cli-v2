# Lania CLI v2 功能验收与 VitePress 文档站任务执行文档

> 文档用途：记录当前验收进度、已发现问题、修复步骤、验收标准与后续执行入口。
> 后续每轮对话开始前，先阅读本文档的「当前进度」和「下一轮开始位置」。
>
> 最后更新：2026-10-19

## 1. 总目标

1. 验证 `lania-cli-v2` 当前已实现的功能与命令能否在 Windows 环境完整跑通。
2. 修复验收过程中发现的阻塞性缺陷，补齐跨平台兼容性。
3. 在功能验收通过后，使用 VitePress 生成一套类似 Vue 3 官方文档的学习文档站。
4. 文档站需要具备：顶部导航、分组侧边栏、本地搜索、深色模式、大纲、首页 Hero、功能卡片、响应式布局、可构建发布。
5. 最终交付物必须可验证：命令可运行、测试可通过、文档站可 `build`。

## 2. 当前结论

主体功能已完成修复与全量回归，当前进入 VitePress 文档站建设阶段。

已修复：

- Windows 下 `lan create` 无法执行包管理器版本查询。
- Windows 下 `lan tools run` 无法正确解析运行时（如 `python`、`node`）。
- `lan add --template v3` 因 EJS 模板语法错误而失败。
- Rust 测试因 Windows 路径分隔符断言失败。
- Node Bridge 测试因 Windows 平台包矩阵预期不一致失败。

当前验证结果：

| 验证项                                                    | 结果    |
| --------------------------------------------------------- | ------- |
| `cargo build --manifest-path rust/Cargo.toml --workspace` | ✅ 通过 |
| `cargo test --manifest-path rust/Cargo.toml --workspace`  | ✅ 通过 |
| `pnpm -C ts typecheck`                                    | ✅ 通过 |
| `pnpm -C ts test`                                         | ✅ 通过 |
| `lan create --preview`                                    | ✅ 通过 |
| `lan tools run *.py` / `*.js`                             | ✅ 通过 |
| `lan add --template v3`                                   | ✅ 通过 |
| `lan product build/pack/publish`                          | ✅ 通过 |
| `pnpm docs:build`                                         | ✅ 通过 |

因此本轮执行顺序为：**先修缺陷 → 再全量回归 → 最后生成 VitePress 文档站**。三步均已完成。

## 3. 环境信息

| 项目       | 值                                        |
| ---------- | ----------------------------------------- |
| 操作系统   | Windows                                   |
| 工作目录   | `E:\vsc-workspace\lania-zip\lania-cli-v2` |
| Node.js    | `v24.12.0`                                |
| pnpm       | `8.6.1`                                   |
| Cargo      | `1.86.0`                                  |
| Rustc      | `1.86.0`                                  |
| Git        | `2.39.2.windows.1`                        |
| CLI 二进制 | `rust\target\debug\lania-cli.exe`         |

## 4. 进度总览

| 阶段                    | 状态      | 说明                                                             |
| ----------------------- | --------- | ---------------------------------------------------------------- |
| A. 仓库结构与命令面梳理 | ✅ 已完成 | 已确认 Rust host、Node Bridge、npm wrapper、模板与 workflow 结构 |
| B. 构建验证             | ✅ 已完成 | `cargo build --manifest-path rust/Cargo.toml --workspace` 通过   |
| C. Rust 测试            | ✅ 已完成 | 已修复跨平台断言，全量测试通过                                   |
| D. TS 测试              | ✅ 已完成 | 已修复平台包矩阵断言，全量测试通过                               |
| E. 命令手工验收         | ✅ 已完成 | `create` / `tools run` / `add v3` 均已修复并验证                 |
| F. 缺陷修复             | ✅ 已完成 | Windows 执行、运行时解析、v3 模板、测试断言均已处理              |
| G. 修复后全量回归       | ✅ 已完成 | Rust / TS 构建与测试全部通过                                     |
| H. VitePress 工程搭建   | ✅ 已完成 | 依赖、配置、导航、主题、脚本已完成                               |
| I. 文档内容编写         | ✅ 已完成 | 指南、命令、配置、架构、示例、参考页面已创建                     |
| J. 文档站构建验证       | ✅ 已完成 | `pnpm docs:build` 通过，预览页面返回 200                         |

## 5. 命令验收矩阵

| 命令/能力                      | 状态    | 验收方式                                 | 当前结果                                             |
| ------------------------------ | ------- | ---------------------------------------- | ---------------------------------------------------- |
| `lan --help`                   | ✅ 通过 | 显示命令树                               | 正常                                                 |
| `lan config get`               | ✅ 通过 | 读取全局配置                             | 正常                                                 |
| `lan config set`               | ✅ 通过 | 设置 locale / output.mode                | 使用临时 `HOME` 验证 `set/get` 往返正常              |
| `lan template`                 | ✅ 通过 | 列出模板                                 | 返回 4 个模板                                        |
| `lan template toolkit`         | ✅ 通过 | 查看模板详情                             | 正常                                                 |
| `lan tools` / `lan tools list` | ✅ 通过 | 枚举 PATH 命令                           | 扫描 50 个目录，匹配 1039 条                         |
| `lan tools run *.py`           | ✅ 通过 | 运行 Python 文件                         | 输出 `hello from lania`                              |
| `lan tools run *.js`           | ✅ 通过 | 运行 JS 文件                             | 输出 `hello js`                                      |
| `lan tools view`               | ✅ 通过 | 查看文本文件                             | 正常显示行号                                         |
| `lan create --preview`         | ✅ 通过 | 预览脚手架                               | 正常生成计划                                         |
| `lan add --template rfc`       | ✅ 通过 | 生成组件文件                             | 正常写入                                             |
| `lan add --template prettier`  | ✅ 通过 | 生成配置文件                             | 正常写入                                             |
| `lan add --template v3`        | ✅ 通过 | 生成 Vue 3 组件                          | 成功写入 `.vue` 文件                                 |
| `lan generate api init`        | ✅ 通过 | 生成契约样例                             | 生成 `lania.contract.yaml`、`schemas/` 等            |
| `lan generate api plan`        | ✅ 通过 | 规划契约生成                             | 4 write，0 conflict                                  |
| `lan generate module init`     | ✅ 通过 | 生成模块样例                             | 生成 `lania.module.yaml` 等                          |
| `lan generate module plan`     | ✅ 通过 | 规划模块生成                             | 11 write，0 conflict                                 |
| `lan release plan`             | ✅ 通过 | Git 仓库内规划发布                       | 正常输出 `git` / `npm` 命令计划                      |
| `lan sync status`              | ✅ 通过 | 查看同步状态                             | 正常                                                 |
| `lan sync --dry-run`           | ✅ 通过 | 规划同步                                 | 规范 commit message 通过 commitlint                  |
| `lan build`                    | ✅ 通过 | 示例项目构建                             | vite fallback 构建成功                               |
| `lan lint`                     | ✅ 通过 | 示例项目 lint                            | 正常执行，示例文件自身有 1 个 lint error             |
| `lan dev`                      | ✅ 通过 | 启动开发服务器                           | 服务就绪：`http://127.0.0.1:8123`，验证后已停止      |
| `lan product inspect`          | ✅ 通过 | 产品诊断                                 | 正常输出兼容性报告                                   |
| `lan product doctor`           | ✅ 通过 | 产品诊断                                 | 正常                                                 |
| `lan product generate`         | ✅ 通过 | 生成产品 CLI 工程                        | 正常生成                                             |
| `lan product build`            | ✅ 通过 | 构建产品快照                             | 正常                                                 |
| `lan product pack`             | ✅ 通过 | 组装 install-root                        | 正常                                                 |
| `lan product publish`          | ✅ 通过 | 生成发布包布局                           | 正常生成 bundle 与发布产物                           |
| 动态命令 `lan demo read`       | ✅ 通过 | 项目 `lania.schemas.ts` + `ctx.tools.fs` | 通过标准 wrapper 加载项目 manifest，相对路径读取成功 |

## 6. 已发现问题与证据

### 问题 1：Windows 下包管理器命令无法 spawn

**影响命令**

- `lan create`
- 任何通过 `lania-exec` 直接执行 `npm` / `pnpm` / `yarn` / `bun` 的流程

**复现**

```powershell
lan create --name demo-app --template spa-vue --preview
```

**实际结果**

```text
spawn_failed: failed to spawn pnpm view @babel/core version --json:
目录名称无效。 (os error 267)
```

使用 `--package-manager npm` 后同样失败：

```text
spawn_failed: failed to spawn npm view webpack-dev-server version --json:
目录名称无效。 (os error 267)
```

**根因判断**

- Windows 上 `npm` / `pnpm` 通常由 `npm.cmd`、`pnpm.cmd` 提供。
- 当前 `lania-exec` 直接 `Command::new("pnpm")`，没有按 `PATHEXT` 解析 `.cmd` / `.bat` / `.exe`。
- 同时 PATH 中存在无扩展名的 Unix shell shim，Windows 无法直接执行。

**修复方向**

- 在 `lania-exec` 中增加 Windows 可执行文件解析：
  - 优先使用显式路径；
  - 按 `PATHEXT` 搜索 PATH；
  - 对 `.cmd` / `.bat` 通过 `cmd /C` 调用；
  - 跳过不可执行的无扩展名 shim。
- 保持 Unix 行为不变。

**验收标准**

- `lan create --name demo-app --template spa-vue --preview` 能完成渲染预览。
- `lan create --package-manager npm --preview` 能完成渲染预览。
- 包管理器安装/执行路径不再出现 `os error 267` / `os error 193`。

### 问题 2：`lan tools run` 运行时解析不兼容 Windows

**复现**

```powershell
lan tools run .\hello.py
lan tools run .\hello.js
```

**实际结果**

```text
no suitable runtime found for ...hello.py (tried: python3, python)
```

```text
spawn_failed: failed to spawn C:\Users\...\pnpm\bun ...hello.js:
%1 不是有效的 Win32 应用程序。 (os error 193)
```

**根因判断**

- `rust/crates/lania-plugins-command-tools/src/run/resolve.rs` 只检查 `PATH/<program>` 精确文件。
- Windows 真实文件是 `python.exe`、`node.exe`，因此匹配不到。
- 同时会把无扩展名的 Unix shim 误判为可执行文件。

**修复方向**

- 复用 `lania-exec` 的跨平台可执行文件解析函数。
- 解析顺序保持：项目内 `node_modules/.bin` → PATH + `PATHEXT`。
- 对 `.cmd` / `.bat` 由 `lania-exec` 统一通过 `cmd /C` 执行。

**验收标准**

- `lan tools run hello.py` 输出 `hello from lania`。
- `lan tools run hello.js` 输出 `hello js`。
- `lan tools run hello.ts`、`hello.rs`、`hello.go` 至少验证可用运行时。
- 不存在的运行时仍返回清晰错误，而不是误执行。

### 问题 3：`lan add --template v3` 渲染失败

**复现**

```powershell
lan add --name button --template v3
```

**实际结果**

```text
addTemplate.render returned no payload
```

**根因**

`ts/packages/templates/src/add-templates/assets/v3.ejs` 中定义了 `toScriptLang`，但调用的是不存在的 `toLang`：

```ejs
function toScriptLang(processor) { ... }
const langAttr = toLang(cssProcessor);
```

EJS 渲染抛出 `ReferenceError`，Node Bridge 返回空 payload，Rust 侧报 `returned no payload`。

**修复方向**

- 统一函数名与调用名。
- 同步修复 `dist` 中的模板资产或重新构建 templates 包。
- 增加 v3 模板渲染回归测试。

**验收标准**

- `lan add --name button --template v3` 成功写入 `.vue` 文件。
- 生成内容包含 `<script setup lang="ts">` 或等价内容。
- `pnpm -C ts test` 中相关模板测试通过。

### 问题 4：Rust 测试在 Windows 上路径断言失败

**失败包**

`lania-plugins-command-build`

**失败用例**

- `tests::builds_build_bridge_request_from_context`
- `tests::builds_product_build_bridge_request_from_context`
- `tests::builds_product_pack_bridge_request_from_context`
- `tests::builds_product_publish_bridge_request_from_context`
- `tests::builds_product_inspect_bridge_request_from_context`
- `tests::builds_product_doctor_bridge_request_from_context`

**证据**

```text
left:  String("/repo\\apps/demo")
right: "/repo/apps/demo"
```

**根因**

测试使用 Unix 路径字符串做期望值，Windows 下 `Path` 拼接产生 `\`。

**修复方向**

- 测试期望值使用 `PathBuf` 或 `std::path::MAIN_SEPARATOR` 生成。
- 不改变生产代码行为。

**验收标准**

```powershell
cargo test --manifest-path rust\Cargo.toml --workspace
```

全部通过。

### 问题 5：Node Bridge `product.publish` 测试在 Windows 上失败

**失败用例**

`product.publish creates a publish-ready npm package layout from packed output`

**证据**

```text
actual:   './bundle/cli-win32-x64-package/lania-cli-cli-win32-x64-0.1.0.tgz'
expected: null
```

**根因**

- 测试只移除了 `npm/cli-darwin-arm64/bin/lania-cli`。
- 当前仓库存在 `npm/cli-win32-x64/bin/lania-cli`，Windows 下会自动发现并打入平台包。
- 断言却固定期望 `platformTarball === null`。

**修复方向**

- 测试开始前备份并移除当前宿主平台对应的 staging binary；
- 或按宿主平台动态计算期望值。
- 优先方案：移除宿主平台 staging binary，保持“无平台包”的测试语义。

**验收标准**

```powershell
pnpm -C ts test
```

全部通过。

## 7. 详细执行步骤

### Phase 1：修复 Windows 命令执行

**要做的事**

1. 在 `rust/crates/lania-exec/src/lib.rs` 增加跨平台可执行文件解析。
2. Windows 下按 `PATHEXT` 解析 `.exe` / `.cmd` / `.bat`。
3. `.cmd` / `.bat` 通过 `cmd /C` 执行。
4. 保留 Unix 下现有行为。
5. 导出解析函数供 `lania-plugins-command-tools` 复用。

**要达到的效果**

- `npm` / `pnpm` / `bun` 等命令在 Windows 可正常 spawn。
- `lan create` 的依赖版本查询不再失败。
- 不引入 shell 注入风险；不改变命令参数语义。

**建议验证**

```powershell
cargo test -p lania-exec
lan create --name demo-app --template spa-vue --preview
```

### Phase 2：修复 `tools run` 运行时解析

**要做的事**

1. 修改 `rust/crates/lania-plugins-command-tools/src/run/resolve.rs`。
2. 复用 Phase 1 的解析函数。
3. 保持项目内 `node_modules/.bin` 优先级。
4. 过滤不可执行 shim。

**要达到的效果**

- `python`、`node`、`tsx`、`rustc`、`go` 等运行时在 Windows 可被正确找到。
- JS/TS 文件优先使用项目内 `node_modules/.bin`，否则使用系统 `node`。

**建议验证**

```powershell
lan tools run .\hello.py
lan tools run .\hello.js
lan tools run .\hello.ts
lan tools run .\hello.rs
```

### Phase 3：修复 `add` v3 模板

**要做的事**

1. 修正 `ts/packages/templates/src/add-templates/assets/v3.ejs`。
2. 同步 `dist` 资产或执行 templates 包构建。
3. 增加或更新 v3 渲染测试。

**要达到的效果**

- v3 模板可以稳定生成 Vue 3 单文件组件。
- 模板测试覆盖 v2 / v3 / rfc / rcc / 配置文件类模板。

**建议验证**

```powershell
pnpm -C ts test
lan add --name button --template v3
```

### Phase 4：修复跨平台测试

**要做的事**

1. 修正 Rust `lania-plugins-command-build` 的 6 个路径断言。
2. 修正 Node Bridge `product.publish` 测试的宿主平台 staging binary 处理。
3. 不为了让测试通过而放宽生产逻辑。

**要达到的效果**

```powershell
cargo test --manifest-path rust\Cargo.toml --workspace
pnpm -C ts test
```

两条命令在 Windows 全部通过。

### Phase 5：全量回归

**要做的事**

1. 重新构建 Rust workspace。
2. 重新执行 TS typecheck / test。
3. 按「命令验收矩阵」重新跑关键命令。
4. 记录所有仍失败项，区分“环境缺失”和“产品缺陷”。

**要达到的效果**

- 命令验收矩阵全部为 ✅，或明确标注仅因本机缺少外部工具而无法验证。
- Rust / TS 测试全部通过。

**建议命令**

```powershell
cargo build --manifest-path rust\Cargo.toml --workspace
cargo test --manifest-path rust\Cargo.toml --workspace
pnpm -C ts typecheck
pnpm -C ts test
```

### Phase 6：VitePress 工程搭建

**要做的事**

1. 新建 `docs-site/`（或 `website/`）目录。
2. 安装 `vitepress`。
3. 增加根 `package.json` 脚本：
   - `docs:dev`
   - `docs:build`
   - `docs:preview`
4. 配置站点：
   - 中文语言；
   - 顶部导航；
   - 分组侧边栏；
   - 本地搜索；
   - 深色模式；
   - 大纲；
   - 编辑链接；
   - 上一篇/下一篇；
   - 页脚。
5. 增加主题样式，参考 Vue 3 官方文档的绿色品牌色与卡片式首页。

**要达到的效果**

- `pnpm docs:dev` 可启动本地文档站。
- 页面结构、导航、侧边栏、搜索、暗色模式均可使用。

### Phase 7：文档内容编写

**建议目录结构**

```text
docs-site/
  .vitepress/
    config.ts
    theme/
      index.ts
      custom.css
  index.md
  guide/
    introduction.md
    getting-started.md
    installation.md
    first-project.md
    commands.md
    configuration.md
    templates.md
    dynamic-commands.md
    product-cli.md
    tools.md
    output-and-locale.md
    troubleshooting.md
  reference/
    cli.md
    config.md
    templates.md
    schemas.md
  architecture/
    overview.md
    rust-host.md
    node-bridge.md
    plugin-system.md
    workflows.md
  cookbook/
    create-project.md
    generate-api.md
    generate-module.md
    release.md
    product-publish.md
  contributing/
    development.md
    testing.md
    release.md
```

**内容要求**

- 以“学习路径”为主线，而不是简单罗列 README。
- 每个命令包含：用途、语法、参数、示例、输出说明、常见问题。
- 配置章节覆盖 `lan.config.js/cjs/json/ts`、输出模式、locale、动态命令、产品配置。
- 架构章节解释 Rust Host、Node Bridge、npm wrapper、插件、workflow 的协作关系。
- 示例章节使用仓库内 `examples/` 的真实路径。
- 重要页面提供“下一步”链接，形成 Vue 官方文档式学习动线。

### Phase 8：文档站构建验证

**要做的事**

1. 执行 `pnpm docs:build`。
2. 检查构建产物无死链、无缺失资源。
3. 启动 `pnpm docs:preview`，抽查首页、指南页、参考页、暗色模式、搜索。
4. 清理临时文件与验证目录。

**要达到的效果**

- VitePress 构建成功。
- 首页、导航、侧边栏、搜索、暗色模式正常。
- 文档中引用的命令与当前实现一致。

## 8. 变更文件清单（计划）

### 功能修复

- `rust/crates/lania-exec/src/lib.rs`
- `rust/crates/lania-plugins-command-tools/src/run/resolve.rs`
- `ts/packages/templates/src/add-templates/assets/v3.ejs`
- `ts/packages/templates/dist/add-templates/assets/v3.ejs`
- `rust/crates/lania-plugins-command-build/src/lib.rs`（仅测试断言）
- `ts/packages/node-bridge/src/plugins/product.test.ts`

### 文档站

- `docs-site/.vitepress/config.ts`
- `docs-site/.vitepress/theme/index.ts`
- `docs-site/.vitepress/theme/custom.css`
- `docs-site/index.md`
- `docs-site/guide/*.md`
- `docs-site/reference/*.md`
- `docs-site/architecture/*.md`
- `docs-site/cookbook/*.md`
- `docs-site/contributing/*.md`
- `docs-site/package.json`（`vitepress` 依赖与 `algoliasearch` override）
- `docs-site/package-lock.json`（由 `npm install` 生成）
- `package.json`（新增 `docs:install` / `docs:dev` / `docs:build` / `docs:preview`）

### 任务文档

- `docs/operations/lania-cli-v2-任务执行.zh-CN.md`（本文档）

## 9. 验收标准汇总

### 功能验收

- [x] `lan create --preview` 在 Windows 成功。
- [x] `lan add --template v3` 成功。
- [x] `lan tools run` 支持 Python / JS / TS。
- [x] `lan tools run` 对未知运行时返回清晰错误。
- [x] `lan build` / `lan lint` / `lan dev` 正常。
- [x] `lan generate api` / `lan generate module` 正常。
- [x] `lan release plan` / `lan sync` 正常。
- [x] `lan product inspect/doctor/generate/build/pack/publish` 正常。

### 测试验收

- [x] `cargo build --manifest-path rust\Cargo.toml --workspace` 通过。
- [x] `cargo test --manifest-path rust\Cargo.toml --workspace` 通过。
- [x] `pnpm -C ts typecheck` 通过。
- [x] `pnpm -C ts test` 通过。

### 文档验收

- [x] `pnpm docs:dev` 可启动。
- [x] `pnpm docs:build` 通过。
- [x] 首页、指南、参考、架构、示例、贡献指南均可访问。
- [x] 顶部导航、侧边栏、本地搜索、深色模式正常。
- [x] 文档命令示例与当前 CLI 行为一致。

## 10. 当前遇到的问题与风险

| 风险                           | 影响                        | 处理方式                                     |
| ------------------------------ | --------------------------- | -------------------------------------------- |
| Windows 无扩展名 shim 干扰执行 | `create` / `tools run` 失败 | 按 `PATHEXT` 解析并过滤不可执行文件          |
| `.cmd` 不能直接 CreateProcess  | 包管理器调用失败            | 使用 `cmd /C` 包装                           |
| `v3.ejs` 变量名错误            | `add v3` 失败               | 修正模板并重建 dist                          |
| 测试硬编码 Unix 路径           | Rust 测试失败               | 测试使用平台无关路径                         |
| 测试假设无平台二进制           | Node Bridge 测试失败        | 测试前移除宿主平台 staging binary            |
| 本机缺少部分运行时             | 命令验收不完整              | 记录为环境限制，不误判为产品缺陷             |
| VitePress 内容丰富度高         | 工作量大、易遗漏            | 按 Phase 7 目录逐页补齐，最后统一导航检查    |
| 文档与实现漂移                 | 学习文档误导                | 以当前 CLI `--help` 与源码为准，写完逐条核对 |

## 11. 修复记录

### 2026-10-19

| 文件                                                                        | 修改                                                                                     |
| --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `rust/crates/lania-exec/src/lib.rs`                                         | 新增 `resolve_executable`，Windows 按 `PATHEXT` 解析；`.cmd` / `.bat` 通过 `cmd /C` 启动 |
| `rust/crates/lania-plugins-command-tools/src/run/resolve.rs`                | 复用 `resolve_executable`，修复 `tools run` 运行时解析                                   |
| `rust/crates/lania-plugins-command-tools/src/run.rs`                        | 展示运行时名称时去掉 Windows 可执行扩展名                                                |
| `rust/crates/lania-workflows/src/create/create_workflow.rs`                 | 依赖版本查询改用已存在的 `input.cwd`，避免目标目录未创建导致 spawn 失败                  |
| `ts/packages/templates/src/add-templates/assets/v3.ejs`                     | 修正 EJS scriptlet 语法与样式语言函数                                                    |
| `ts/packages/templates/dist/add-templates/assets/v3.ejs`                    | 同步构建产物                                                                             |
| `ts/packages/templates/src/template.test.ts`                                | 增加 v3 渲染回归测试                                                                     |
| `rust/crates/lania-plugins-command-build/src/lib.rs`                        | 测试路径断言改为平台无关                                                                 |
| `rust/crates/lania-plugins-command-dev/src/lib.rs`                          | 测试路径断言改为平台无关                                                                 |
| `rust/crates/lania-plugins-command-tools/src/view/tests.rs`                 | 树形目录断言改为平台无关                                                                 |
| `rust/crates/lania-plugins-command-tools/src/run/tests.rs`                  | 本地运行时测试适配 Windows；缺少 Java 时跳过                                             |
| `rust/crates/lania-host/src/runtime/host_rpc/fs_domain.rs`                  | `host.fs.exists/read/readdir/stat` 按 `cwd` 解析相对路径，修复动态命令读取工作区文件失败 |
| `rust/crates/lania-host/src/runtime/host_rpc.rs`                            | 增加相对路径 Host RPC 回归测试                                                           |
| `npm/cli/bin/lan.mjs`                                                       | 标准 CLI 的 `LANIA_PRODUCT_ROOT` 改为当前项目目录，修复项目动态命令不加载                |
| `ts/packages/node-bridge/src/plugins/dynamic-commands/hooks.ts`             | inline hook 默认 kind 对齐 Rust 的 8 个 waterfall key                                    |
| `ts/packages/node-bridge/src/plugins/dynamic-commands-test/inline-hooks.ts` | 增加 `onArgsParsed` inline hook 默认 waterfall 断言                                      |
| `docs-site/public/logo.svg` / `hero.svg`                                    | 设计 Laniakea 超星系团 + CLI 提示符品牌标志，并重绘首页 Hero                             |
| `docs-site/guide/introduction.md`                                           | 增加“名字与标志”品牌说明                                                                 |
| `README.md` / `README.zh-CN.md` / `docs/operations/publish-ci.md`           | 旧产品命令改为 `lan product build/pack/publish`                                          |
| `package.json`                                                              | `publish:npm` 改为合法命令 `lan product publish`                                         |
| `ts/packages/node-bridge/src/plugins/product/handlers/{build,inspect}.ts`   | 错误提示改为合法命令形式                                                                 |
| `ts/packages/node-bridge/src/plugins/product.test.ts`                       | 移除所有平台 staging binary，修正平台矩阵预期                                            |
| `docs-site/**`                                                              | 新增 VitePress 文档站                                                                    |
| `package.json`                                                              | 新增 `docs:install` / `docs:dev` / `docs:build` / `docs:preview`                         |
| `README.md` / `README.zh-CN.md`                                             | 增加文档站入口                                                                           |

## 12. 文档完整性补强（已完成）

第二轮文档补强目标：从“使用手册”升级为“完整原理手册”。

已完成：

- Rust Host 与 Node Bridge 的通信协议、请求/响应/事件、生命周期与错误映射：
  `architecture/bridge-protocol.md`。
- Node 与 Rust 双向方法目录：`reference/bridge-methods.md`、`reference/host-rpc.md`。
- `ctx.tools` 宿主能力对象：使用指南、完整 API、权限策略、Host RPC 路径：
  `guide/host-tools.md`、`reference/host-tools.md`。
- 命令类型与加载原理：内置 Rust 插件、Bridge 命令、Workflow、混合命令、动态命令、产品命令：
  `guide/command-model.md`。
- 运行时命令开发：manifest 全字段、prompt、hooks、handler、workflow、scaffold、插件绑定、
  端到端示例：`guide/runtime-commands.md`、`reference/manifest.md`。
- 项目插件开发与安全边界：来源、allowlist、方法交集、签名策略、Hook：
  `architecture/plugin-system.md`。
- 配置全量字段、全局偏好、输出 profile、事件模式、状态文件：
  `reference/config.md`、`reference/output-and-state.md`。
- CommandSpec / Handler / Capability / Hook 的 Rust 扩展开发方式：
  `guide/runtime-commands.md` 第 10 节。
- 模板目录协议、发现顺序、模板 Bridge 方法与 add 模板上下文：
  `reference/templates.md`。
- 其他代码中已存在但文档缺失的命令、参数、别名与兼容行为：
  以 `lan --help` 与各子命令 `--help` 为准更新 `reference/cli.md`。

补强后 `pnpm docs:build` 再次通过。

## 13. 后续可选工作

当前主任务已完成。后续可选增强：

1. 为文档站补充英文版本。
2. 增加更多架构时序图与请求示例。
3. 为 `lan product dev` 增加手工验收记录。
4. 把文档站构建加入 CI。
5. 增加文档死链检查与 Markdown Lint。

## 14. 验证命令清单

```powershell
# Rust
cargo build --manifest-path rust\Cargo.toml --workspace
cargo test --manifest-path rust\Cargo.toml --workspace

# TypeScript
pnpm -C ts typecheck
pnpm -C ts test

# CLI 手工验收
.\rust\target\debug\lania-cli.exe --help
.\rust\target\debug\lania-cli.exe template
.\rust\target\debug\lania-cli.exe tools --plain --names-only --unique --limit 8
.\rust\target\debug\lania-cli.exe create --name demo-app --template spa-vue --preview
.\rust\target\debug\lania-cli.exe add --name button --template v3
.\rust\target\debug\lania-cli.exe generate api init
.\rust\target\debug\lania-cli.exe generate module init
.\rust\target\debug\lania-cli.exe release plan --profile package --version 1.2.3
.\rust\target\debug\lania-cli.exe sync status

# 文档站（Phase 6 之后）
pnpm docs:dev
pnpm docs:build
pnpm docs:preview
```
