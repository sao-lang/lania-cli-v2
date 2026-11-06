//! 模块生成功能的回归测试。
//!
//! 关键点：
//! - 包含异步/超时/取消等控制流
use super::*;

#[tokio::test]
async fn generate_module_workflow_writes_outputs_and_injects_main() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
  language: go
  main: main.go
inputs:
  - name: user
    source: protobuf
    path: schemas/proto
    include:
      - "**/*.proto"
    targets:
      - grpc
      - http
targets:
  - kind: grpc
  - kind: http
output:
  root: generated/lania
  moduleDir: generated/lania/modules
  adapterDir: generated/lania/adapters
  contractDir: generated/lania/contracts
  manifest: .lania/module-gen.lock.json
inject:
  enabled: true
  targetMain: main.go
  marker:
    start: "lania:modules:start"
    end: "lania:modules:end"
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"message User {
  string id = 1;
}

service UserService {
  rpc GetUser (User) returns (User);
}
"#,
    );
    std::fs::write(
        root.join("main.go"),
        "package main\n\nfunc main() {\n    // lania:modules:start\n    // lania:modules:end\n}\n",
    )
    .expect("main.go written");

    let workflow = GenerateModuleWorkflow;
    let result = workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("generate module workflow succeeds");

    assert_eq!(result.workflow, "generate-module");
    assert!(root
        .join("generated/lania/contracts/user.contract.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/adapters/grpc/user_service/dto.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/adapters/grpc/user_service/register.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/adapters/grpc/demo/user/bootstrap.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/modules/user_module.gen.go")
        .exists());
    assert!(root
        .join("generated/lania/modules/generated_modules.gen.go")
        .exists());
    assert!(root.join("zz_lania_module_inject.gen.go").exists());
    assert!(root.join(".lania/module-gen.lock.json").exists());
    assert!(std::fs::read_to_string(root.join("main.go"))
        .expect("main.go readable")
        .contains("RegisterLaniaGeneratedModules"));
    assert!(!root
        .join("generated/lania/adapters/grpc/user_grpc.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/adapters/grpc/user_grpc_dsl.gen.go")
        .exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_workflow_supports_check_and_clean() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: user
    source: protobuf
    path: schemas/proto
targets:
  - kind: grpc
output:
  manifest: .lania/module-gen.lock.json
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"message User {
  string id = 1;
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    let preview = workflow
        .run(
            &services,
            GenerateModuleWorkflowInput {
                check: true,
                ..generate_module_input(&root)
            },
        )
        .await
        .expect("check workflow succeeds");
    assert_eq!(preview.state, WorkflowState::Planned);
    assert!(preview
        .notes
        .iter()
        .any(|note| note.contains("drift detected")));

    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("initial module generation succeeds");
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: user
    source: protobuf
    path: schemas/proto
targets:
  - kind: http
output:
  manifest: .lania/module-gen.lock.json
inject:
  enabled: false
"#,
    );
    let clean_result = workflow
        .run(
            &services,
            GenerateModuleWorkflowInput {
                clean: true,
                ..generate_module_input(&root)
            },
        )
        .await
        .expect("module clean succeeds");
    assert!(!root
        .join("generated/lania/adapters/grpc/user_service/dto.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/adapters/grpc/user_service/register.gen.go")
        .exists());
    assert!(clean_result
        .notes
        .iter()
        .any(|note| note.contains("clean mode")));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_grpc_renders_proto_binding_and_stream_signatures() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: user
    source: protobuf
    path: schemas/proto
targets:
  - kind: grpc
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"syntax = "proto2";

message Contact {
  oneof value {
    string email = 1;
    string mobile = 2;
  }
}

message CreateUserRequest {
  required string name = 1;
  optional Contact contact = 2;
}

message CreateUserResponse {
  optional string id = 1;
}

message WatchUsersRequest {
  optional string keyword = 1;
}

message UserEvent {
  optional string text = 1;
}

service UserService {
  option deprecated = true;
  rpc CreateUser(CreateUserRequest) returns (CreateUserResponse) {
    option deprecated = true;
    option idempotency_level = IDEMPOTENT;
  }
  rpc WatchUsers(WatchUsersRequest) returns (stream UserEvent);
  rpc UploadUsers(stream UserEvent) returns (CreateUserResponse);
  rpc ChatUsers(stream UserEvent) returns (stream UserEvent);
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("grpc module generation succeeds");

    let grpc_dto = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/dto.gen.go"),
    )
    .expect("grpc dto readable");
    let grpc_register = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/register.gen.go"),
    )
    .expect("grpc register readable");
    let grpc_create = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/create_user.gen.go"),
    )
    .expect("grpc create readable");
    let grpc_watch = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/watch_users.gen.go"),
    )
    .expect("grpc watch readable");
    let grpc_upload = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/upload_users.gen.go"),
    )
    .expect("grpc upload readable");
    let grpc_chat = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/chat_users.gen.go"),
    )
    .expect("grpc chat readable");
    let grpc_metadata = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/metadata.gen.go"),
    )
    .expect("grpc metadata readable");
    let grpc_errors = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/errors.gen.go"),
    )
    .expect("grpc errors readable");
    let grpc_demo = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/demo/user/bootstrap.gen.go"),
    )
    .expect("grpc demo readable");

    assert!(grpc_dto.contains("package user_service"));
    assert!(grpc_dto.contains("type CreateUserRequest struct"));
    assert!(grpc_dto.contains("Name string `json:\"name\" validate:\"required\"`"));
    assert!(grpc_dto.contains("type Contact struct"));
    assert!(grpc_dto.contains("func (v Contact) ValidateOneof() error"));
    assert!(grpc_register.contains("b.Method(\"CreateUser\", receiver.CreateUser).WithReqType((*structpb.Struct)(nil))"));
    assert!(grpc_register.contains("b.ServerStreamMethod(\"WatchUsers\", receiver.WatchUsers).WithReqType((*structpb.Struct)(nil))"));
    assert!(grpc_register.contains("b.ClientStreamMethod(\"UploadUsers\", receiver.UploadUsers)"));
    assert!(grpc_register.contains("b.BidiStreamMethod(\"ChatUsers\", receiver.ChatUsers)"));
    assert!(grpc_create.contains("func (r *UserUserServiceGrpcReceiver) CreateUser(ctx grpcbinding.GRPCContext) (any, error)"));
    assert!(grpc_create.contains("if err := ctx.ShouldBindReq(&req); err != nil {"));
    assert!(grpc_watch.contains("Stream grpcbinding.ServerStream[*structpb.Struct]"));
    assert!(grpc_upload.contains("Stream grpcbinding.ClientStream[*structpb.Struct]"));
    assert!(grpc_chat.contains("Stream grpcbinding.BidiStream[*structpb.Struct, *structpb.Struct]"));
    assert!(grpc_metadata.contains("type ServiceMetadata struct"));
    assert!(grpc_metadata.contains("type MethodMetadata struct"));
    assert!(grpc_metadata.contains("Deprecated: true"));
    assert!(grpc_metadata.contains("IdempotencyLevel: \"IDEMPOTENT\""));
    assert!(grpc_metadata.contains("FullMethod: \"/UserService/CreateUser\""));
    assert!(grpc_errors.contains("func GRPCStatusCodeFromError(err error) codes.Code"));
    assert!(grpc_errors.contains("return status.Error(GRPCStatusCodeFromError(err), err.Error())"));
    assert!(grpc_demo.contains("grpcadapter \"github.com/sao-lang/lania-g/protocol/grpc/v3\""));
    assert!(grpc_demo.contains("package bootstrap"));
    assert!(grpc_demo.contains("func NewUserGrpcBootstrap() *UserGrpcBootstrap"));
    assert!(grpc_demo.contains("func (b *UserGrpcBootstrap) Providers() []any"));
    assert!(grpc_demo.contains(
        "generatedUserService.RegisterUserUserServiceGrpc(api, b.UserUserServiceGrpcReceiver)"
    ));
    assert!(!root
        .join("generated/lania/contracts/user.contract.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/modules/user_module.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/adapters/grpc/user_grpc.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/adapters/grpc/user_grpc_dsl.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/modules/generated_modules.gen.go")
        .exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_grpc_supports_grpc_root_dir() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: proto-grpc
    source: protobuf
    path: schemas/proto
    targets:
      - grpc
output:
  grpcRootDir: generated/grpc
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"syntax = "proto3";

message User {
  string id = 1;
}

service UserService {
  rpc GetUser(User) returns (User);
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("grpc root dir generation succeeds");

    assert!(root.join("generated/grpc/user_service/dto.gen.go").exists());
    assert!(root
        .join("generated/grpc/user_service/register.gen.go")
        .exists());
    assert!(root
        .join("generated/grpc/user_service/metadata.gen.go")
        .exists());
    assert!(root
        .join("generated/grpc/user_service/errors.gen.go")
        .exists());
    assert!(root
        .join("generated/grpc/bootstrap.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/adapters/grpc/demo/proto_grpc/bootstrap.gen.go")
        .exists());
    let grpc_bootstrap = std::fs::read_to_string(root.join("generated/grpc/bootstrap.gen.go"))
        .expect("grpc bootstrap readable");
    assert!(grpc_bootstrap.contains("package bootstrap"));
    assert!(grpc_bootstrap.contains("func NewProtoGrpcBootstrap() *ProtoGrpcBootstrap"));
    assert!(grpc_bootstrap.contains(
        "generatedUserService.RegisterProtoGrpcUserServiceGrpc(api, b.ProtoGrpcUserServiceGrpcReceiver)"
    ));
    assert!(!root.join("generated/grpc/proto_grpc_grpc.gen.go").exists());
    assert!(!root
        .join("generated/grpc/proto_grpc_grpc_dsl.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/modules/generated_modules.gen.go")
        .exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_workflow_supports_graphql_and_ws_targets() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: gateway
    source: graphql
    path: schemas/graphql
    include:
      - "**/*.graphql"
targets:
  - kind: graphql
  - kind: ws
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/graphql/schema.graphql",
        r#"type User {
  id: ID!
}

type Query {
  user(id: ID!): User
}

type Subscription {
  userUpdated: User
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("graphql module generation succeeds");

    let graphql_adapter = std::fs::read_to_string(
        root.join("generated/lania/adapters/graphql/gateway_graphql.gen.go"),
    )
    .expect("graphql adapter readable");
    let graphql_dsl = std::fs::read_to_string(
        root.join("generated/lania/adapters/graphql/gateway_graphql_dsl.gen.go"),
    )
    .expect("graphql dsl readable");
    let ws_adapter =
        std::fs::read_to_string(root.join("generated/lania/adapters/ws/gateway_ws.gen.go"))
            .expect("ws adapter readable");
    let ws_dsl =
        std::fs::read_to_string(root.join("generated/lania/adapters/ws/gateway_ws_dsl.gen.go"))
            .expect("ws dsl readable");
    assert!(graphql_adapter.contains("query user"));
    assert!(ws_adapter.contains("user.updated"));
    assert!(graphql_dsl.contains("api.Resolver"));
    assert!(ws_dsl.contains("api.Gateway"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_workflow_supports_json_source_with_overrides() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: account
    source: yaml
    path: schemas/json
targets:
  - kind: http
  - kind: grpc
overrides:
  operations:
    GetAccount:
      service: AccountService
      input: GetAccountRequest
      output: Account
      kind: query
      http:
        method: GET
        path: /accounts/:id
      grpc:
        service: AccountService
        method: GetAccount
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/json/account.yaml",
        r#"title: Account
type: object
properties:
  id:
    type: string
  enabled:
    type: boolean
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("json module generation succeeds");

    let contract =
        std::fs::read_to_string(root.join("generated/lania/contracts/account.contract.gen.go"))
            .expect("contract readable");
    let http_adapter =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/account_http.gen.go"))
            .expect("http adapter readable");
    let http_dsl =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/account_http_dsl.gen.go"))
            .expect("http dsl readable");
    assert!(contract.contains("type Account struct"));
    assert!(http_adapter.contains("GET /accounts/:id"));
    assert!(http_dsl.contains("api.Controller"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_http_rest_renders_json_binding_for_validated_body() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: user-http
    source: thrift
    path: schemas/thrift
    targets:
      - http
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/thrift/user.thrift",
        r#"struct Data {
}

struct CreateUserResponse {
  1: required i32 code (api.body = "code")
  2: optional Data data (api.body = "data")
  3: required string message (api.body = "msg")
}

struct CreateUserRequest {
  1: required string username (api.body = "username,required")
  2: required string password (api.body = "password,required")
}

service UserService {
  CreateUserResponse CreateUser(1: CreateUserRequest req) (api.post = "/xxx/api/v1/users/create", api.handler_path = "users", api.category = "users")
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("http rest generation succeeds");

    let http_register =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/users/register.gen.go"))
            .expect("http register readable");
    let http_dto =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/users/dto.gen.go"))
            .expect("http dto readable");
    let http_create =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/users/create_user.gen.go"))
            .expect("http create readable");

    assert!(http_register.contains("package users"));
    assert!(http_register.contains("type UserController struct {}"));
    assert!(http_register.contains("b := api.Controller(\"/users\", controller)"));
    assert!(http_register.contains("b.Post(\"/create\", controller.Create)"));
    assert!(http_dto.contains("type CreateUserResponse struct {"));
    assert!(http_dto.contains("Message string `json:\"msg\"`"));
    assert!(http_create.contains("type createUserRequest struct {"));
    assert!(http_create.contains("Username string `json:\"username\" validate:\"required\"`"));
    assert!(http_create.contains("Password string `json:\"password\" validate:\"required\"`"));
    assert!(
        http_create.contains("func (c *UserController) Create(ctx httpbinding.Context) (any, error)")
    );
    assert!(http_create.contains("if err := ctx.ShouldBindJSON(&req); err != nil {"));
    assert!(!root
        .join("generated/lania/contracts/user_http.contract.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/modules/user_http_module.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/modules/generated_modules.gen.go")
        .exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_http_rest_renders_direct_args_without_validation_rules() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: user-http
    source: thrift
    path: schemas/thrift
    targets:
      - http
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/thrift/user.thrift",
        r#"struct UpdateUserRequest {
  1: required i32 id (api.path = "id")
  2: optional string username (api.body = "username")
  3: optional string password (api.body = "password")
}

struct UpdateUserResponse {
  1: required i32 code (api.body = "code")
}

service UserService {
  UpdateUserResponse UpdateUser(1: UpdateUserRequest req) (api.post = "/xxx/api/v1/users/update/:id", api.handler_path = "users")
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("http rest args generation succeeds");

    let http_update =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/users/update_user.gen.go"))
            .expect("http update readable");

    assert!(http_update.contains("type updateUserArgs struct {"));
    assert!(http_update.contains("ID httpbinding.Param[int32] `param:\"id\" required:\"true\"`"));
    assert!(http_update.contains("Username httpbinding.Body[string] `body:\"username\"`"));
    assert!(http_update.contains("Password httpbinding.Body[string] `body:\"password\"`"));
    assert!(http_update.contains("func (c *UserController) Update(args updateUserArgs) (any, error)"));
    assert!(!http_update.contains("ShouldBindJSON"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_http_rest_supports_http_root_dir() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: user-http
    source: thrift
    path: schemas/thrift
    targets:
      - http
output:
  httpRootDir: generated/http-root
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/thrift/user.thrift",
        r#"struct GetUserRequest {
  1: required string id (api.path = "id")
}

struct GetUserResponse {
  1: required i32 code (api.body = "code")
}

service UserService {
  GetUserResponse GetUser(1: GetUserRequest req) (api.get = "/api/v1/users/:id", api.handler_path = "users")
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("http root dir generation succeeds");

    assert!(root.join("generated/http-root/bootstrap.gen.go").exists());
    assert!(root
        .join("generated/http-root/users/register.gen.go")
        .exists());
    assert!(root
        .join("generated/http-root/users/get_user.gen.go")
        .exists());
    assert!(!root
        .join("generated/lania/adapters/http/demo/user_http/main.go")
        .exists());
    let http_demo_main = std::fs::read_to_string(root.join("generated/http-root/bootstrap.gen.go"))
        .expect("http demo readable");
    assert!(http_demo_main.contains(
        "generatedUsers \"REPLACE_WITH_YOUR_MODULE/generated/http-root/users\""
    ));
    assert!(http_demo_main.contains("package bootstrap"));
    assert!(http_demo_main.contains("func NewUserHttpBootstrap() *UserHttpBootstrap"));
    assert!(http_demo_main.contains("func (b *UserHttpBootstrap) Providers() []any"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_http_rest_supports_rich_thrift_syntax() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: user-http
    source: thrift
    path: schemas/thrift
    targets:
      - http
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/thrift/shared.thrift",
        r#"namespace go demo.shared

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
"#,
    );
    write_proto_schema(
        &root,
        "schemas/thrift/user.thrift",
        r#"namespace go demo.user

include "shared.thrift"

typedef string UserID

const string USERS_CATEGORY = "users"

enum UserStatus {
  UNKNOWN = 0,
  ENABLED = 1,
  DISABLED = 2
}

struct UserProfile {
  1: optional string nickname (api.body = "nickname")
  2: optional list<string> tags (api.body = "tags")
  3: optional map<string, string> ext (api.body = "ext")
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

struct GetUserRequest {
  1: required UserID id (api.path = "id")
  2: optional shared.TraceID trace_id (api.header = "X-Trace-Id")
}

struct GetUserResponse {
  1: required i32 code (api.body = "code")
  2: optional User data (api.body = "data")
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

service BaseUserService {
  GetUserResponse ListUsers(1: shared.PageQuery req) (
    api.get = "/api/v1/users",
    api.handler_path = "users"
  )
}

service UserService extends BaseUserService {
  GetUserResponse GetUser(1: GetUserRequest req) throws (
    1: UserNotFound not_found
  ) (
    api.get = "/api/v1/users/:id",
    api.handler_path = "users",
    api.category = USERS_CATEGORY
  )

  GetUserResponse ResetPassword(
    1: UserID id (api.path = "id"),
    2: string password (api.body = "password,required,min=6")
  ) throws (
    1: ValidationException invalid
  ) (
    api.post = "/api/v1/users/:id/reset-password",
    api.handler_path = "users"
  )

  oneway void RebuildCache(
    1: UserID id (api.path = "id")
  ) (
    api.post = "/api/v1/users/:id/rebuild-cache",
    api.handler_path = "users"
  )
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("rich thrift generation succeeds");

    let http_dto =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/users/dto.gen.go"))
            .expect("http dto readable");
    let http_register =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/users/register.gen.go"))
            .expect("http register readable");
    let http_list =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/users/list_users.gen.go"))
            .expect("http list readable");
    let http_get =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/users/get_user.gen.go"))
            .expect("http get readable");
    let http_reset = std::fs::read_to_string(
        root.join("generated/lania/adapters/http/users/reset_password.gen.go"),
    )
    .expect("http reset readable");
    let http_rebuild = std::fs::read_to_string(
        root.join("generated/lania/adapters/http/users/rebuild_cache.gen.go"),
    )
    .expect("http rebuild readable");
    let http_errors = std::fs::read_to_string(
        root.join("generated/lania/adapters/http/users/errors.gen.go"),
    )
    .expect("http errors readable");
    let http_envelope = std::fs::read_to_string(
        root.join("generated/lania/adapters/http/users/envelope.gen.go"),
    )
    .expect("http envelope readable");
    let http_demo_main = std::fs::read_to_string(
        root.join("generated/lania/adapters/http/demo/user_http/bootstrap.gen.go"),
    )
    .expect("http demo main readable");

    assert!(http_dto.contains("package users"));
    assert!(http_dto.contains("type TraceID = string"));
    assert!(http_dto.contains("type UserID = string"));
    assert!(http_dto.contains("DefaultPageSize int32 = 20"));
    assert!(http_dto.contains("type Gender int32"));
    assert!(http_dto.contains("type PageQuery struct {"));
    assert!(http_dto.contains("Page int32 `json:\"page,omitempty\" default:\"1\"`"));
    assert!(http_dto.contains("Size int32 `json:\"size,omitempty\" default:\"20\"`"));
    assert!(http_dto.contains("type UserContact struct {"));
    assert!(http_dto.contains("Email *string `json:\"email,omitempty\"`"));
    assert!(http_dto.contains("func (v UserContact) ValidateUnion() error"));
    assert!(http_dto.contains("type BizException struct {"));
    assert!(http_dto.contains("func (e *BizException) Error() string"));
    assert!(http_errors.contains("func HTTPStatusFromError(err error) int"));
    assert!(http_list.contains("type listUsersArgs struct {"));
    assert!(http_list.contains("Page httpbinding.Query[int32] `query:\"page\" default:\"1\"`"));
    assert!(http_list.contains("Size httpbinding.Query[int32] `query:\"size\" default:\"20\"`"));
    assert!(http_register.contains("b.Get(\"\", controller.List)"));
    assert!(http_list.contains("func (c *UserController) List(args listUsersArgs) (any, error)"));
    assert!(http_get.contains("ID httpbinding.Param[UserID] `param:\"id\" required:\"true\"`"));
    assert!(http_get.contains("TraceID httpbinding.Header[TraceID] `header:\"X-Trace-Id\"`"));
    assert!(http_reset.contains("type resetPasswordRequest struct {"));
    assert!(http_reset.contains("Password string `json:\"password\" validate:\"required,min=6\"`"));
    assert!(http_reset
        .contains("func (c *UserController) ResetPassword(args resetPasswordArgs) (any, error)"));
    assert!(http_reset.contains("if err := args.Ctx.ShouldBindJSON(&req); err != nil {"));
    assert!(http_rebuild
        .contains("func (c *UserController) RebuildCache(args rebuildCacheArgs) (any, error)"));
    assert!(http_rebuild.contains("args.Ctx.Status(http.StatusAccepted)"));
    assert!(http_errors.contains("func HTTPErrorEnvelopeFromError(err error) HTTPEnvelope[any]"));
    assert!(http_errors.contains("func HTTPWriteError(err error) (int, HTTPEnvelope[any])"));
    assert!(http_envelope.contains("type HTTPEnvelope[T any] struct {"));
    assert!(http_envelope.contains("func HTTPEnvelopeFromResult[T any](data T, err error) HTTPEnvelope[T]"));
    assert!(http_demo_main.contains("package bootstrap"));
    assert!(http_demo_main.contains(
        "generatedUsers \"REPLACE_WITH_YOUR_MODULE/generated/lania/adapters/http/users\""
    ));
    assert!(http_demo_main.contains("func NewUserHttpBootstrap() *UserHttpBootstrap"));
    assert!(http_demo_main.contains("b.UserController = &generatedUsers.UserController{}"));
    assert!(http_demo_main.contains(
        "generatedUsers.RegisterUserHttpUsersHttp(api, b.UserController)"
    ));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn generate_module_workflow_supports_schema_embedded_transport_metadata() {
    let (services, root) = services(ExecService::dry_run());
    write_module_config(
        &root,
        r#"version: 1
framework:
  name: lania-g
inputs:
  - name: thrift-http
    source: thrift
    path: schemas/thrift
    targets: [http]
  - name: yaml-ws
    source: yaml
    path: schemas/ws
    targets: [ws]
  - name: proto-grpc
    source: protobuf
    path: schemas/proto
    targets: [grpc]
  - name: gql-graphql
    source: graphql
    path: schemas/graphql
    targets: [graphql]
inject:
  enabled: false
"#,
    );
    write_proto_schema(
        &root,
        "schemas/thrift/user.thrift",
        r#"namespace go demo.user

struct User {
  1: string id
}

service UserService {
  User getUser(1: User req) // lania:http GET /users/:id
}
"#,
    );
    write_proto_schema(
        &root,
        "schemas/ws/user.yaml",
        r#"title: UserMessage
type: object
properties:
  id:
    type: string
  name:
    type: string
x-lania-operations:
  userCreated:
    service: UserGateway
    input: UserMessage
    output: UserMessage
    kind: event
    ws:
      namespace: /ws/user
      event: user.created
"#,
    );
    write_proto_schema(
        &root,
        "schemas/proto/user.proto",
        r#"syntax = "proto3";

message User {
  string id = 1;
}

service UserService {
  rpc GetUser (User) returns (User); // lania:grpc service=UserService method=GetUser
}
"#,
    );
    write_proto_schema(
        &root,
        "schemas/graphql/user.graphql",
        r#"type User {
  id: ID!
}

type Query {
  user(id: ID!): User
}
"#,
    );

    let workflow = GenerateModuleWorkflow;
    workflow
        .run(&services, generate_module_input(&root))
        .await
        .expect("schema-embedded metadata generation succeeds");

    let thrift_http_register = std::fs::read_to_string(
        root.join("generated/lania/adapters/http/userservice/register.gen.go"),
    )
    .expect("thrift http register readable");
    let thrift_http_get =
        std::fs::read_to_string(root.join("generated/lania/adapters/http/userservice/get_user.gen.go"))
            .expect("thrift http get readable");
    let yaml_ws_dsl =
        std::fs::read_to_string(root.join("generated/lania/adapters/ws/yaml_ws_ws_dsl.gen.go"))
            .expect("yaml ws dsl readable");
    let proto_grpc_register = std::fs::read_to_string(
        root.join("generated/lania/adapters/grpc/user_service/register.gen.go"),
    )
    .expect("proto grpc register readable");
    let gql_graphql_dsl = std::fs::read_to_string(
        root.join("generated/lania/adapters/graphql/gql_graphql_graphql_dsl.gen.go"),
    )
    .expect("graphql dsl readable");

    assert!(thrift_http_register.contains("Get(\"/users/:id\", controller.GetUser)"));
    assert!(thrift_http_register.contains("type UserserviceController struct {}"));
    assert!(thrift_http_get.contains("ID httpbinding.Param[string] `param:\"id\"`"));
    assert!(yaml_ws_dsl.contains("Gateway(\"/ws/user\""));
    assert!(yaml_ws_dsl.contains("On(\"user.created\""));
    assert!(proto_grpc_register.contains("Service(\"UserService\""));
    assert!(proto_grpc_register.contains("Method(\"GetUser\""));
    assert!(gql_graphql_dsl.contains("Resolver(\"GraphqlService\""));
    assert!(gql_graphql_dsl.contains("Query(\"user\""));
    let _ = std::fs::remove_dir_all(root);
}
