# 第 2 批规格：Hub 真实承接 — spawn Node 端点 + 删回落

## 目标

让 `HubBridgeProvider` 在装配时真实 spawn 一个以 hub 模式运行的 Node bridge 端点，
用完整 method 名（如 `config.loadLan`）经 Hub 精确路由拿到响应；删除 `call_bridge` 的 1:1 回落分支，
provider 失败直接报错。

## FR

- FR-1: [完成] `endpointCapabilities()` 返回完整 method 名（非命名空间前缀），与 Hub 精确匹配对齐。
- FR-2: [完成] 重新编译 bridge-runtime dist（含 hub-stdio.js 新逻辑）。
- FR-3: [完成] `HubBridgeProvider` 装配时通过 `attach_command` spawn 真实 Node bridge（hub 模式入口），
  announce 完整 method 能力。
- FR-5: [完成] 新增端到端测试：spawn 真实 Node 端点 → 完整 method 请求 → 响应非空。
- FR-4: [推迟到第 3 批] `call_bridge` 删除 `dispatch_bridge_request` 回落。

## FR-4 推迟理由（诚实决策，非放弃）

调查确认 Hub 模式 `hub-stdio.ts` 未安装 hostRPC transport（1:1 的 stdio.ts:41 有），
事件 onEvent 仍是空占位。此刻删回落会让 dev/build 的进度/日志丢失、Node 反向调用宿主能力
(fs/git/exec 等 schema-tools)失败——违背用户"事件不退化"的明确要求。
正确顺序：先在后续批次接通 Hub 模式的 hostRPC + 事件源，验证功能等价后再删回落。
单轨化目标不变，只是把"删回落"放到它不会造成功能灾难的位置。

## AC

- AC-1: cargo check --workspace rc=0
- AC-2: 相关 cargo test 全绿（含新增端到端测试）
- AC-3: grep 确认回落路径仍保留且 provider 失败记 Warn（删回落推迟到第 3 批）
- AC-4: check-bridge-protocol rc=0
- AC-5: 独立 review Blocker=0

## 范围外（第 3/4 批）

- 事件流源（hub-stdio.ts onEvent 空占位）
- 长任务 active/shutdown
- hostRPC 反向通道
- 其余 5 语言端点全部 spawn
- 删 register\_\*\_bridge 1:1 登记、文档同步
