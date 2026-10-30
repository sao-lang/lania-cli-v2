---
'@lania-cli-v2/cli': minor
---

`runex` 从「独立 CLI」提升为「独立产品 + Rust API」：

- `runex` 改为 **lib + bin**：新增 `runex::api::Runex` 稳定 Rust API，覆盖 install / use / remove / list / info / versions / which / doctor / init / plan / apply。
- `runex` 新增 `ui` 子命令；TUI 从 `lania-plugins-command-env` 迁入 `runex::tui`，`lan env ui` 改为薄委托。
- 内部引擎 `lania-runtime-manager` 重命名为 **`runex-core`**；`cargo tree -p runex` 不再出现任何 `lania-*` crate。
- `lan env` 不再直接依赖内部引擎，所有运行时管理动作改为调用 `runex` API；命令层只保留 lania 宿主注册、`--yes` / dry-run 门控与输出模板。
- `lania-cli` 的 `activate_managed_runtimes` 也改为通过 `runex` 调用。
- 新增 `runex` API 测试与 `runex ui --snapshot` 进程级测试。
