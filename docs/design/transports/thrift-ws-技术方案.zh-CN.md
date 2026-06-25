# Thrift 到 WS 技术方案

这份文档用于收敛当前 `generate module` 中 `thrift -> ws` 的后续改造方向，目标是从“最小可用的事件声明列表 + 简单 Gateway 壳子”升级为：

- 支持更完整的 Thrift IDL 语法
- 面向 `lania-g/cmd/ws-demo` 风格生成单套 WS 代码
- 让生成物更贴近真实业务项目可接入的 Gateway 写法

本文重点回答四个问题：

1. 解析层最终采用什么技术方案
2. 哪些 Thrift 语法会映射成哪些 WS 代码结构
3. 一份覆盖面较全的 Thrift 输入应长什么样
4. 目标生成出来的 `lania-g` 代码文件应长什么样

## 1. 当前现状

仓库当前已经有一条可运行的 `ws` 链路，但它主要基于 YAML Schema + `x-lania-operations`，仍然属于“面向 demo 的最小子集”：

- WS 生成已支持基础事件声明和 Gateway DSL 接线
- 但当前主输入还不是 Thrift
- 还没有把 Thrift 的 `struct/union/exception/service` 真正映射到 WS 生成链路
- 也还没有形成“面向真实业务”的单文件 WS 代码形态

当前相关实现主要在：

- [module_render.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/module_render.rs)
- [thrift.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/schema/thrift.rs)
- [types.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/types.rs)

这意味着 `thrift -> ws` 更适合被设计成一条新的正式产品能力，而不是在现有 YAML WS demo 基础上继续堆补丁。

## 2. 目标

目标不是做一个“把 Thrift 原样翻译成消息 DTO”的编译器，而是做一条稳定的产品能力链路：

1. 读取完整 Thrift IDL
2. 做符号解析和语义归一化
3. 识别 `ws.*` 注解
4. 生成符合 `lania-g` WS adapter 风格的代码

最终默认产物应是一个单文件：

- `generated/lania/adapters/ws/<entry>_ws.gen.go`

文件内部包含：

- DTO 类型
- `typedef` / `enum` / `exception` 对应 Go 类型
- Gateway
- `Register...Ws(...)`
- 事件处理方法桩
- 错误帧 helper
- ack helper
- 必要的 union 校验 helper

## 3. 技术方案选择

### 3.1 推荐方案

推荐采用“三层式”结构：

1. `Parser Frontend`
   - 使用成熟 Thrift 语法前端获取完整语法树
2. `Semantic / IR`
   - 在 Rust 内部做 include、符号表、类型解析和 `ws.*` 注解解释
3. `WS Codegen`
   - 基于 IR 输出 `lania-g` 风格单文件 WS 代码

### 3.2 与 `thrift -> http rest` 的关系

`thrift -> ws` 和 `thrift -> http rest` 应共享：

- Thrift parser frontend
- include 递归解析
- 符号表
- `typedef` / `enum` / `union` / `exception` / `service extends` 语义归一化

但两者不应共享协议注解和生成逻辑：

- HTTP 使用 `api.*`
- WS 使用 `ws.*`
- HTTP 关注 route / body / query / path
- WS 关注 namespace / event / direction / ack / broadcast / room

因此推荐：

- 共享解析层和部分语义层
- 分离协议绑定 IR
- 分离 codegen

### 3.3 为什么不继续用 YAML 作为 WS 主输入

YAML 非常适合 demo 和轻量声明，但它不适合作为 `thrift -> ws` 主方案的长期主输入，因为我们希望：

- 统一类型契约表达
- 复用已有 Thrift 语义能力
- 支持 `include`、跨文件类型、`union`、`exception`
- 把事件、命令、错误帧都建立在更强的 IDL 之上

因此 YAML 可以保留为简化模式，但不应替代 `thrift -> ws` 正式方案。

## 4. 分层设计

### 4.1 解析层

解析层负责把 `.thrift` 文件变成完整 AST，至少覆盖：

- `namespace`
- `include`
- `typedef`
- `const`
- `enum`
- `struct`
- `union`
- `exception`
- `service`
- `service extends`
- `throws`
- `oneway`
- `list/set/map`
- field default
- annotations

这一层只做语法树，不直接参与 WS 代码生成。

### 4.2 语义层

语义层负责：

- include 递归解析
- 构建符号表
- 解析跨文件类型引用
- 展开 `typedef`
- 收敛 `service extends`
- 收集 `throws`
- 解释 `ws.*` 注解
- 归一化成内部 IR

IR 需要至少包含：

- `Document`
- `TypeDef`
- `Enum`
- `Struct`
- `Union`
- `Exception`
- `Service`
- `Method`
- `Field`
- `WsBinding`
- `WsNamespace`
- `WsEvent`
- `AckBinding`
- `Throws`

### 4.3 生成层

生成层只关心 IR，不直接关心 Thrift 源码文本。

生成目标包括：

- DTO 类型定义
- `typedef` / `enum` / `exception`
- Gateway
- `Register...Ws(...)`
- 事件处理方法桩
- 错误帧 helper
- ack helper
- union 校验 helper

## 5. Thrift 语法到 WS 代码的映射

### 5.1 文件与类型级

- `namespace go xxx`
  - 作为未来 package/import 组织的输入
- `include`
  - 用于解析跨文件类型与常量引用
- `typedef`
  - 生成 Go alias
- `const`
  - 生成事件名、namespace、topic 等 Go const
- `enum`
  - 生成 Go 枚举类型与常量
- `struct`
  - 生成消息 DTO
- `union`
  - 生成 DTO + `ValidateUnion()` helper
- `exception`
  - 生成错误帧类型

### 5.2 service 与 method 级

- `service`
  - 映射为 Gateway 分组
- `service extends`
  - 父 service 的事件与命令合并到当前 Gateway
- method
  - 映射为事件处理方法或 command handler
- `throws`
  - 生成错误帧映射 helper
- `oneway`
  - 映射为无需 ack 的单向事件
- 普通 method
  - 映射为 request / ack 风格处理器

### 5.3 annotation 级

建议使用独立的 `ws.*` 注解，而不是复用 `api.*`：

- `ws.namespace`
  - 决定 Gateway namespace
- `ws.event`
  - 决定事件名
- `ws.direction`
  - 决定是 `client`、`server` 或 `bidirectional`
- `ws.room`
  - 决定 room key
- `ws.broadcast`
  - 标记为广播事件
- `ws.ack`
  - 标记该方法需要 ack
- `ws.timeout`
  - 指定 ack 超时元数据
- `ws.auth`
  - 作为认证策略输入

## 6. 推荐的全面语法示例

为了覆盖后续实现，建议用两份文件做金样例：

### 6.1 `shared.thrift`

```thrift
namespace go demo.shared

typedef string TraceID

enum EventSource {
  UNKNOWN = 0,
  CLIENT = 1,
  SERVER = 2
}

exception WsBizException {
  1: required i32 code
  2: required string message
}
```

### 6.2 `user_ws.thrift`

```thrift
namespace go demo.user

include "shared.thrift"

typedef string UserID

const string USER_NAMESPACE = "/ws/user"

struct UserProfile {
  1: optional string nickname
  2: optional string avatar_url
}

union UserContact {
  1: string email
  2: string mobile
}

struct User {
  1: required UserID id
  2: required string username
  3: optional UserProfile profile
  4: optional UserContact contact
}

struct UserJoinedEvent {
  1: required User data
  2: optional shared.TraceID trace_id
}

struct JoinRoomRequest {
  1: required UserID id
  2: required string room_id
}

struct JoinRoomAck {
  1: required bool ok
  2: optional string message
}

exception UserNotFound {
  1: required UserID id
  2: required string message
}

service PresenceGateway {
  oneway void UserJoined(1: UserJoinedEvent event) (
    ws.namespace = USER_NAMESPACE,
    ws.event = "user.joined",
    ws.direction = "server",
    ws.broadcast = "true"
  )

  JoinRoomAck JoinRoom(1: JoinRoomRequest req) throws (
    1: UserNotFound not_found,
    2: shared.WsBizException biz
  ) (
    ws.namespace = "/ws/user",
    ws.event = "user.join-room",
    ws.direction = "client",
    ws.ack = "true",
    ws.room = "room_id",
    ws.auth = "user"
  )
}
```

## 7. 目标生成代码形态

下面这份代码不是要求逐字符完全一致，而是目标生成结构的参考形态。

### 7.1 目标文件

- `generated/lania/adapters/ws/user_ws.gen.go`

### 7.2 目标结构

```go
package ws

import (
  "errors"

  wsadapter "lania-g/v3/adapter/ws"
)

type UserID = string

const (
  UserNamespace = "/ws/user"
  UserJoinedEventName = "user.joined"
  UserJoinRoomEventName = "user.join-room"
)

type UserContact struct {
  Email  *string `json:"email,omitempty"`
  Mobile *string `json:"mobile,omitempty"`
}

func (v UserContact) ValidateUnion() error {
  count := 0
  if v.Email != nil {
    count++
  }
  if v.Mobile != nil {
    count++
  }
  if count > 1 {
    return errors.New("UserContact allows only one active field")
  }
  return nil
}

type PresenceGateway struct{}

func RegisterUserWs(api *wsadapter.API, gateway *PresenceGateway) {
  if api == nil || gateway == nil {
    return
  }
  api.Gateway("/ws/user", gateway).
    On("user.joined", gateway.UserJoined).
    On("user.join-room", gateway.JoinRoom).
    Build()
}

func (g *PresenceGateway) UserJoined(args any) (any, error) {
  _ = args
  return nil, errors.New("TODO")
}

func (g *PresenceGateway) JoinRoom(args any) (any, error) {
  _ = args
  return nil, errors.New("TODO")
}
```

## 8. 还可以生成哪些其它代码结构

在默认单文件 WS 输出之外，后续还可以扩展以下可选产物：

### 8.1 错误帧文件

- `*_ws_errors.gen.go`

内容包括：

- `exception` 类型
- `throws` 到错误帧映射
- 错误包装 helper

### 8.2 Ack 文件

- `*_ws_ack.gen.go`

内容包括：

- ack DTO
- ack timeout helper
- 成功 / 失败 ack helper

### 8.3 Demo 启动文件

- `*_ws_demo.gen.go`

内容包括：

- 最小可运行示例
- adapter 初始化
- gateway 注册

### 8.4 Client 文件

- `*_ws_client.gen.go`

内容包括：

- emit / on client
- request / ack helper
- 错误解包 helper

## 9. 里程碑建议

### 里程碑 1：复用并补齐 Thrift 解析前端

- 复用 `thrift -> http rest` 的完整 parser frontend
- 产出完整 AST
- 保留现有最小 WS DSL 生成功能兜底

### 里程碑 2：补 WS 语义层

- `include`
- 符号表
- `typedef`
- `enum`
- `union`
- `exception`
- `throws`
- `service extends`
- `ws.*` 注解

### 里程碑 3：补 WS 单文件生成

- alias / const / enum / exception 输出
- union 校验 helper
- Gateway / Register helper
- 单向事件与 ack 事件方法桩

### 里程碑 4：补测试矩阵

- 多文件 include
- enum / union / exception
- throws / ack
- extends
- annotation 组合

## 10. 测试建议

建议至少补四类测试：

1. `Parser`
   - 语法覆盖测试
2. `Semantic`
   - include、extends、throws、注解解释
3. `WS Render`
   - 生成代码快照
4. `Workflow`
   - `lan generate module` 端到端生成

关键断言应覆盖：

- 单文件 WS 输出是否成立
- namespace / event 是否稳定生成
- `oneway` 与 `ack` 模式切换是否正确
- `throws` 是否能稳定生成错误帧骨架
- `include` 后的跨文件类型是否正确落到生成代码里

## 11. 当前建议

当前建议的正式推进边界是：

- 先只聚焦 `thrift -> ws`
- 一次性补齐与 WS 生成直接相关的完整 Thrift 语法支持
- 保留 YAML WS 作为轻量模式，但不作为正式主链路

这是当前风险最低、收益最高的推进方式。
