# 常见问题

<div class="lead">
本页收集安装、Windows 兼容、Node Bridge、模板、发布与测试中的常见问题。排查时先确认 stdout 结果与 stderr 日志，再判断问题发生在 Rust 宿主还是 Node 侧。
</div>

## 命令找不到或启动失败

### `lan` 不在 PATH

```bash
npm root -g
npm bin -g
```

确认全局 bin 目录在 PATH 中，或直接使用：

```bash
node ./npm/cli/bin/lan.mjs --help
```

### 只有 `lan`，没有 `lania-cli`

平台二进制没有正确安装。检查：

```bash
npm ls -g @lania-cli/cli
```

以及对应平台包是否安装。

## Windows 兼容

### 包管理器命令无法 spawn

历史上 Windows 下会出现：

```text
spawn_failed: failed to spawn pnpm view ... 目录名称无效。 (os error 267)
```

当前版本已修复：

- 按 `PATHEXT` 解析 `.exe` / `.cmd` / `.bat`。
- `.cmd` / `.bat` 通过 `cmd /C` 执行。
- 跳过 npm/pnpm 留下的无扩展名 Unix shim。

如果仍出现问题，确认：

```powershell
where.exe pnpm
where.exe npm
$env:PATHEXT
```

### `tools run` 找不到运行时

```text
no suitable runtime found for ... (tried: python3, python)
```

确认运行时真实存在于 PATH：

```powershell
where.exe python
where.exe node
where.exe tsx
```

当前解析逻辑会：

1. 先查项目内 `node_modules/.bin`。
2. 再查 PATH，并按 `PATHEXT` 展开。
3. 过滤不可执行的无扩展名 shim。

## Node Bridge 问题

### `template` 或 `add` 报 Bridge 错误

检查 Bridge 目录：

```text
ts/packages/node-bridge/dist
```

开发态需要先构建：

```bash
cd ts
pnpm install
pnpm build
```

或显式设置：

```powershell
$env:LANIA_NODE_BRIDGE_DIR = "E:\path\to\node-bridge"
```

### Bridge 方法返回空 payload

常见原因：

- 插件抛出了异常。
- 模板资产缺失或语法错误。
- 配置加载失败。

先看 stderr 中的 `host.runtime` 日志，再检查对应插件与资产。

## 模板问题

### `add --template v3` 失败

历史版本存在 EJS 语法问题：

```text
addTemplate.render returned no payload
```

当前版本已修复 `v3.ejs` 的 scriptlet 与样式语言函数。若仍失败，确认 `templates/dist` 资产已更新：

```bash
cd ts
pnpm --filter @lania-cli/templates build
```

### `create` 卡在依赖版本查询

`create` 会执行：

```text
<pnpm|npm> view <pkg> version --json
```

需要 registry 可访问。离线环境可以：

- 使用 `--skip-install`。
- 或在模板依赖中写死版本。

## Lint 与构建

### 缺少工具依赖

例如：

```text
declared dependency `vite` is missing from package.json
```

按项目需要安装对应工具：

```bash
pnpm add -D vite
```

### Lint 发现已有错误

`lan lint` 的退出码反映真实结果。示例项目中的文件本身可能有 lint error，不代表 CLI 故障。

## 发布问题

### commitlint 拒绝提交信息

```text
commitlint rejected commit message: test sync
```

使用 Conventional Commits：

```bash
lan sync --message "chore(sync): update workspace"
```

### `product publish --execute --dry-run` 仍失败

该流程会先做 registry 预检，例如 `npm whoami`。需要：

- 目标 registry 可访问。
- 当前环境已登录 npm。

只验证产物时，直接执行：

```bash
lan product publish
```

## 测试问题

### Rust 测试路径断言失败

历史测试硬编码 Unix 路径，Windows 下会出现：

```text
left:  "/repo\\apps/demo"
right: "/repo/apps/demo"
```

当前已改为平台无关断言。

### Node Bridge 平台包测试失败

历史测试假设宿主平台二进制不存在。当前测试会移除所有平台 staging binary，保证矩阵稳定。

## 收集诊断信息

```bash
lan --version
lan config get
lan template
```

并提供：

- 操作系统与架构
- Node / pnpm / Rust 版本
- 完整 stderr
- stdout 中的结构化错误
- 相关 `lan.config.*` 与 manifest

## 下一步

- [安装与运行原理](/guide/installation)
- [输出、语言与进度](/guide/output-and-locale)
- [测试与验收](/contributing/testing)
