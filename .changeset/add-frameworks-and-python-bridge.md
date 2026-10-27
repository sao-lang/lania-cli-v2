---
'@lania-cli-v2/cli': minor
'@lania-cli-v2/node-bridge': patch
'@lania-cli-v2/templates': patch
---

`add` 扩展到 6 个框架 + Python bridge（设计文档 B7 / B8）。

**B7：`lan add` 覆盖 nestjs / nextjs / gin / fastapi / axum / flutter**

- 每个框架至少有**一个多文件生成器**：
  - `nextjs segment` → `app/<seg>/{page,layout,loading}.tsx`
  - `gin resource` → `internal/{model,repository,service,handler}/*.go`，并写入
    `internal/router/router.go` 的 `// <lania:routes>` 锚点
  - `fastapi resource` → `app/{schemas,models,crud,routers}/*.py`
  - `axum resource` → `src/{model,repository,service,handler}/*.rs`，并写入
    `src/router/mod.rs` 的 `// <lania:mods>` 锚点
  - `flutter feature` → `lib/{models,services,controllers,pages}/*.dart`
- 其余单文件生成器补齐设计矩阵：nextjs（page/layout/route/loading/error/not-found/
  component/hook/middleware/api/action）、gin（handler/model/repository/service/
  middleware/router）、fastapi（router/endpoint/model/schema/crud/dependency/middleware）、
  axum（handler/model/repository/service/middleware/router）、
  flutter（page/widget/controller/model/service/repository）。
- **框架探测**新增依赖声明探测：`go.mod`→gin、`pyproject.toml`→fastapi、
  `Cargo.toml`→axum、`pubspec.yaml`→flutter。只有依赖里真的写了框架名才算命中；
  有 `go.mod` 但不是 gin 时仍然回退到通用生成器，不猜框架。
- **默认目标目录**按框架推导，避免 `internal/internal/…` 这类重复段。
- **`lan add --list`** 新增 `groups: [{framework, generators}]`；`generators` 仍平铺，兼容旧消费者。
- `create` 的 nestjs 模板 `app.module.ts` 加上 `// <lania:imports>`，
  新建的 Nest 项目也能直接 `lan add resource`。
- 生成产物已用真实工具链验证：`go build` / `cargo check` / `dart analyze` /
  `fastapi` + `TestClient` 全部通过（为此 axum 的 repository/service 改为
  `Clone` + `Arc<RwLock<_>>`，才能作为 axum `State`）。

**B8：Python bridge（`lan-py-bridge`）**

- 新增 crate `rust/crates/lania-py-bridge`：单文件 Python 脚本（`include_str!` 内嵌，
  首次运行落盘到 `LANIA_BRIDGE_HOME`，内容变化才重写），说与 node-bridge 完全相同的
  jsonl 协议，因此复用 `lania_node_bridge::NodeBridgeClient`。
- **`lint.mypy` 走真 API**：每个请求起一个干净解释器跑 `mypy.api.run()`，解析留在主进程，
  保证 API 路径与 spawn 路径产出**同一份** `Diagnostic`。
  `ruff` / `black` / `flake8` 同一套接口，`ruff` 因无 Python API 退化为 spawn。
- **Python 项目的 `lan lint` 改走该 bridge**：检测 `pyproject.toml` / `setup.py` /
  `requirements*.txt` / 裸 `.py` 文件；结果聚合成与 `lint.run` 相同的形状
  （`diagnostics` / `counts` / `resultsByAdaptor` / `exitCode`），渲染层无需分支。
- **回落**：没有 python 解释器或 bridge 调用失败时，`lint` 继续走 JS 簇 bridge 的
  spawn 适配器，「移除 bridge 后仍可用」（`LANIA_PYTHON` 可覆盖解释器路径）。
- 新增 `CommandExecution::RuntimeBridge` 变体承载独立运行时 bridge 的结果。
- 顺带修掉 lint 报告里规则号超过 10 字符时与消息粘连（`assignmentIncompatible …`）的问题。
