# 环境变量参考

<div class="lead">
本页列出 CLI 运行时、Bridge、执行策略、日志、交互与产品打包会读取的环境变量。未列出的变量不会改变宿主行为。
</div>

## 启动与运行时

| 变量                    | 作用                        | 默认           |
| ----------------------- | --------------------------- | -------------- |
| `LANIA_NODE_BRIDGE_DIR` | 强制 Node Bridge 目录       | 自动解析       |
| `LANIA_NODE_BIN`        | 指定 Node 可执行文件        | `node`         |
| `LANIA_PRODUCT_ROOT`    | 产品运行时根目录            | 当前项目       |
| `LANIA_RUNTIME_MODE`    | `development` / `installed` | 由启动方式决定 |

这些变量通常由 `npm/cli/bin/lan.mjs` 或产品 wrapper 注入，手工调试时可以覆盖。

```powershell
$env:LANIA_NODE_BIN = "C:\Program Files\nodejs\node.exe"
$env:LANIA_NODE_BRIDGE_DIR = "E:\repo\ts\packages\node-bridge"
```

## 执行策略

| 变量                       | 作用                           | 默认    |
| -------------------------- | ------------------------------ | ------- |
| `LANIA_TIMEOUT_MS`         | 单次 Bridge 调用超时           | `30000` |
| `LANIA_RETRY_ATTEMPTS`     | Bridge 调用失败后的重试次数    | `0`     |
| `LANIA_INTERRUPT_AFTER_MS` | 自动模拟中断，便于测试取消路径 | 未启用  |

`LANIA_TIMEOUT_MS` 会覆盖 `ExecutionPolicy.timeout_ms`，进而影响 Bridge 请求和命令超时。

## 日志

| 变量                                | 作用                                                          |
| ----------------------------------- | ------------------------------------------------------------- |
| `LANIA_LOG_LEVEL`                   | `trace` / `debug` / `info` / `warn` / `error`                 |
| `LANIA_TRACE=1`                     | 旧快捷开关，等价于 trace；仅在未设置 `LANIA_LOG_LEVEL` 时生效 |
| `LANIA_DEBUG=1`                     | 旧快捷开关，等价于 debug；仅在未设置 `LANIA_LOG_LEVEL` 时生效 |
| `LANIA_BRIDGE_PASSTHROUGH_STDERR=1` | 把 Node stderr 转成 `event.log`                               |

```powershell
$env:LANIA_LOG_LEVEL = "debug"
lan build
```

## 交互与 CI

| 变量                        | 作用                  |
| --------------------------- | --------------------- |
| `LANIA_PROMPT_ANSWERS_JSON` | 预置 prompt 答案 JSON |
| `HOME`                      | 全局偏好文件根目录    |

```powershell
$env:LANIA_PROMPT_ANSWERS_JSON = '{"env":"staging","confirm":true}'
```

全局偏好路径是 `$HOME/.lania/preferences.json`。Windows 上如果没有设置 `HOME`，偏好读写会退化为默认值。

## 产品打包与发布

以下变量用于 `lan product pack` / `lan product publish` 或产品 wrapper 的调试。

| 变量                                  | 作用                          |
| ------------------------------------- | ----------------------------- |
| `LANIA_PRODUCT_HOST_BINARY`           | 产品 wrapper 使用的宿主二进制 |
| `LANIA_HOST_BINARY`                   | 上者的旧名/兜底               |
| `LANIA_PRODUCT_HOST_BINARY_SOURCE`    | 显式指定宿主二进制来源        |
| `LANIA_PRODUCT_BINARY_SOURCES_DIR`    | 平台二进制来源根目录          |
| `LANIA_PRODUCT_PLATFORM_BINARIES_DIR` | 同上，产品语义别名            |
| `LANIA_CLI_PLATFORM_BINARIES_DIR`     | 同上，CLI 语义别名            |

平台专属二进制变量按平台名大写、非字母数字转下划线：

```text
linux-x64      → LANIA_PRODUCT_BINARY_SOURCE_LINUX_X64
darwin-arm64   → LANIA_PRODUCT_BINARY_SOURCE_DARWIN_ARM64
win32-x64      → LANIA_PRODUCT_BINARY_SOURCE_WIN32_X64
```

同时兼容：

```text
LANIA_PRODUCT_<SUFFIX>_BINARY_SOURCE
LANIA_CLI_<SUFFIX>_BINARY_SOURCE
```

例如：

```powershell
$env:LANIA_PRODUCT_BINARY_SOURCE_WIN32_X64 = "E:\artifacts\lania-cli.exe"
lan product publish --execute --dry-run --yes
```

## 构建与发布脚本

这些变量只被仓库脚本读取，不属于 CLI 二进制：

| 变量                                      | 作用               |
| ----------------------------------------- | ------------------ |
| `LANIA_NPM_REGISTRY`                      | 覆盖 npm registry  |
| `LANIA_NPM_SCOPE`                         | 覆盖 npm scope     |
| `LANIA_CLI_BINARY_RELEASE_REPO`           | 平台二进制发布仓库 |
| `LANIA_CLI_BINARY_RELEASE_TAG_TEMPLATE`   | 发布 tag 模板      |
| `LANIA_CLI_BINARY_RELEASE_ASSET_TEMPLATE` | 发布资产名模板     |
| `LANIA_GITHUB_TOKEN` / `GITHUB_TOKEN`     | 拉取平台二进制     |

## 下一步

- [Bridge 通信协议](/architecture/bridge-protocol)
- [配置字段参考](/reference/config)
- [输出与状态文件](/reference/output-and-state)
