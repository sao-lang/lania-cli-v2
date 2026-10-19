# 产品 CLI

<div class="lead">
product 命令族把 Lania CLI 从“工程工具”变成“可分发的 CLI 产品”：生成产品工程、开发本地命令、构建快照、组装 install-root、生成 npm 发布包，并提供诊断能力。
</div>

## 命令结构

```text
lan product
  generate  生成产品 CLI 工程骨架
  dev       以开发模式运行产品命令
  build     生成产品快照
  pack      组装 install-root 布局
  publish   生成可发布的 npm 包布局
  inspect   检查配置、schema 与产物状态
  doctor    运行兼容性与完整性诊断
```

## generate

```bash
lan product generate \
  --name "Acme CLI" \
  --binary-name acme \
  --output-dir ./acme-cli \
  --force
```

参数：

| 参数                        | 作用          |
| --------------------------- | ------------- |
| `--preset <demo\|minimal>`  | 脚手架预设    |
| `-i, --interactive`         | 交互式补问    |
| `-n, --name <name>`         | 展示名        |
| `-b, --binary-name <name>`  | 二进制/命令名 |
| `-p, --package-name <name>` | npm 包名      |
| `-o, --output-dir <dir>`    | 输出目录      |
| `-f, --force`               | 覆盖已有文件  |

## dev

```bash
lan product dev hello --path ./products/acme-cli
lan product dev deploy --path ./products/acme-cli -- --service api --env staging
```

`product dev` 会在开发模式下执行产品自己的动态命令，适合迭代 `lania.schemas.ts`。

## build

```bash
lan product build
lan product build --output-dir .lania/build/product --clean
```

产物：

- `.lania/build/product`
- `build-report.json`

## pack

```bash
lan product pack
lan product pack --build-dir .lania/build/product --clean
```

产物：

```text
.lania/pack/product/install-root/
  bin/
  lib/product/
  lib/node-bridge/
```

## publish

```bash
# 只生成发布物，不真正发布
lan product publish

# 生成 next 通道发布清单
lan product publish --dist-tag next --channel next

# 对发布清单做 dry-run 预检
lan product publish --execute --dry-run --yes
```

常用选项：

| 选项                             | 作用                             |
| -------------------------------- | -------------------------------- |
| `--pack-dir <dir>`               | pack 产物目录                    |
| `-o, --output-dir <dir>`         | 发布输出目录                     |
| `-t, --dist-tag <tag>`           | 发布 dist-tag                    |
| `-C, --channel <channel>`        | 发布通道                         |
| `--registry <url>`               | 覆盖 registry                    |
| `--platform-binaries-dir <dir>`  | 平台二进制目录                   |
| `--platform-binary-paths <json>` | 平台到二进制路径的 JSON 映射     |
| `--execute`                      | 执行 publish manifest            |
| `--dry-run`                      | 使用 npm publish --dry-run       |
| `-y, --yes`                      | 真实发布确认                     |
| `--resume`                       | 从已完成步骤恢复                 |
| `--otp <code>`                   | 传入 npm OTP                     |
| `--npm-bin <path>`               | 覆盖 npm 可执行文件              |
| `--max-retries <n>`              | 瞬时失败重试次数                 |
| `--retry-delay-ms <n>`           | 重试间隔                         |
| `--rollback-on-failure`          | 发布部分失败后执行 npm unpublish |
| `--clean`                        | 清理输出目录                     |

### 产物

```text
.lania/publish/product/npm-package/
  package.json
  bin/
  lib/
  publish-manifest.json
  publish-report.json
  *.tgz
```

`publish-manifest.json` 包含：

- 发布步骤顺序
- 依赖关系
- 平台矩阵
- registry 配置
- 重试与回滚策略

## inspect 与 doctor

```bash
lan product inspect --path . --compat
lan product doctor --path .
```

检查项：

- 产品配置
- schema 入口与 schema roots
- 模板目录
- build / pack / publish 产物状态
- 框架、协议、Node Bridge 版本兼容性

## 产品配置

在 `lan.config.json` 中声明：

```json
{
  "extensions": { "dynamicCommands": true },
  "product": {
    "name": "@acme/cli",
    "binaryName": "acme",
    "displayName": "Acme CLI",
    "templatesDir": "./product/templates",
    "compat": {
      "frameworkVersionRange": ">=0.1.0 <1.0.0",
      "protocolVersionRange": "0.1.0",
      "nodeBridgeVersionRange": "0.1.0"
    }
  },
  "schema": {
    "entry": "./product/lania.schemas.ts"
  }
}
```

## 推荐验证顺序

```bash
lan product generate --preset demo --name "Acme CLI" --binary-name acme --output-dir ./acme-cli
cd ./acme-cli
lan product inspect --path .
lan product dev hello --path .
lan product build
lan product pack
lan product publish
```

## 下一步

- [打包并发布产品 CLI](/cookbook/product-publish)
- [动态命令与插件](/guide/dynamic-commands)
- [配置字段参考](/reference/config)
