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
