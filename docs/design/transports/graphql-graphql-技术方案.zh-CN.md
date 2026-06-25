# GraphQL 到 GraphQL 技术方案

这份文档用于收敛当前 `generate module` 中 `graphql -> graphql` 的后续改造方向，目标是从“最小可用的 root operation 扫描 + 简单 resolver 壳子”升级为：

- 支持更完整的 GraphQL Schema 语法
- 面向 `lania-g/cmd/graphql-demo` 风格生成单套 GraphQL 代码
- 让生成物更贴近真实业务项目可接入的 Resolver 写法

本文重点回答四个问题：

1. 解析层最终采用什么技术方案
2. 哪些 GraphQL 语法会映射成哪些 GraphQL 代码结构
3. 一份覆盖面较全的 `.graphql` 输入应长什么样
4. 目标生成出来的 `lania-g` 代码文件应长什么样

## 1. 当前现状

仓库当前已经有一条可运行的 `graphql -> graphql` 链路，但它仍然属于“面向 demo 的最小子集”：

- GraphQL 解析仍偏向轻量级文本扫描
- 已支持基础 `type Query` / `type Mutation` / `type Subscription`
- 已支持把 root field 提取为 service methods
- 已支持输出协议声明文件
- 已支持输出 DSL 接线壳子
- 但尚未完整支持：
  - `input`
  - `interface`
  - `union`
  - `scalar`
  - `directive`
  - `extend type`
  - 完整参数和 nullability 语义

当前相关实现主要在：

- [graphql.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/schema/graphql.rs)
- [module_render.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/module_render.rs)

这条链路已经足够验证基本方向，但还不适合作为“完整 GraphQL 支持”的长期形态。

## 2. 目标

目标不是做一个“把 GraphQL Schema 原样翻译成 Go 类型”的生成器，而是做一条稳定的产品能力链路：

1. 读取完整 GraphQL Schema
2. 做符号解析和语义归一化
3. 识别 `Query` / `Mutation` / `Subscription`、类型系统和 directive 元数据
4. 生成符合 `lania-g` GraphQL adapter 风格的代码

最终默认产物应是一个单文件：

- `generated/lania/adapters/graphql/<entry>_graphql.gen.go`

文件内部包含：

- DTO 类型
- `input` / `enum` / `scalar` 对应 Go 类型
- union / interface resolve helper
- Resolver 类型
- `Register...Graphql(...)`
- Query / Mutation / Subscription 方法桩
- 必要的 subscription helper

## 3. 技术方案选择

### 3.1 推荐方案

推荐采用“三层式”结构：

1. `Parser Frontend`
   - 使用成熟 GraphQL parser 获取完整 schema AST
2. `Semantic / IR`
   - 在 Rust 内部做类型系统解析、schema merge、directive 解释
3. `GraphQL Codegen`
   - 基于 IR 输出 `lania-g` 风格单文件 GraphQL 代码

### 3.2 Parser Frontend 选型

当前建议优先采用完整 GraphQL parser，而不是继续扩展手写扫描逻辑。

原因：

- GraphQL 的语法重点不只是 root operation
- `input`、`enum`、`interface`、`union`、`scalar`、`directive`、`extend type` 都会影响生成结果
- GraphQL 的 nullability、list 嵌套和参数类型表达非常依赖结构化 AST
- `Subscription` 与普通 `Query` 在运行语义上差异明显，不适合继续靠字符串切分

因此推荐：

- 解析前端：成熟 GraphQL parser crate
- 语义绑定：仓库内自己实现
- 代码生成：仓库内自己实现

### 3.3 为什么不直接依赖外部 GraphQL 代码生成器

外部 GraphQL generator 可以生成 schema glue code，但它不适合作为我们的主生成链路，因为我们需要的不是“通用 GraphQL 服务器脚手架”，而是：

- `lania-g` 风格 Resolver 注册
- 单文件产物
- Query / Mutation / Subscription 的统一接线风格
- 自定义 directive 解释能力
- 与后续产品化代码组织一致的 helper 结构

因此外部 generator 可作为辅助参考，但不应是主方案。

## 4. 分层设计

### 4.1 解析层

解析层负责把 `.graphql` 文件变成完整 AST，至少覆盖：

- `schema`
- `type`
- `input`
- `enum`
- `interface`
- `union`
- `scalar`
- `directive`
- `extend type`
- field arguments
- list / non-null 类型包装

这一层只做语法树，不直接参与 GraphQL 代码生成。

### 4.2 语义层

语义层负责：

- 多文件 schema merge
- 构建符号表
- 解析 root operation
- 解析参数类型与 nullability
- 解析 union / interface
- 解释 directive
- 归一化输入输出类型
- 归一化成内部 IR

IR 需要至少包含：

- `Schema`
- `ObjectType`
- `InputType`
- `EnumType`
- `ScalarType`
- `UnionType`
- `InterfaceType`
- `Resolver`
- `ResolverArg`
- `SubscriptionBinding`
- `DirectiveMeta`

### 4.3 生成层

生成层只关心 IR，不直接关心 GraphQL 源码文本。

生成目标包括：

- DTO 类型定义
- `input`
- `enum`
- `scalar` helper
- union / interface resolve helper
- Resolver
- `Register...Graphql(...)`
- Query / Mutation / Subscription 方法桩
- subscription helper

## 5. GraphQL 语法到 GraphQL 代码的映射

### 5.1 类型系统级

- `type`
  - 生成输出 DTO
- `input`
  - 生成输入 DTO
- `enum`
  - 生成 Go 枚举类型与常量
- `scalar`
  - 生成 alias 或 marshal / unmarshal helper
- `interface`
  - 生成接口结果 helper
- `union`
  - 生成 union resolve helper
- `extend type`
  - 进入 schema merge

### 5.2 root operation 级

- `type Query`
  - 映射为 query resolver
- `type Mutation`
  - 映射为 mutation resolver
- `type Subscription`
  - 映射为 subscription resolver
- field arguments
  - 生成 `Args` 或提升为独立 `Input`
- field return type
  - 决定 resolver 返回值类型

### 5.3 directive 级

- 标准 directive
  - 先进入语义层存档
- 自定义 directive
  - 作为权限、缓存、标签、废弃说明等扩展输入
- `@deprecated`
  - 映射为注释和文档元数据
- `@auth`
  - 可映射为 registration metadata
- `@topic`
  - 可映射为 subscription 事件来源

## 6. 推荐的全面语法示例

为了覆盖后续实现，建议用一份覆盖面较全的文件做金样例：

### 6.1 `user.graphql`

```graphql
schema {
  query: Query
  mutation: Mutation
  subscription: Subscription
}

scalar DateTime

directive @auth(role: String!) on FIELD_DEFINITION
directive @topic(name: String!) on FIELD_DEFINITION

enum UserStatus {
  UNKNOWN
  ENABLED
  DISABLED
}

interface Node {
  id: ID!
}

type UserProfile {
  nickname: String
  avatarUrl: String
  tags: [String!]
}

type User implements Node {
  id: ID!
  username: String!
  status: UserStatus!
  profile: UserProfile
  createdAt: DateTime!
}

type UserError {
  code: Int!
  message: String!
}

union UserResult = User | UserError

input CreateUserInput {
  username: String!
  password: String!
}

input ListUsersInput {
  keyword: String
  page: Int = 1
  size: Int = 20
}

type Query {
  user(id: ID!): UserResult @auth(role: "reader")
  users(input: ListUsersInput): [User!]!
}

type Mutation {
  createUser(input: CreateUserInput!): UserResult @auth(role: "writer")
}

type Subscription {
  userUpdated(id: ID!): User! @topic(name: "user.updated")
}
```

## 7. 目标生成代码形态

下面这份代码不是要求逐字符完全一致，而是目标生成结构的参考形态。

### 7.1 目标文件

- `generated/lania/adapters/graphql/user_graphql.gen.go`

### 7.2 目标结构

```go
package graphql

import (
  "errors"

  graphqladapter "lania-g/v3/adapter/graphql"
)

type UserStatus string

const (
  UserStatusUnknown  UserStatus = "UNKNOWN"
  UserStatusEnabled  UserStatus = "ENABLED"
  UserStatusDisabled UserStatus = "DISABLED"
)

type CreateUserInput struct {
  Username string `json:"username"`
  Password string `json:"password"`
}

type UserResolver struct{}

func RegisterUserGraphql(api *graphqladapter.API, resolver *UserResolver) {
  if api == nil || resolver == nil {
    return
  }
  api.Resolver("UserResolver", resolver).
    Query("user", resolver.User).Returns("UserResult").
    Query("users", resolver.Users).Returns("[]User").
    Mutation("createUser", resolver.CreateUser).Returns("UserResult").
    Subscription("userUpdated", resolver.UserUpdated).Returns("User").
    Build()
}

func (r *UserResolver) User(args any) (any, error) {
  _ = args
  return nil, errors.New("TODO")
}

func (r *UserResolver) CreateUser(args any) (any, error) {
  _ = args
  return nil, errors.New("TODO")
}
```

## 8. 还可以生成哪些其它代码结构

在默认单文件 GraphQL 输出之外，后续还可以扩展以下可选产物：

### 8.1 Schema 元数据文件

- `*_graphql_schema.gen.go`

内容包括：

- type map
- field metadata
- directive metadata

### 8.2 Subscription 桥接文件

- `*_graphql_subscription.gen.go`

内容包括：

- topic 常量
- subscription source helper
- push / broadcast helper

### 8.3 Demo 启动文件

- `*_graphql_demo.gen.go`

内容包括：

- 最小可运行示例
- adapter 初始化
- resolver 注册

### 8.4 Client 文件

- `*_graphql_client.gen.go`

内容包括：

- Query / Mutation / Subscription client
- DTO 复用
- 错误解包 helper

## 9. 里程碑建议

### 里程碑 1：替换解析前端

- 引入完整 GraphQL parser
- 产出完整 AST
- 保留现有最小 root operation IR

### 里程碑 2：补语义层

- schema merge
- 符号表
- `input`
- `enum`
- `union`
- `interface`
- `scalar`
- `directive`

### 里程碑 3：补 GraphQL 单文件生成

- DTO / input / enum / scalar 输出
- union / interface helper
- Resolver / Register helper
- Query / Mutation / Subscription 方法桩

### 里程碑 4：补测试矩阵

- 多文件 schema
- union / interface / scalar / directive
- subscription
- 参数和 nullability

## 10. 测试建议

建议至少补四类测试：

1. `Parser`
   - 语法覆盖测试
2. `Semantic`
   - schema merge、参数类型、union、interface、directive
3. `GraphQL Render`
   - 生成代码快照
4. `Workflow`
   - `lan generate module` 端到端生成

关键断言应覆盖：

- 单文件 GraphQL 输出是否成立
- Query / Mutation / Subscription 是否稳定生成
- `input` / `enum` / `scalar` 是否正确落到生成代码里
- union / interface helper 是否能稳定输出
- 自定义 directive 元数据是否能进入后续生成层

## 11. 当前建议

当前建议的正式推进边界是：

- 先只聚焦 `graphql -> graphql`
- 一次性补齐与 GraphQL 生成直接相关的完整 schema 支持
- 暂不把 GraphQL 语义硬塞进 `http/ws/grpc` 的共用生成策略

这是当前风险最低、收益最高的推进方式。
