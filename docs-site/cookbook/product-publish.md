# 打包并发布产品 CLI

<div class="lead">
目标：把一个产品 CLI 工程从开发态推进到可分发的 npm 包，并理解 build / pack / publish 三个阶段。
</div>

## 1. 生成产品工程

```bash
lan product generate \
  --preset demo \
  --name "Acme CLI" \
  --binary-name acme \
  --output-dir ./acme-cli \
  --force
```

## 2. 检查工程

```bash
cd acme-cli
lan product inspect --path .
lan product doctor --path .
```

检查：

- 产品配置
- schema 入口
- 模板目录
- 兼容范围
- 本地产物状态

## 3. 开发产品命令

```bash
lan product dev hello --path .
lan product dev deploy --path . -- --service api --env staging --confirm
```

## 4. 构建快照

```bash
lan product build
```

产物：

```text
.lania/build/product/
build-report.json
```

## 5. 组装 install-root

```bash
lan product pack
```

产物：

```text
.lania/pack/product/install-root/
  bin/
  lib/product/
  lib/node-bridge/
```

## 6. 生成发布包

```bash
lan product publish --dist-tag next --channel next
```

产物：

```text
.lania/publish/product/npm-package/
  package.json
  bin/
  lib/
  publish-manifest.json
  publish-report.json
  *.tgz
```

这一步不会真正向 registry 发布。

## 7. 发布预检

```bash
lan product publish --execute --dry-run --yes
```

注意：

- 会执行 registry 预检，例如 `npm whoami`。
- 需要目标 registry 可访问且已登录。

## 8. 真实发布

```bash
lan product publish --execute --yes --max-retries 2 --retry-delay-ms 2000
```

可选：

```bash
lan product publish --execute --yes --rollback-on-failure
```

## 9. 本地 registry 演练

```bash
pnpm publish:verdaccio
```

## 发布清单

`publish-manifest.json` 包含：

- 发布顺序
- 包依赖关系
- 平台矩阵
- registry 配置
- 重试与回滚策略
- 每一步的命令

## 预期结果

- `build` 产出产品快照。
- `pack` 产出可运行 install-root。
- `publish` 产出 npm 包与发布清单。
- dry-run 通过后才考虑真实发布。
- 真实发布可通过 manifest 恢复与重试。

## 下一步

- [产品 CLI](/guide/product-cli)
- [发布与同步](/guide/release)
- [配置字段参考](/reference/config)
