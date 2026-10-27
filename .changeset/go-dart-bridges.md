---
'@lania-cli-v2/cli': minor
---

`lan lint` / `lan build` 按运行时路由到 Go / Dart bridge（设计文档 B9 / B10）。

**B10：Go bridge（`lan-go-bridge`）**

- 新增 crate `rust/crates/lania-go-bridge`：内嵌单文件 Go 源码（只用标准库），
  说与 node-bridge 完全相同的 jsonl 协议，因此复用 `lania_node_bridge::NodeBridgeClient`。
  优先使用 `LANIA_GO_BRIDGE` / 与 CLI 同目录的预编译二进制；都没有时用本机 `go build`
  编一次并缓存（缓存键 = 源码内容 + crate 版本），**不新增 npm 平台包，也不依赖 CGO**。
- **Go 项目的 `lan lint` 改走该 bridge**：`go vet` 与 `golangci-lint` 由 bridge 内 exec，
  `file.go:line:col: msg` / `--out-format json` 两种输出都归一化成 §7.3 的 `Diagnostic`
  （`tool` / `id` / `severity` / `file` / `range` / `fix` 全字段）。
- **Go 项目的 `lan build` 去掉 Node 中转**：进程链从
  `Rust → node → compiler-worker → go build` 降到 `Rust → go bridge → go build`，
  编译器 stdout/stderr 逐行走 `event.diag`（`channel: "build"`）。

**B9：Dart bridge（`lan-dart-bridge`）**

- 新增 crate `rust/crates/lania-dart-bridge`：内嵌单文件 Dart 脚本（只用 `dart:io`），
  `dart run <script>` 直接解释执行 —— 与 `lan-py-bridge` 同构、零平台包，
  从而绕开「`dart compile exe` 无法交叉编译」的成本。
- **Dart 项目的 `lan lint` / `lan build` 都改走该 bridge**：
  `dart analyze --format machine` 的 8 字段输出归一化为 `Diagnostic`；
  `dart compile exe` 由 bridge 内 exec（子进程一律用 `Platform.resolvedExecutable`，
  保证与 bridge 同一个 SDK）。Dart 项目进程层数同样从 3 降到 2。

**共同部分**

- **运行时判定收口**：新增 `lania_host::detect_runtime`（`--runtime` > `lan.config.runtime` >
  特征文件 > node），`lint` / `build` 不再各自数特征文件；非 JS 强信号
  （`pyproject.toml` / `go.mod` / `Cargo.toml` / `pubspec.yaml`）排在 `package.json` 之前。
- **`build` 的运行时分流补齐**（含 Python）：构建命令来自
  `lan.config.runtimes.<id>.build` 或 `toolchain.describe`（单一权威来源），
  `{output}` 由 Rust 替换；`build --watch` 与 `dev` 仍由 compiler-worker 承担。
- **懒加载登记**：`BridgeRegistry::register_deferred` 让 bridge 的「定位/编译」推迟到首次
  真正需要时，`lan env` / `lan add` 之类命令零额外开销；解析失败照样回落到 JS 簇路径
  （`LANIA_GO` / `LANIA_DART` 可覆盖工具链路径，e2e 用「指向不存在的路径」锁住回落）。
- 新增 crate `rust/crates/lania-bridge-support`：三个语言 bridge 共用
  「资产落盘 + Windows `PATHEXT` 可执行文件定位 + `.bat` 包装」的实现。
