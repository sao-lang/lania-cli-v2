namespace go demo.user

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
  CreateUserResponse CreateUser(1: CreateUserRequest req) throws (
    1: ValidationException invalid
  ) (
    api.post = "/api/v1/users",
    api.handler_path = "users",
    api.category = USERS_CATEGORY
  )

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
