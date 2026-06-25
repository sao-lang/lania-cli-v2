# Protobuf 到 gRPC 技术方案

这份文档用于收敛当前 `generate module` 中 `protobuf -> grpc` 的后续改造方向，目标是从“最小可用的声明列表 + 简单 DSL 壳子”升级为：

- 支持更完整的 Protobuf 语法
- 面向 `lania-g/cmd/grpc-demo` 风格生成单套 gRPC 代码
- 让生成物更贴近真实业务项目可接入的 Receiver 写法

本文重点回答四个问题：

1. 解析层最终采用什么技术方案
2. 哪些 Protobuf 语法会映射成哪些 gRPC 代码结构
3. 一份覆盖面较全的 `.proto` 输入应长什么样
4. 目标生成出来的 `lania-g` 代码文件应长什么样

## 1. 当前现状

仓库当前已经有一条可运行的 `protobuf -> grpc` 链路，但它仍然属于“面向 demo 的最小子集”：

- Protobuf 解析仍偏向轻量级字符串提取
- 已支持基础 `message` / `service` / `rpc`
- 已支持输出协议声明文件
- 已支持输出 DSL 接线壳子
- 但尚未生成：
  - 完整 DTO
  - `enum`
  - `oneof` helper
  - 流式方法签名
  - 错误映射 helper
  - metadata helper

当前相关实现主要在：

- [module_render.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/module_render.rs)
- [types.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/types.rs)

当前生成物更像“声明和占位符”，还不是“可直接接入真实业务的 gRPC adapter 代码”。

## 2. 目标

目标不是做一个“把 `.proto` 原样翻译成 Go 文件”的官方编译器替代，而是做一条稳定的产品能力链路：

1. 读取完整 Protobuf IDL
2. 做符号解析和语义归一化
3. 识别 `service` / `rpc` / `option` / streaming 信息
4. 生成符合 `lania-g` gRPC adapter 风格的代码

最终默认产物应是一个单文件：

- `generated/lania/adapters/grpc/<entry>_grpc.gen.go`

文件内部包含：

- DTO 类型
- `enum` 对应 Go 类型
- `oneof` 对应 Go 类型与校验 helper
- Receiver 类型
- `Register...Grpc(...)`
- unary / stream 方法桩
- 错误映射 helper
- metadata helper

## 3. 技术方案选择

### 3.1 推荐方案

推荐采用“三层式”结构：

1. `Parser Frontend`
   - 使用成熟 Protobuf 语法前端获取完整语法树
2. `Semantic / IR`
   - 在 Rust 内部做 import、符号表、类型解析和 option 解释
3. `gRPC Codegen`
   - 基于 IR 输出 `lania-g` 风格单文件 gRPC 代码

### 3.2 Parser Frontend 选型

当前建议优先采用完整的 Protobuf parser，而不是继续扩展手写或半手写解析。

原因：

- Protobuf 的完整语法并不只包含 `message/service/rpc`
- `import`、`package`、`enum`、`oneof`、`map`、nested type、`option`、streaming 都需要稳定解析
- gRPC 生成最关键的信息往往在 `service`、`rpc` 和各种 option 上，字符串切分会持续放大维护成本

因此推荐：

- 解析前端：成熟的 Protobuf parser crate 或可嵌入式语法前端
- 语义绑定：仓库内自己实现
- 代码生成：仓库内自己实现

### 3.3 为什么不直接依赖 `protoc`

`protoc` 可以完整处理官方 IDL，但它更适合：

- 直接生成官方语言代码
- 或作为外部校验工具

它不适合作为我们的主生成链路，因为我们需要的不是“官方 gRPC Go 代码”，而是：

- `lania-g` 风格 Receiver
- `Register...Grpc(...)`
- 单文件产物
- 针对 `lania-g` adapter 的接线方式
- 可以按产品需要解释自定义 option
- 可插入错误映射、metadata、helper

因此 `protoc` 可以作为辅助校验工具，但不应是主方案。

## 4. 分层设计

### 4.1 解析层

解析层负责把 `.proto` 文件变成完整 AST，至少覆盖：

- `syntax`
- `package`
- `import`
- `option`
- `message`
- nested `message`
- `enum`
- `oneof`
- `map`
- `repeated`
- `service`
- `rpc`
- client / server / bidi streaming
- field / method / service option

这一层只做语法树，不直接参与 gRPC 代码生成。

### 4.2 语义层

语义层负责：

- import 递归解析
- 构建符号表
- 解析跨文件类型引用
- 展开 nested type
- 收敛 package 与全限定名
- 解析 `oneof`
- 识别 streaming 模式
- 解释 service / method option
- 归一化成内部 IR

IR 需要至少包含：

- `Document`
- `Message`
- `Enum`
- `Oneof`
- `Service`
- `RpcMethod`
- `StreamingMode`
- `GrpcBinding`
- `GrpcMetadata`
- `ErrorBinding`

### 4.3 生成层

生成层只关心 IR，不直接关心 Protobuf 源码文本。

生成目标包括：

- DTO 类型定义
- `enum`
- `oneof` helper
- Receiver
- `Register...Grpc(...)`
- unary / stream 方法桩
- metadata helper
- 错误映射 helper

## 5. Protobuf 语法到 gRPC 代码的映射

### 5.1 文件与类型级

- `package`
  - 作为命名空间和默认 service 名推断输入
- `import`
  - 用于解析跨文件类型引用
- `message`
  - 生成 DTO / request / response
- nested `message`
  - 归一化后生成独立 Go 类型
- `enum`
  - 生成 Go 枚举类型与常量
- `oneof`
  - 生成 Go struct + `ValidateOneof()` helper
- `map<K, V>`
  - 生成 Go `map[K]V`
- `repeated`
  - 生成 Go slice

### 5.2 service 与 rpc 级

- `service`
  - 映射为 Receiver 分组
- `rpc`
  - 映射为 Receiver 方法
- unary
  - 映射为标准 `(args any) (any, error)` 或后续更强类型签名
- `stream Req`
  - 映射为 client stream 方法壳子
- `returns (stream Resp)`
  - 映射为 server stream 方法壳子
- 双向 `stream`
  - 映射为 bidi stream 方法壳子
- method option
  - 作为 metadata、拦截器、超时等扩展位输入

### 5.3 option 级

- 标准 option
  - 先进入语义层存档，不强制全部参与代码生成
- 自定义 option
  - 作为未来 `lania.grpc.*` 元数据扩展入口
- `deprecated`
  - 作为注释和文档元数据输出
- `idempotency` / `timeout` / `auth`
  - 可映射为 helper 或 registration metadata

## 6. 推荐的全面语法示例

为了覆盖后续实现，建议用两份文件做金样例：

### 6.1 `shared.proto`

```proto
syntax = "proto3";

package demo.shared;

message PageRequest {
  int32 page = 1;
  int32 size = 2;
}

enum Gender {
  GENDER_UNKNOWN = 0;
  GENDER_MALE = 1;
  GENDER_FEMALE = 2;
}

message BizError {
  int32 code = 1;
  string message = 2;
}
```

### 6.2 `user.proto`

```proto
syntax = "proto3";

package demo.user;

import "shared.proto";

message UserProfile {
  string nickname = 1;
  string avatar_url = 2;
  repeated string tags = 3;
  map<string, string> ext = 4;
}

message UserContact {
  oneof value {
    string email = 1;
    string mobile = 2;
  }
}

message User {
  string id = 1;
  string username = 2;
  demo.shared.Gender gender = 3;
  UserProfile profile = 4;
  UserContact contact = 5;
}

message GetUserRequest {
  string id = 1;
}

message GetUserResponse {
  int32 code = 1;
  User data = 2;
  string message = 3;
}

message WatchUsersRequest {
  repeated string ids = 1;
}

message UserEvent {
  string event = 1;
  User data = 2;
}

service UserService {
  rpc GetUser(GetUserRequest) returns (GetUserResponse);
  rpc WatchUsers(WatchUsersRequest) returns (stream UserEvent);
  rpc UploadAvatar(stream UserEvent) returns (GetUserResponse);
  rpc Chat(stream UserEvent) returns (stream UserEvent);
}
```

## 7. 目标生成代码形态

下面这份代码不是要求逐字符完全一致，而是目标生成结构的参考形态。

### 7.1 目标文件

- `generated/lania/adapters/grpc/user_grpc.gen.go`

### 7.2 目标结构

```go
package grpc

import (
  "errors"

  grpcadapter "lania-g/v3/adapter/grpc"
)

type Gender int32

const (
  GenderUnknown Gender = 0
  GenderMale    Gender = 1
  GenderFemale  Gender = 2
)

type UserContact struct {
  Email  *string `json:"email,omitempty"`
  Mobile *string `json:"mobile,omitempty"`
}

func (v UserContact) ValidateOneof() error {
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

type UserServiceReceiver struct{}

func RegisterUserGrpc(api *grpcadapter.API, receiver *UserServiceReceiver) {
  if api == nil || receiver == nil {
    return
  }
  api.Service("UserService", receiver).
    Method("GetUser", receiver.GetUser).
    Method("WatchUsers", receiver.WatchUsers).
    Method("UploadAvatar", receiver.UploadAvatar).
    Method("Chat", receiver.Chat).
    Build()
}

func (r *UserServiceReceiver) GetUser(args any) (any, error) {
  _ = args
  return nil, errors.New("TODO")
}

func (r *UserServiceReceiver) WatchUsers(args any) (any, error) {
  _ = args
  return nil, errors.New("TODO")
}
```

## 8. 还可以生成哪些其它代码结构

在默认单文件 gRPC 输出之外，后续还可以扩展以下可选产物：

### 8.1 错误映射文件

- `*_grpc_errors.gen.go`

内容包括：

- 业务错误类型
- gRPC status code 映射
- 错误包装 helper

### 8.2 Metadata 文件

- `*_grpc_metadata.gen.go`

内容包括：

- metadata key 常量
- header / trailer helper
- 认证上下文 helper

### 8.3 Demo 启动文件

- `*_grpc_demo.gen.go`

内容包括：

- 最小可运行示例
- adapter 初始化
- receiver 注册

### 8.4 Client 文件

- `*_grpc_client.gen.go`

内容包括：

- gRPC client 包装
- request/response DTO 复用
- 流式调用 helper

## 9. 里程碑建议

### 里程碑 1：替换解析前端

- 引入完整 Protobuf parser
- 产出完整 AST
- 保留现有最小 IR 兜底

### 里程碑 2：补语义层

- `import`
- 符号表
- nested type
- `enum`
- `oneof`
- streaming
- option

### 里程碑 3：补 gRPC 单文件生成

- DTO / enum / oneof 输出
- Register helper
- unary / stream 方法桩
- 错误与 metadata helper

### 里程碑 4：补测试矩阵

- 多文件 import
- enum / oneof / map / repeated
- unary / stream
- option

## 10. 测试建议

建议至少补四类测试：

1. `Parser`
   - 语法覆盖测试
2. `Semantic`
   - import、全限定名、oneof、streaming
3. `gRPC Render`
   - 生成代码快照
4. `Workflow`
   - `lan generate module` 端到端生成

关键断言应覆盖：

- 单文件 gRPC 输出是否成立
- DTO、enum、oneof 是否正确落盘
- streaming 方法是否稳定生成
- service / method 名是否与 schema 对齐
- import 后的跨文件类型是否正确落到生成代码里

## 11. 当前建议

当前建议的正式推进边界是：

- 先只聚焦 `protobuf -> grpc`
- 一次性补齐与 gRPC 生成直接相关的完整语法支持
- 暂不混入 HTTP / WS / GraphQL 的跨协议抽象收敛

这是当前风险最低、收益最高的推进方式。
