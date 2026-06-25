# 文档索引

这份索引用于收敛 `docs/` 目录。当前文档按“阅读入口、稳定架构、专题设计、工程运营、参考资料、历史归档”六类组织。

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
7. `design/transports/`
   - 按协议查看 `generate module` 的专项方案

## 目录结构

### `guides/`

- `guides/learning-map.zh-CN.md`
  - 面向维护者的阅读起点
- `guides/command-runtime-roadmap.zh-CN.md`
  - 面向维护者的命令链路总图和函数追踪入口
- `guides/create-symbol-index.zh-CN.md`
  - `lan create` 的结构体、字段、函数与上下游索引
- `guides/generate-module-symbol-index.zh-CN.md`
  - `lan generate module` 的结构体、字段、函数与上下游索引
- `guides/release-symbol-index.zh-CN.md`
  - `lan release` 的结构体、字段、函数与上下游索引

### `architecture/`

- `architecture/模块设计与通信总览.zh-CN.md`
  - 各模块职责、上下游依赖、插件系统、Node Bridge、Rust runtime 与通信边界
- `architecture/框架化与产品化总览.zh-CN.md`
  - 框架化定位、产品作者 API、产品构建与发布、复杂 DSL 示例

### `design/`

- `design/专题设计与对账.zh-CN.md`
  - 专题级设计决策与实现对账
- `design/transports/thrift-http-rest-技术方案.zh-CN.md`
  - `generate module` 在 `thrift -> http rest` 场景下的后续技术方案
- `design/transports/protobuf-grpc-技术方案.zh-CN.md`
  - `generate module` 在 `protobuf -> grpc` 场景下的后续技术方案
- `design/transports/graphql-graphql-技术方案.zh-CN.md`
  - `generate module` 在 `graphql -> graphql` 场景下的后续技术方案
- `design/transports/thrift-ws-技术方案.zh-CN.md`
  - `generate module` 在 `thrift -> ws` 场景下的后续技术方案

### `operations/`

- `operations/publish-ci.md`
  - 发布 CI、本地 registry 演练与收尾事项
- `operations/成熟度地图.zh-CN.md`
  - 当前成熟度判断
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
