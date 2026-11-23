# 文档索引

这份索引用于收敛 `docs/` 目录。当前文档按“阅读入口、稳定架构、专题设计、工程运营、参考资料、历史归档”六类组织。

## Bridge Hub 深度精讲（逐行代码链路）

想深入看「多运行时通信」和「命令从终端到结束的主链路」时，读这一组（每模块一篇、逐行精讲）：

- `architecture/hub-deep-dive/00-总览与学习推荐.zh-CN.md` — 5 个最值得学的点 + 文档索引
- `architecture/hub-deep-dive/01-终端命令主链路.zh-CN.md` — `lan lint` 从 argv 到退出码的完整轨迹
- `architecture/hub-deep-dive/02-多运行时通信机制.zh-CN.md` — BusFrame 协议、三张路由表、host 反向 RPC、事件流
- `architecture/hub-deep-dive/03-多语言接入与惰性attach.zh-CN.md` — 五种语言的接入差异与成本设计
- `architecture/hub-deep-dive/04-请求构造与流式调用.zh-CN.md` — RequestFactory 解耦与 agent 逐 token 流式渲染

## 建议阅读顺序

1. `guides/learning-map.zh-CN.md`
   - 第一次接手仓库时的阅读入口
2. `guides/command-runtime-roadmap.zh-CN.md`
   - 按命令族查看“入口 -> handler -> workflow/bridge/local -> 关键函数”的完整链路
3. `architecture/模块设计与通信总览.zh-CN.md`
   - 逐模块查看职责、边界、协作关系和跨进程通信模型
4. `architecture/框架化与产品化总览.zh-CN.md`
   - 框架定位、作者 API、产品化链路与后续演进方向
5. `design/专题设计与对账.zh-CN.md`
   - `v2.3`、模板、权限、类型契约、Phase 3 工程化增强的收敛说明
6. `operations/成熟度地图.zh-CN.md`
   - 当前哪些能力已经被验证、哪些能力应谨慎使用
7. `operations/known-issues.zh-CN.md`
   - 已知测试失败与遗留问题（当前进度入口）

## 目录结构

### `guides/`

- `guides/learning-map.zh-CN.md`
  - 面向维护者的阅读起点
- `guides/command-runtime-roadmap.zh-CN.md`
  - 面向维护者的命令链路总图和函数追踪入口
- `guides/create-symbol-index.zh-CN.md`
  - `lan create` 的结构体、字段、函数与上下游索引
- `guides/release-symbol-index.zh-CN.md`
  - `lan release` 的结构体、字段、函数与上下游索引

### `architecture/`

- `architecture/hub-deep-dive/` — Bridge Hub 逐行精讲（5 篇，见上方索引）
- `architecture/模块设计与通信总览.zh-CN.md`
  - 各模块职责、上下游依赖、插件系统、Node Bridge、Rust runtime 与通信边界
- `architecture/框架化与产品化总览.zh-CN.md`
  - 框架化定位、产品作者 API、产品构建与发布、复杂 DSL 示例

### `design/`

- `design/agent-architecture.zh-CN.md` — `lan pi` 编码 Agent 的分层与会话模型
- `design/bridge-layer.zh-CN.md` — 语言中立 Bridge Hub（第一阶段已实施，见 §9）
- `design/multi-runtime-bridge.zh-CN.md` — 多运行时 bridge 与 dev/build/create/add/lint 重构
- `design/package-split-command-atomization.zh-CN.md` — 按包拆分与命令原子化（§1 为拆分前历史基线）
- `design/self-managed-runtime-manager.zh-CN.md` — 自管版本管理器 runex
- `design/专题设计与对账.zh-CN.md`
  - 专题级设计决策与实现对账

### `operations/`

- `operations/publish-ci.md`
  - 发布 CI、本地 registry 演练与收尾事项
- `operations/成熟度地图.zh-CN.md`
  - 当前成熟度判断
- `operations/known-issues.zh-CN.md`
  - 已知测试失败、遗留问题与当前进度入口
- `operations/changesets/README.md`
  - 版本变更记录与 release 版本同步流程

### `references/`

- `references/ai/directory-structure.md`
  - 仓库目录职责参考
- `references/examples/author-transaction-workflow.schemas.ts`
  - 作者侧工作流 schema 示例

### `archive/`

- `archive/phase0/core-abstractions.md`
- `archive/phase0/node-bridge-protocol.md`
- `archive/4.2-help-i18n.md`

这部分偏“底层协议冻结”或“阶段性策略记录”，不适合作为第一次阅读入口，但在追溯历史设计边界时仍然有价值。

## 整理原则

- 把“第一次阅读入口”和“专题方案”拆开，减少根目录扁平堆放
- 保留仍可指导维护的总览文档，把历史文档集中到 `archive/`
- 让索引页和实际目录结构一致，避免 README 说明与文件落点脱节
