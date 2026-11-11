# 测试专项报告：功能完整性与 100% 通过率（2026-11-11）

> 范围：Lania CLI v2 全 workspace。结论先行：**`cargo test --workspace` 全绿（含 phase3_e2e 102/102），TS 侧 195/195 全绿**。

## 一、测试结果

| 套件                                 | 结果                   | 备注                                                    |
| ------------------------------------ | ---------------------- | ------------------------------------------------------- |
| `phase3_e2e`（102 个端到端用例）     | **102/102 通过**       | 修复前 83/102；串行约 4 分钟                            |
| `cargo test --workspace`（全部单测） | **全部通过（exit 0）** | 含 runex / bridge / workflows / cli-kit 等              |
| TS 单测（15 个包）                   | **195/195 通过**       | dynamic-commands 61、templates 26、bridge-runtime 26 等 |

建议运行方式：

```powershell
cargo test --workspace                 # 全量单测
cargo test -p lania-cli --test phase3_e2e -- --test-threads=1   # e2e 串行
```

## 二、本轮修复的缺陷（17 项）

### 产品级真实缺陷（9 项）

1. **`lan config` 全挂**：launcher 转发总包二进制时 `argv.slice(1)` 丢掉 `config` token。
2. **总包依赖清单漂移**：`packages/distribution/cli/npm/cli/package.json` 停留在重构前的 cmd-\* 包集，workspace/product 报「未安装」。
3. **静默 mock 假数据**：launcher 向子包注入不存在的 `LANIA_NODE_BRIDGE_DIR` → Rust 侧静默降级 mock transport（`create template` 返回写死的 4 个假模板）。改为 payload 真实存在才注入。
4. **Host RPC src/dist 双实例分裂**：dev 模式下入口跑 src、切片加载 dist，`host-rpc` 状态分裂 → 动态命令 `ctx.tools.*` 永远报 transport not installed。改为 `globalThis` 单例。
5. **bridge 启动双模块图**：`dev_source` 优先导致同一请求被执行多次（实测 4 个 `compiler_start`）。改为优先 dist 入口。
6. **eslint runner 崩溃冒泡**：非 JS 项目的 `lintFiles(['.'])` 抛 `All files matched by '.' are ignored` 使整个 lint.run 失败。改为返回 fallback。
7. **workspace bin 未注册 rust bridge**（B11 契约）：rust 项目 lint/build 无法走 clippy/cargo bridge。启用 `include_rust_bridge=true`。
8. **子包二进制无 stderr sink**：`lan dev` 的 `[app]` 流式日志、`--grouped-output`、Warn 提示全部静默丢失。cli-kit `run_single_command` 统一装配 `StderrLogSink`（与总包对齐）。
9. **dart 错误信息取堆栈尾**（`<asynchronous suspension>`）：改为取第一条非堆栈行。

### 环境与构建健壮性（4 项）

10. **e2e 封闭化环境**（`common.rs` `hermetic_toolchain_env`）：`GOTMPDIR` / `GOCACHE` / `TMP` / `TEMP` / `TMPDIR` / `LANIA_BRIDGE_HOME` 全部收敛到 `target/.cargo-tmp/e2e-env/`，`CI=true` 关闭 dart 遥测，pip --user 安装的 mypy Scripts 自动前置 PATH。根因：部分 Windows 会话的应用控制策略拒绝深层子进程写系统临时目录（go work dir、go-build 缓存、dart telemetry utime 全部 Access Denied）。
11. **go bridge 默认构建命令**加 `-buildvcs=false`（仓库内 fixture 的 VCS stamping 直接失败）。
12. **py bridge worker**：mypy/black/flake8 import 失败显式 `return 2`（原来 exit 1 被误判为「正常返回 0 条诊断」，不回落 spawn）。
13. **清除旧 pack 残留 payload** `packages/distribution/cli/npm/cli/lib/node-bridge`：曾被 bridge 候选路径加载，同一请求被双重执行——「时好时坏」类 flake 的元凶之一。

### 测试契约修复（4 项）

14. env config `locale` 已是合法键（zh|en），更新过时断言；product 裸调用改为 usage 错误 exit 2。
15. rust fixture 的 Cargo.toml 增加空 `[workspace]` 表（e2e 临时目录位于仓库 target 内，cargo 会向上匹配仓库 workspace 导致 clippy 失败）；lint_fix fixture 增加显式 `eslint.config.js`（fixture 会向上解析到仓库真实 eslint）。
16. binary distribution smoke 改为 staged `lan-workspace` + staged bridge payload 的真实安装态链路（总包 exe 本身不承载 known-commands 分发——那是 npm launcher 的职责）。
17. `run_command` 失败时输出真实 stderr；workspace bin 按 `subcommand_spec` 达成 `command.name`（"dev"/"build"）契约，并新增 `run_subcommand_entry` 避免裸调用误回 help。

## 三、遗留问题与注意事项

### 环境相关（非代码缺陷）

| 问题                              | 说明                                                                                                                 | 规避                                   |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------- | -------------------------------------- |
| `git push` 在 bash→cargo 链路失败 | WorkBuddy bash 会话下 msys `sh.exe` 共享对象创建被拒（NtCreateDirectoryObject 0xC0000022），git 派生远端 helper 失败 | 用 PowerShell 原生链路运行，已验证通过 |
| `target/.cargo-tmp` 必须存在      | mingw 自包含链接器的临时目录，缺失时全量链接报错                                                                     | `mkdir target/.cargo-tmp` 即可         |
| e2e 需真实工具链                  | py bridge lint 用例需要 mypy（`pip install mypy`，测试会自动发现 Roaming 下的安装）；go/dart/rust 用例需要对应 SDK   | 测试注释已明示此前提                   |

### 需要产品决策

- **compiler fallback 桩会伪造构建成功**（假资产 `dist/index.js`、`success:true`），与 README「工具链缺失时 fail-fast 不伪造」契约冲突，且现有 e2e 把该行为写成了断言。本次未翻转语义——如需改为 fail-fast，实现与测试需一并调整。

### 工程注意事项

- 修改桥接 assets（`lan_go_bridge.go` / `lan_dart_bridge.dart` / `lan_py_bridge.py`）后，必须 `touch` 对应 crate 的 `src/lib.rs` 并 `cargo build --workspace --bins`：`include_str!` 嵌入不会自动触发依赖 bin 重编，`target/debug` 旁路旧二进制会一直生效。
- bridge 调试日志由 `LANIA_LINT_DEBUG=1` 门控，输出到 `.tmp-smoke/debug-*.log`（lint 切片 / go bridge / dart bridge 三处）。
- `target/.cargo-tmp` 是 mingw 自包含链接器的临时目录，不要删除。

## 四、未测范围

- `lan pi`（按需求排除）。
- `npm install` 后的真实安装布局（npm install layout smoke 已通过，但未覆盖真实 `npm publish → npm install → 全命令` 循环）。
