# Thrift 到 HTTP Rest 技术方案

这份文档用于收敛当前 `generate module` 中 `thrift -> http rest` 的后续改造方向，目标是从“最小可用的手写解析 + 简单代码生成”升级为：

- 支持更完整的 Thrift IDL 语法
- 面向 `lania-g/cmd/http-demo` 风格生成单套 HTTP 代码
- 让生成物更贴近真实业务项目可接入的 Controller 写法

本文重点回答四个问题：

1. 解析层最终采用什么技术方案
2. 哪些 Thrift 语法会映射成哪些 HTTP 代码结构
3. 一份覆盖面较全的 Thrift 输入应长什么样
4. 目标生成出来的 `lania-g` 代码文件应长什么样

## 1. 当前现状

仓库当前已经有一条可运行的 `thrift -> http rest` 链路，但它仍然属于“面向 demo 的最小子集”：

- Thrift 解析仍是手写 parser
- 已支持基础 `struct` / `service` / 简单字段注解
- 已支持单文件 HTTP 输出
- 已支持生成：
  - DTO
  - `Args`
  - `Request`
  - `Controller`
  - `Register...Http(...)`
  - 方法桩

当前相关实现主要在：

- [thrift.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/schema/thrift.rs)
- [module_render.rs](file:///Users/bytedance/Desktop/files/self/lania-zip/lania-cli-v2/rust/crates/lania-workflows/src/generate/module_render.rs)

这条链路已经适合验证基本方向，但不适合作为“完整 Thrift 支持”的长期形态。

## 2. 目标

目标不是做一个“把 Thrift 原样翻译成 Go 结构体”的编译器，而是做一条稳定的产品能力链路：

1. 读取完整 Thrift IDL
2. 做符号解析和语义归一化
3. 识别 `api.*` 注解
4. 生成符合 `lania-g` HTTP 风格的代码

最终默认产物应是一个单文件：

- `generated/lania/adapters/http/<entry>_http.gen.go`

文件内部包含：

- DTO 类型
- `typedef` / `enum` / `exception` 对应 Go 类型
- `Args` / `Request`
- `Controller`
- `Register...Http(...)`
- 方法桩
- 错误映射 helper
- 必要的 union 校验 helper

## 3. 技术方案选择

### 3.1 推荐方案

推荐采用“三层式”结构：

1. `Parser Frontend`
   - 使用成熟 Thrift 语法前端获取完整语法树
2. `Semantic / IR`
   - 在 Rust 内部做 include、符号表、类型解析和注解解释
3. `HTTP Codegen`
   - 基于 IR 输出 `lania-g` 风格单文件 HTTP 代码

### 3.2 Parser Frontend 选型

当前建议优先采用基于 `tree-sitter` 的 Thrift grammar，而不是继续扩展手写 parser。

原因：

- Thrift 完整语法面较大
- `include`、`typedef`、`enum`、`union`、`exception`、`throws`、`extends` 不适合继续靠字符串切分
- `tree-sitter` 适合作为嵌入式语法前端，不要求引入外部二进制工具链

因此推荐：

- 解析前端：`tree-sitter-thrift` 或同类 grammar crate
- 语义绑定：仓库内自己实现
- 代码生成：仓库内自己实现

### 3.3 为什么不直接依赖外部 `thrift` 编译器

外部 `thrift` 编译器当然可以完整处理官方 IDL，但它更适合：

- 直接生成目标语言代码
- 或作为外部校验工具

它不适合作为我们的主生成链路，因为我们需要的不是“官方 Go 代码”，而是：

- `lania-g` 风格 `Controller`
- `httpbinding.Context`
- `Args / Request`
- `ShouldBindJSON`
- 自定义 `api.*` 注解解释
- `exception -> HTTP` 映射策略

因此外部编译器可作为辅助，但不应是主方案。

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

这一层只做语法树，不直接参与 HTTP 代码生成。

### 4.2 语义层

语义层负责：

- include 递归解析
- 构建符号表
- 解析跨文件类型引用
- 展开 `typedef`
- 收敛 `service extends`
- 收集 `throws`
- 解释 `api.*` 注解
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
- `HttpBinding`
- `HttpRoute`
- `Throws`

### 4.3 生成层

生成层只关心 IR，不直接关心 Thrift 源码文本。

生成目标包括：

- DTO 类型定义
- `Args` 类型
- `Request` 类型
- `Controller`
- `Register...Http(...)`
- 方法桩
- `exception -> status` helper
- union 校验 helper

## 5. Thrift 语法到 HTTP 代码的映射

### 5.1 文件与类型级

- `namespace go xxx`
  - 作为未来 package/import 组织的输入
  - 当前阶段先不直接决定输出包名，默认仍生成 `package http`
- `include`
  - 用于解析跨文件类型与常量引用
- `typedef`
  - 生成 Go alias
- `const`
  - 生成 Go const
- `enum`
  - 生成 Go 枚举类型与常量
- `struct`
  - 生成 DTO、request、response
- `union`
  - 生成 DTO + `ValidateUnion()` helper
- `exception`
  - 生成 error 类型

### 5.2 service 与 method 级

- `service`
  - 映射为 controller 分组
- `service extends`
  - 父 service 方法合并到当前 controller
- method
  - 映射为 handler 方法
- `throws`
  - 生成错误类型映射与状态码 helper
- `oneway`
  - 映射为 `202 Accepted` 风格 handler

### 5.3 field 与 annotation 级

- `required`
  - 进入 `required:"true"` 或 `validate:"required"`
- `optional`
  - 映射为非强制字段
- `api.body`
  - 生成 body 绑定或 JSON request DTO
- `api.query`
  - 生成 `httpbinding.Query[...]`
- `api.path` / `api.param`
  - 生成 `httpbinding.Param[...]`
- `api.header`
  - 生成 `httpbinding.Header[...]`
- `api.form`
  - 生成 `form` 绑定
- `api.get/post/put/delete/patch/...`
  - 决定 HTTP method
- `api.handler_path`
  - 决定 group path 与 controller 命名
- `api.category`
  - 作为 group/tag fallback

## 6. 推荐的全面语法示例

为了覆盖后续实现，建议用两份文件做金样例：

### 6.1 `shared.thrift`

```thrift
namespace go demo.shared

typedef string TraceID

const i32 DEFAULT_PAGE_SIZE = 20

enum Gender {
  UNKNOWN = 0,
  MALE = 1,
  FEMALE = 2
}

struct PageQuery {
  1: optional i32 page = 1 (api.query = "page")
  2: optional i32 size = DEFAULT_PAGE_SIZE (api.query = "size")
}

exception BizException {
  1: required i32 code
  2: required string message
}
```

### 6.2 `user_http.thrift`

```thrift
namespace go demo.user

include "shared.thrift"

typedef string UserID

const string USERS_CATEGORY = "users"

enum UserStatus {
  UNKNOWN = 0,
  ENABLED = 1,
  DISABLED = 2,
  LOCKED = 3
}

struct UserProfile {
  1: optional string nickname (api.body = "nickname")
  2: optional string avatar_url (api.body = "avatarUrl")
  3: optional list<string> tags (api.body = "tags")
  4: optional map<string, string> ext (api.body = "ext")
}

union UserContact {
  1: string email
  2: string mobile
}

struct User {
  1: required UserID id (api.body = "id")
  2: required string username (api.body = "username")
  3: optional shared.Gender gender (api.body = "gender")
  4: optional UserStatus status (api.body = "status")
  5: optional UserProfile profile (api.body = "profile")
  6: optional UserContact contact (api.body = "contact")
}

struct CreateUserRequest {
  1: required string username (api.body = "username,required")
  2: required string password (api.body = "password,required,min=6")
  3: optional shared.Gender gender (api.body = "gender")
  4: optional UserProfile profile (api.body = "profile")
  5: optional UserContact contact (api.body = "contact")
}

struct CreateUserResponse {
  1: required i32 code (api.body = "code")
  2: optional User data (api.body = "data")
  3: required string message (api.body = "msg")
}

struct GetUserRequest {
  1: required UserID id (api.path = "id")
  2: optional shared.TraceID trace_id (api.header = "X-Trace-Id")
}

struct GetUserResponse {
  1: required i32 code (api.body = "code")
  2: optional User data (api.body = "data")
  3: required string message (api.body = "msg")
}

struct ListUsersRequest {
  1: optional string keyword (api.query = "keyword")
  2: optional shared.Gender gender (api.query = "gender")
  3: optional UserStatus status (api.query = "status")
  4: optional i32 page = 1 (api.query = "page")
  5: optional i32 size = 20 (api.query = "size")
  6: optional shared.TraceID trace_id (api.header = "X-Trace-Id")
}

struct ListUsersResponse {
  1: required i32 code (api.body = "code")
  2: optional list<User> data (api.body = "data")
  3: optional i32 total (api.body = "total")
  4: required string message (api.body = "msg")
}

struct UpdateUserRequest {
  1: required UserID id (api.path = "id")
  2: optional UserStatus status (api.body = "status")
  3: optional UserProfile profile (api.body = "profile")
  4: optional UserContact contact (api.body = "contact")
}

struct UpdateUserResponse {
  1: required i32 code (api.body = "code")
  2: optional User data (api.body = "data")
  3: required string message (api.body = "msg")
}

struct DeleteUserRequest {
  1: required UserID id (api.path = "id")
}

struct DeleteUserResponse {
  1: required i32 code (api.body = "code")
  2: optional bool data (api.body = "data")
  3: required string message (api.body = "msg")
}

struct ResetPasswordResponse {
  1: required i32 code (api.body = "code")
  2: optional bool data (api.body = "data")
  3: required string message (api.body = "msg")
}

exception UserNotFound {
  1: required UserID id
  2: required string message
}

exception ValidationException {
  1: required string field
  2: required string message
}

service HealthService {
  string Ping() (
    api.get = "/api/v1/health/ping",
    api.handler_path = "health",
    api.category = "health"
  )
}

service UserService extends HealthService {
  CreateUserResponse CreateUser(1: CreateUserRequest req) throws (
    1: shared.BizException biz,
    2: ValidationException invalid
  ) (
    api.post = "/api/v1/users",
    api.handler_path = "users",
    api.category = "users"
  )

  GetUserResponse GetUser(1: GetUserRequest req) throws (
    1: UserNotFound not_found
  ) (
    api.get = "/api/v1/users/:id",
    api.handler_path = "users",
    api.category = "users"
  )

  ListUsersResponse ListUsers(1: ListUsersRequest req) (
    api.get = "/api/v1/users",
    api.handler_path = "users",
    api.category = "users"
  )

  UpdateUserResponse UpdateUser(1: UpdateUserRequest req) throws (
    1: UserNotFound not_found,
    2: ValidationException invalid
  ) (
    api.put = "/api/v1/users/:id",
    api.handler_path = "users",
    api.category = USERS_CATEGORY
  )

  DeleteUserResponse DeleteUser(1: DeleteUserRequest req) throws (
    1: UserNotFound not_found
  ) (
    api.delete = "/api/v1/users/:id",
    api.handler_path = "users",
    api.category = "users"
  )

  ResetPasswordResponse ResetPassword(
    1: UserID id (api.path = "id"),
    2: string password (api.body = "password,required,min=6")
  ) throws (
    1: UserNotFound not_found,
    2: ValidationException invalid
  ) (
    api.post = "/api/v1/users/:id/reset-password",
    api.handler_path = "users",
    api.category = "users"
  )

  oneway void RebuildCache(
    1: UserID id (api.path = "id")
  ) (
    api.post = "/api/v1/users/:id/rebuild-cache",
    api.handler_path = "users",
    api.category = "users"
  )
}
```

## 7. 目标生成代码形态

下面这份代码不是要求逐字符完全一致，而是目标生成结构的参考形态。

### 7.1 目标文件

- `generated/lania/adapters/http/user_http.gen.go`

### 7.2 目标结构

```go
package http

import (
  "errors"
  "fmt"
  "net/http"

  httpbinding "github.com/sao-lang/lania-g/protocol/http/v3/binding"
  httpadapter "lania-g/v3/adapter/http"
)

type TraceID = string
type UserID = string

const (
  UsersCategory   = "users"
  DefaultPageSize = 20
)

type Gender int32

const (
  GenderUnknown Gender = 0
  GenderMale    Gender = 1
  GenderFemale  Gender = 2
)

type UserStatus int32

const (
  UserStatusUnknown  UserStatus = 0
  UserStatusEnabled  UserStatus = 1
  UserStatusDisabled UserStatus = 2
  UserStatusLocked   UserStatus = 3
)

type UserProfile struct {
  Nickname  string            `json:"nickname,omitempty"`
  AvatarURL string            `json:"avatarUrl,omitempty"`
  Tags      []string          `json:"tags,omitempty"`
  Ext       map[string]string `json:"ext,omitempty"`
}

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

type BizException struct {
  Code    int32  `json:"code"`
  Message string `json:"message"`
}

func (e *BizException) Error() string {
  if e == nil {
    return "biz exception"
  }
  return fmt.Sprintf("biz exception: code=%d message=%s", e.Code, e.Message)
}

func HTTPStatusFromError(err error) int {
  switch err.(type) {
  case *UserNotFound:
    return http.StatusNotFound
  case *ValidationException:
    return http.StatusBadRequest
  case *BizException:
    return http.StatusConflict
  default:
    return http.StatusInternalServerError
  }
}

type createUserRequest struct {
  Username string      `json:"username" validate:"required"`
  Password string      `json:"password" validate:"required,min=6"`
  Gender   Gender      `json:"gender"`
  Profile  UserProfile `json:"profile"`
  Contact  UserContact `json:"contact"`
}

type getUserArgs struct {
  ID      httpbinding.Param[UserID]   `param:"id" required:"true"`
  TraceID httpbinding.Header[TraceID] `header:"X-Trace-Id"`
}

type listUsersArgs struct {
  Keyword httpbinding.Query[string]     `query:"keyword"`
  Gender  httpbinding.Query[Gender]     `query:"gender"`
  Status  httpbinding.Query[UserStatus] `query:"status"`
  Page    httpbinding.Query[int32]      `query:"page"`
  Size    httpbinding.Query[int32]      `query:"size"`
  TraceID httpbinding.Header[TraceID]   `header:"X-Trace-Id"`
}

type updateUserArgs struct {
  Ctx httpbinding.Context
  ID  httpbinding.Param[UserID] `param:"id" required:"true"`
}

type updateUserRequest struct {
  Status  UserStatus  `json:"status"`
  Profile UserProfile `json:"profile"`
  Contact UserContact `json:"contact"`
}

type HealthController struct{}
type UserController struct{}

func RegisterHealthHttp(api *httpadapter.API, controller *HealthController) {
  if api == nil || controller == nil {
    return
  }
  api.Controller("/health", controller).
    Get("/ping", controller.Ping).
    Build()
}

func RegisterUserHttp(api *httpadapter.API, controller *UserController) {
  if api == nil || controller == nil {
    return
  }
  api.Controller("/users", controller).
    Post("", controller.Create).
    Get("/:id", controller.Get).
    Get("", controller.List).
    Put("/:id", controller.Update).
    Delete("/:id", controller.Delete).
    Post("/:id/reset-password", controller.ResetPassword).
    Post("/:id/rebuild-cache", controller.RebuildCache).
    Build()
}

func (c *UserController) Create(ctx httpbinding.Context) (any, error) {
  var req createUserRequest
  if err := ctx.ShouldBindJSON(&req); err != nil {
    return nil, err
  }
  if err := req.Contact.ValidateUnion(); err != nil {
    return nil, err
  }
  ctx.Status(http.StatusCreated)
  return nil, errors.New("TODO")
}
```

## 8. 还可以生成哪些其它代码结构

在默认单文件 HTTP 输出之外，后续还可以扩展以下可选产物：

### 8.1 错误映射文件

- `*_http_errors.gen.go`

内容包括：

- `exception` 类型
- `throws` 到 HTTP 状态码映射
- 统一错误包装 helper

### 8.2 Envelope 文件

- `*_http_envelope.gen.go`

内容包括：

- `code/msg/data` 统一响应包装
- 默认成功/失败渲染 helper

### 8.3 Demo 启动文件

- `*_http_demo.gen.go`

内容包括：

- `main.go` 风格最小可运行示例
- adapter 初始化
- controller 注册

### 8.4 OpenAPI 文件

- `*_http_openapi.gen.go`

内容包括：

- route metadata
- request/response schema
- enum/exception 描述

### 8.5 Client 文件

- `*_http_client.gen.go`

内容包括：

- 对应路由的 HTTP client
- request/response DTO 复用
- 错误解包 helper

## 9. 里程碑建议

### 里程碑 1：替换解析前端

- 引入 `tree-sitter` Thrift grammar
- 产出完整 AST
- 保留现有最小 IR

### 里程碑 2：补语义层

- `include`
- 符号表
- `typedef`
- `enum`
- `union`
- `exception`
- `throws`
- `service extends`

### 里程碑 3：补 HTTP 单文件生成

- alias / const / enum / exception 输出
- union 校验 helper
- `multi-arg method -> Args/Request`
- `oneway -> 202` 语义

### 里程碑 4：补测试矩阵

- 多文件 include
- enum/default/container
- throws/exception
- extends
- 多参数方法
- annotation 组合

## 10. 测试建议

建议至少补四类测试：

1. `Parser`
   - 语法覆盖测试
2. `Semantic`
   - include/extends/throws/typedef 展开
3. `HTTP Render`
   - 生成代码快照
4. `Workflow`
   - `lan generate module` 端到端生成

关键断言应覆盖：

- 单文件 HTTP 输出是否成立
- `contracts/modules/dsl` 是否在纯 HTTP 模式下消失
- `ShouldBindJSON` 与 `Args` 的切换是否正确
- `throws` 是否能稳定生成错误骨架
- `include` 后的跨文件类型是否正确落到生成代码里

## 11. 当前建议

当前建议的正式推进边界是：

- 先只聚焦 `thrift -> http rest`
- 一次性补齐与 HTTP 生成直接相关的完整语法支持
- 暂不同时重写 `grpc/ws/graphql`

这是当前风险最低、收益最高的推进方式。
