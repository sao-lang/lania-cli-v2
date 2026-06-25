# product-cli-demo

[English](./README.md)

这是一个最小可运行的产品 CLI 示例工程，用来演示：

- 如何通过 `lan.config.json` 声明产品 CLI 基础配置
- 如何通过 `product/lania.schemas.ts` 定义产品命令与 workflow
- 如何给产品 CLI 增加一个带参数和补问能力的 `deploy` 命令
- 如何通过 `lan product build`、`lan product pack`、`lan product publish` 验证产品化链路

## 目录说明

- `lan.config.json`
  - 产品包名、命令名、模板目录、兼容范围和最小工具权限策略
- `product/lania.schemas.ts`
  - 示例命令定义
  - `hello`：验证产品命令运行时上下文
  - `deploy`：演示参数、prompt、落盘报告和日志输出
  - `product-template`：扫描产品模板并写出模板报告
- `product/templates/demo-app/template.json`
  - 产品自带模板的元信息
- `product/templates/demo-app/files/README.md.ejs`
  - 模板渲染产物示例

## 快速开始

在当前目录下执行：

```bash
lan product inspect --path .
lan product dev hello --path .
lan product dev deploy --path . -- --service api --env staging --confirm
lan product build
lan product pack
lan product publish
```

如果你想继续验证 publish manifest 的执行路径：

```bash
lan product publish --execute --dry-run --yes
```

注意：

- `--execute --dry-run` 仍然会先做 registry 预检，例如 `npm whoami`
- 如果本机没有配置 npm 登录态，更适合先执行 `lan product publish`
- 如果要在本地完整演练 registry publish，建议使用 `pnpm publish:verdaccio`

## 命令说明

### `lan product inspect --path .`

用途：

- 检查产品配置、schema 入口、模板目录和本地产物状态

你会看到：

- 产品包名和命令名
- schema 入口与 schema roots
- `build / pack / publish` 三类产物的当前状态
- 下一步建议

### `lan product dev hello --path .`

用途：

- 在开发模式下执行本地产品命令

这个命令会返回：

- `message`
- `binaryName`
- `workspaceRoot`
- `productRoot`
- `schemaRoot`

它适合用来理解“产品命令实际运行时能拿到哪些上下文字段”。

### `lan product dev deploy --path . -- --service api --env staging --confirm`

用途：

- 演示一个更接近业务命令的产品 CLI 命令

这个命令会：

- 读取 `service`、`env`、`confirm` 三个输入
- 在 `.lania/reports/deploy-plan.json` 中写出一份部署计划
- 输出一条成功日志
- 返回结构化结果，方便脚本化调用

如果你省略 `--service` 或 `--env`，该命令的定义本身已经带了 `prompt`，后续可以直接扩成真正的交互式部署命令。

### `lan product build`

用途：

- 生成产品 snapshot

主要产物：

- `.lania/build/product`
- `build-report.json`

### `lan product pack`

用途：

- 把产品 snapshot 组装成 install-root 布局

主要产物：

- `.lania/pack/product/install-root/bin`
- `.lania/pack/product/install-root/lib/product`
- `.lania/pack/product/install-root/lib/node-bridge`

### `lan product publish`

用途：

- 生成产品 npm 包、wrapper、官方 CLI bundle 和 publish manifest

主要产物：

- `.lania/publish/product/npm-package/publish-manifest.json`
- `.lania/publish/product/npm-package/publish-report.json`
- 对应 tarball 和 wrapper 文件

这一步不会真正向 registry 执行发布。

## 推荐阅读顺序

如果你是第一次接触产品 CLI 能力，建议按这个顺序看：

1. `lan.config.json`
2. `product/lania.schemas.ts`
3. 先跑 `lan product inspect --path .`
4. 再跑 `lan product dev hello --path .`
5. 然后跑 `lan product dev deploy --path . -- --service api --env staging --confirm`
6. 最后再看 `lan product build / pack / publish`

这样会更容易把“命令定义”“运行时上下文”“产品化产物”三层关系串起来。
