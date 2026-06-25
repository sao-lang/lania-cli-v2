# 发布 CI 与 Registry 收尾

本文补充 `publish-manifest` 主线的工程化收尾，包括：

- CI 集成
- 真实 registry 凭据管理
- Verdaccio 自动化演练
- 发布失败后的重试 / 回滚策略

## 1. CI 工作流

仓库新增两个 GitHub Actions：

- `.github/workflows/publish-product.yml`
  - 通过 `workflow_dispatch` 手动触发
  - 依次执行 `lan build product`、`lan pack product`、`lan publish product`
  - 可选择只生成 `publish-manifest.json`，也可在同一条流水线里直接执行真实发布
  - 始终上传 `publish-manifest.json`、`publish-report.json` 与 bundle 产物，便于失败后继续恢复

- `.github/workflows/publish-verdaccio-smoke.yml`
  - 在 `pull_request`、`push main`、`workflow_dispatch` 下执行
  - 通过 `pnpm publish:verdaccio` 拉起临时 Verdaccio，生成临时 tarball，执行真实 `npm publish`
  - 最后用 `npm view` 校验包是否已经进入本地 registry

- `.github/workflows/publish-manifest.yml`
  - 通过 `workflow_dispatch` 手动触发
  - 读取指定 `publish-manifest.json`
  - 先生成临时 `.npmrc`
  - 再执行 `node ./scripts/publish.mjs --manifest ...`

这样区分后：

- `publish-product` 负责“生成 manifest + 可选执行发布”的主流水线
- `publish-verdaccio-smoke` 负责持续集成里的自动回归
- `publish-manifest` 负责真实 registry 的受控发布

## 2. 真实凭据管理

新增脚本：

- `node ./scripts/configure-npm-auth.mjs`

用途：

- 从 `--manifest` 或 `--registry` 推导 registry 地址
- 从 `LANIA_NPM_TOKEN`、`NODE_AUTH_TOKEN`、`NPM_TOKEN` 中读取 token
- 输出临时 `.npmrc`

示例：

```bash
NODE_AUTH_TOKEN=xxxxx \
node ./scripts/configure-npm-auth.mjs \
  --manifest ./.lania/publish/product/npm-package/publish-manifest.json \
  --output ./.lania/tmp/npmrc.publish
```

生成结果会写入：

```ini
registry=https://registry.npmjs.org/
always-auth=true
//registry.npmjs.org/:_authToken=xxxxx
```

推荐做法：

- 不在仓库根目录写死 token
- CI 中通过 `NPM_CONFIG_USERCONFIG` 指向临时 `.npmrc`
- 真实发布只使用 secret 注入的短生命周期 token
- `publish-product` 只有在 `execute=true` 时才会写入凭据并执行 registry publish

## 3. Verdaccio 演练

新增脚本：

- `pnpm publish:verdaccio`

它会自动完成：

1. 启动临时 Verdaccio
2. 创建本地测试用户 / token
3. 调用 `configure-npm-auth.mjs` 生成临时 `.npmrc`
4. 打两个临时 npm tarball
5. 生成 `publish-manifest.json`
6. 执行 `scripts/publish.mjs --manifest ... --yes`
7. 用 `npm view` 回查发布结果

它的目标不是替代真实发布，而是把以下关键路径变成 CI 可回归能力：

- `whoami` 预检
- `npm publish <tarball>`
- manifest 执行顺序
- registry 可见性验证

## 4. 重试与回滚

`scripts/publish.mjs --manifest` 与 `product.publish` 现在新增以下策略：

- `--max-retries <n>`
  - 对网络型失败进行重试
- `--retry-delay-ms <ms>`
  - 控制重试间隔
- `--rollback-on-failure`
  - 在真实发布且出现部分成功时，按逆序执行 `npm unpublish`

当前识别为可重试的失败主要包括：

- `EAI_AGAIN`
- `ECONNRESET`
- `ECONNREFUSED`
- `ETIMEDOUT`
- `ENOTFOUND`
- `socket hang up`
- `429`
- `502 / 503 / 504`

执行状态现在会持续写回 `publish-manifest.json` 与 `publish-report.json`，新增：

- `execution.retryPolicy`
- `execution.attempts`
- `execution.rollbackPlan`

其中 `rollbackPlan` 会记录：

- 需要逆序处理的包
- 目标 registry
- 建议或已执行的 `npm unpublish` 命令

默认行为仍然保守：

- 不会因为失败自动回滚，除非显式传入 `--rollback-on-failure`
- 非 `--dry-run` 场景仍然必须显式传入 `--yes`

## 5. 推荐发布顺序

建议在真实环境中按下面顺序使用：

1. `pnpm publish:verdaccio`
2. 优先触发 `Publish Product` 工作流生成目标环境的 `publish-manifest.json`
3. 若要在同一条流水线里真实执行，设置 `execute=true`
4. 首次建议保持 `dry_run=true`
5. 确认无误后再用 `execute=true` + `dry_run=false`
6. 如需灾备，再加 `rollback_on_failure=true`

这样可以把“可发布”、“能重试”、“有回滚计划”、“有 CI 演练”四件事闭环起来。
