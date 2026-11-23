# Change: 5860885b / 2026-11-23 / draft

## Why

用户明确要求不做兼容、不打补丁、不要兜底：当前 `call_bridge` 双轨（Hub 优先、失败回落 1:1 NodeBridgeClient）与 `LANIA_BRIDGE_MODE` 开关让 Bridge Hub 沦为echo 级空壳，真实业务流量 100% 走旧链路。要让 Hub 成为唯一桥接路径，所有语言运行时都经它通信，且 dev/build/lint 的事件流与长任务能力不退化。

## Delta

- 新增 `BridgeProvider::call` 的响应+事件流+长任务句柄扩展签名（`host-core/src/lib.rs:28-32`）
- 新增 BusFrame 事件流帧与长任务 shutdown 帧，schema + gen.mjs 生成 5 语言绑定
- 新增 `HubBridgeProvider` 真 spawn 各语言端点 + announce 真实业务能力（当前只有 echo.ping/echo.upper）
- 删除 `dispatch_bridge_request` 回落调用（因 call_bridge 单轨 provider 而消失）
- 删除 `LANIA_BRIDGE_MODE` 开关（因 stdio.ts 默认 Hub 端点而消失）
- 删除 `register_python_bridge` / `register_go_bridge` / `register_dart_bridge` / `register_rust_bridge` 1:1 登记（因各语言端点统一由 Hub attach 而消失）
- 删除 `runStdioBridge` / `maybe_run_rust_bridge` 1:1 短路入口（因 Hub 统一入口而消失）
- 删除 `bridge_api.rs` 中 provider 命中时 events=[], active=None 的降级构造

## Invariants

- 命令子包的 `CommandSpec` 与 handler 注册形态不变（命令树对外契约稳定）
- stdout jsonl envelope 的 `requestId`/`id` 关联语义不变，events 必须先于 response
- `NodeBridgeClient` 保留为 Hub 端点的 stdio 传输底层实例（handshake/配置加载/动态命令/Hook/host RPC/shutdown 仍可用），不得整体删除
- 语言层禁止出现 language/runtime 字段（busInvariant，check-bridge-protocol 保持）
- 错误语义：无 provider 仍回 E_METHOD_NOT_FOUND，只是不再回落

## Affected

- `packages/foundation/host-core/rust/lania-host-core/src/lib.rs`（BridgeProvider trait）
- `packages/distribution/cli/rust/lania-host/src/execution/bridge_api.rs`（call_bridge）
- `packages/distribution/cli/rust/lania-host/src/execution/bridge_helpers/requests.rs`（dispatch_bridge_request）
- `packages/distribution/cli/rust/lania-host/src/execution/bridge_helpers/long_running.rs`
- `packages/distribution/cli/rust/lania-host/src/bridge_setup.rs`
- `packages/distribution/cli/rust/lania-host/src/runtime/{core,bridges,config,host_rpc,lifecycle}.rs`
- `packages/products/bridge-hub/protocol/{schema.json,golden/protocol.json,gen.mjs,gen/*}`
- `packages/products/bridge-hub/rust/lania-bridge-bus/src/{hub.rs,channel.rs}`
- `packages/products/bridge-hub/rust/lania-bridge-host/src/{lib.rs,provider.rs}`
- `packages/products/bridge-hub/adaptors/rust/endpoint.rs`
- `packages/foundation/bridge-runtime/ts/src/entry/{stdio.ts,hub-stdio.ts,envelope.ts}`
- 命令子包：workspace/product/tools/pi（NodeBridgeClient 用法改为 Hub provider）
- 需 conductor 编排影响面核实
- 设计文档：`docs/design/bridge-layer.zh-CN.md`、`docs/design/multi-runtime-bridge.zh-CN.md`、`docs/architecture/模块设计与通信总览.zh-CN.md`、`docs/design/package-split-command-atomization.zh-CN.md`、`docs-site/architecture/bridge-protocol.md`

## Acceptance

- AC-1 cargo test 全绿 + check-bridge-protocol 退出 0
- AC-2 removal_check 对废弃符号零残留
- AC-3 call_bridge 源码无回落分支
- AC-4 stdio_endpoints 跨语言测试覆盖 5 语言端点 announce + request/response + 事件流
- AC-5 dev/build/lint e2e 通过，进度/日志可见、watch shutdown 正常
- AC-6 source_purity.py check --strict rc=0
- AC-7 review_gate Blocker=0 + commitment_ledger check --strict rc=0
- AC-8 所有 Affected 设计文档同步或显式声明无影响

## Open

- U-1 go/dart 真机验证环境缺 staticcheck/flutter，是否阻塞收尾需用户拍板；降级=生成物契约测试覆盖 + 登记环境阻断。未确认前不得标 confirmed。
