//! `add` 生成器表（§4.5.2 / §4.5.3）：**仓库内唯一一处生成器定义**。
//!
//! 三级结构：`框架` → `生成器` → `产物（N 个文件）`。
//! Node bridge 通过 bridge 方法 `addGenerator.list` 读取这张表，
//! TS 侧不再维护第二份平铺清单（§4.5.6）。
//!
//! 范围：B6 落地 **nestjs**（第一个框架）与**通用生成器**（`component` / `config`）；
//! B7 补齐 **nextjs / gin / fastapi / axum / flutter**（每个框架至少一个多文件生成器）。

//! 生成器参数（位置参数 + 选项）。
#[derive(Debug, Clone, Copy)]
pub struct GeneratorQuestion {
    pub field: &'static str,
    pub label_en: &'static str,
    pub label_zh: &'static str,
    pub kind: GeneratorQuestionKind,
    pub required: bool,
    /// 选项型问题的候选值（`Select` 用）。
    pub choices: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorQuestionKind {
    Text,
    Select,
    Confirm,
}

/// 一个产物文件规则。
#[derive(Debug, Clone, Copy)]
pub struct ArtifactRule {
    /// 路径模板，支持 `{name}` / `{Name}` / `{name_kebab}` 等占位符。
    pub path: &'static str,
    /// 内置模板名（见 `super::templates::ARTIFACT_TEMPLATES`）。
    pub template: &'static str,
    /// 仅在某个变体下生成（`option=value`）。
    pub when: Option<&'static str>,
}

/// 注册副作用（v1 只做显式标记插入，不做 AST 改写）。
#[derive(Debug, Clone, Copy)]
pub struct RegistrationRule {
    /// 目标文件路径模板）。
    pub target: &'static str,
    /// 插入锚点标记；标记不存在时跳过并给出提示。
    pub marker: &'static str,
    /// 插入内容模板（同样支持命名占位符）。
    pub snippet: &'static str,
}

/// 注册锚点约定（§4.5.5）：`MarkerInsert` 之外一律 `None`。
pub const LANIA_IMPORTS_MARKER: &str = "// <lania:imports>";

/// 模块声明锚点（`mod.rs` / `lib.rs` 里的 `// <lania:mods>`）。
pub const LANIA_MODS_MARKER: &str = "// <lania:mods>";

/// 路由注册锚点（`router.go` / `router.rs` 里的 `// <lania:routes>`）。
pub const LANIA_ROUTES_MARKER: &str = "// <lania:routes>";

#[derive(Debug, Clone, Copy)]
pub struct Generator {
    /// 例如 `nestjs`；空串表示「通用生成器」。
    pub framework: &'static str,
    /// 例如 `resource` / `controller`。
    pub name: &'static str,
    pub label_en: &'static str,
    pub label_zh: &'static str,
    pub questions: &'static [GeneratorQuestion],
    pub artifacts: &'static [ArtifactRule],
    pub registrations: &'static [RegistrationRule],
    /// 该生成器是否要求 `name`（配置类生成器不需要）。
    pub wants_name: bool,
}

/// 通用生成器的 `framework` 取值。
pub const GENERIC_FRAMEWORK: &str = "";

const VARIANT_QUESTION: GeneratorQuestion = GeneratorQuestion {
    field: "variant",
    label_en: "Variant",
    label_zh: "变体",
    kind: GeneratorQuestionKind::Select,
    required: true,
    choices: &["rest", "graphql"],
};

const CRUD_QUESTION: GeneratorQuestion = GeneratorQuestion {
    field: "crud",
    label_en: "Generate CRUD entry points?",
    label_zh: "生成 CRUD 入口点？",
    kind: GeneratorQuestionKind::Confirm,
    required: false,
    choices: &[],
};

const DESCRIPTION_QUESTION: GeneratorQuestion = GeneratorQuestion {
    field: "description",
    label_en: "One-line description (optional)",
    label_zh: "一句话描述（可选）",
    kind: GeneratorQuestionKind::Text,
    required: false,
    choices: &[],
};

const NEST_RESOURCE_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "src/{name_kebab}/{name_kebab}.module.ts",
        template: "nestjs.module",
        when: None,
    },
    ArtifactRule {
        path: "src/{name_kebab}/{name_kebab}.controller.ts",
        template: "nestjs.controller",
        when: None,
    },
    ArtifactRule {
        path: "src/{name_kebab}/{name_kebab}.service.ts",
        template: "nestjs.service",
        when: None,
    },
    ArtifactRule {
        path: "src/{name_kebab}/dto/create-{name_singular}.dto.ts",
        template: "nestjs.dto_create",
        when: None,
    },
    ArtifactRule {
        path: "src/{name_kebab}/dto/update-{name_singular}.dto.ts",
        template: "nestjs.dto_update",
        when: None,
    },
    ArtifactRule {
        path: "src/{name_kebab}/entities/{name_singular}.entity.ts",
        template: "nestjs.entity",
        when: None,
    },
    ArtifactRule {
        path: "src/{name_kebab}/{name_kebab}.controller.spec.ts",
        template: "nestjs.controller_spec",
        when: None,
    },
    ArtifactRule {
        path: "src/{name_kebab}/{name_kebab}.resolver.ts",
        template: "nestjs.resolver",
        when: Some("variant=graphql"),
    },
];

const NEST_RESOURCE_REGISTRATIONS: &[RegistrationRule] = &[RegistrationRule {
    target: "src/app.module.ts",
    marker: LANIA_IMPORTS_MARKER,
    snippet: "import { {Name}Module } from './{name_kebab}/{name_kebab}.module';",
}];

const NEST_MODULE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/{name_kebab}/{name_kebab}.module.ts",
    template: "nestjs.module",
    when: None,
}];

const NEST_CONTROLLER_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/{name_kebab}/{name_kebab}.controller.ts",
    template: "nestjs.controller",
    when: None,
}];

const NEST_SERVICE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/{name_kebab}/{name_kebab}.service.ts",
    template: "nestjs.service",
    when: None,
}];

const NEST_DTO_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "src/{name_kebab}/dto/create-{name_singular}.dto.ts",
        template: "nestjs.dto_create",
        when: None,
    },
    ArtifactRule {
        path: "src/{name_kebab}/dto/update-{name_singular}.dto.ts",
        template: "nestjs.dto_update",
        when: None,
    },
];

const NEST_ENTITY_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/{name_kebab}/entities/{name_singular}.entity.ts",
    template: "nestjs.entity",
    when: None,
}];

const NEST_GUARD_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/{name_kebab}/{name_kebab}.guard.ts",
    template: "nestjs.guard",
    when: None,
}];

const NEST_SINGLE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/{name_kebab}/{name_kebab}.ts",
    template: "nestjs.single",
    when: None,
}];

const NEST_GENERATORS: &[Generator] = &[
    Generator {
        framework: "nestjs",
        name: "resource",
        label_en: "NestJS resource (module + controller + service + dto + entity + spec)",
        label_zh: "NestJS 资源（module + controller + service + dto + entity + spec）",
        questions: &[VARIANT_QUESTION, CRUD_QUESTION, DESCRIPTION_QUESTION],
        artifacts: NEST_RESOURCE_ARTIFACTS,
        registrations: NEST_RESOURCE_REGISTRATIONS,
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "module",
        label_en: "NestJS module",
        label_zh: "NestJS module",
        questions: &[],
        artifacts: NEST_MODULE_ARTIFACTS,
        registrations: NEST_RESOURCE_REGISTRATIONS,
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "controller",
        label_en: "NestJS controller",
        label_zh: "NestJS controller",
        questions: &[],
        artifacts: NEST_CONTROLLER_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "service",
        label_en: "NestJS service",
        label_zh: "NestJS service",
        questions: &[],
        artifacts: NEST_SERVICE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "dto",
        label_en: "NestJS DTOs (create + update)",
        label_zh: "NestJS DTO（create + update）",
        questions: &[],
        artifacts: NEST_DTO_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "entity",
        label_en: "NestJS entity",
        label_zh: "NestJS entity",
        questions: &[],
        artifacts: NEST_ENTITY_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "guard",
        label_en: "NestJS guard",
        label_zh: "NestJS guard",
        questions: &[],
        artifacts: NEST_GUARD_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "interceptor",
        label_en: "NestJS interceptor",
        label_zh: "NestJS interceptor",
        questions: &[],
        artifacts: NEST_SINGLE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "filter",
        label_en: "NestJS exception filter",
        label_zh: "NestJS 异常过滤器",
        questions: &[],
        artifacts: NEST_SINGLE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "pipe",
        label_en: "NestJS pipe",
        label_zh: "NestJS pipe",
        questions: &[],
        artifacts: NEST_SINGLE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "middleware",
        label_en: "NestJS middleware",
        label_zh: "NestJS middleware",
        questions: &[],
        artifacts: NEST_SINGLE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "decorator",
        label_en: "NestJS decorator",
        label_zh: "NestJS 装饰器",
        questions: &[],
        artifacts: NEST_SINGLE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "interface",
        label_en: "NestJS interface",
        label_zh: "NestJS interface",
        questions: &[],
        artifacts: NEST_SINGLE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "class",
        label_en: "NestJS class",
        label_zh: "NestJS class",
        questions: &[],
        artifacts: NEST_SINGLE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nestjs",
        name: "enum",
        label_en: "NestJS enum",
        label_zh: "NestJS enum",
        questions: &[],
        artifacts: NEST_SINGLE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
];

// ---------------------------------------------------------------------------
// B7 框架生成器：nextjs / gin / fastapi / axum / flutter
// ---------------------------------------------------------------------------

const NEXT_PAGE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/app/{name_kebab}/page.tsx",
    template: "nextjs.page",
    when: None,
}];

/// `nextjs segment`：App Router 的一个路由段（page + layout + loading）。
///
/// 这是 nextjs 的多文件生成器：三个文件必须同目录同段名才有意义，
/// 分开生成会让用户手动对齐路径。
const NEXT_SEGMENT_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "src/app/{name_kebab}/page.tsx",
        template: "nextjs.page",
        when: None,
    },
    ArtifactRule {
        path: "src/app/{name_kebab}/layout.tsx",
        template: "nextjs.layout",
        when: None,
    },
    ArtifactRule {
        path: "src/app/{name_kebab}/loading.tsx",
        template: "nextjs.loading",
        when: None,
    },
];

const NEXT_LAYOUT_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/app/{name_kebab}/layout.tsx",
    template: "nextjs.layout",
    when: None,
}];

const NEXT_ROUTE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/app/api/{name_kebab}/route.ts",
    template: "nextjs.route",
    when: None,
}];

const NEXT_LOADING_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/app/{name_kebab}/loading.tsx",
    template: "nextjs.loading",
    when: None,
}];

const NEXT_ERROR_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/app/{name_kebab}/error.tsx",
    template: "nextjs.error",
    when: None,
}];

const NEXT_NOT_FOUND_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/app/{name_kebab}/not-found.tsx",
    template: "nextjs.not_found",
    when: None,
}];

const NEXT_COMPONENT_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/components/{Name}.tsx",
    template: "nextjs.component",
    when: None,
}];

const NEXT_HOOK_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/hooks/{name_camel}.ts",
    template: "nextjs.hook",
    when: None,
}];

const NEXT_MIDDLEWARE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/middleware.ts",
    template: "nextjs.middleware",
    when: None,
}];

const NEXT_API_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/app/api/{name_kebab}/route.ts",
    template: "nextjs.route",
    when: None,
}];

const NEXT_ACTION_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/app/actions/{name_camel}.ts",
    template: "nextjs.action",
    when: None,
}];

const NEXT_GENERATORS: &[Generator] = &[
    Generator {
        framework: "nextjs",
        name: "segment",
        label_en: "Next.js route segment (page + layout + loading)",
        label_zh: "Next.js 路由段（page + layout + loading）",
        questions: &[],
        artifacts: NEXT_SEGMENT_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "page",
        label_en: "Next.js page (route segment)",
        label_zh: "Next.js 页面（路由段）",
        questions: &[],
        artifacts: NEXT_PAGE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "layout",
        label_en: "Next.js layout",
        label_zh: "Next.js 布局",
        questions: &[],
        artifacts: NEXT_LAYOUT_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "route",
        label_en: "Next.js route handler",
        label_zh: "Next.js 路由处理器",
        questions: &[],
        artifacts: NEXT_ROUTE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "loading",
        label_en: "Next.js loading boundary",
        label_zh: "Next.js loading 边界",
        questions: &[],
        artifacts: NEXT_LOADING_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "error",
        label_en: "Next.js error boundary",
        label_zh: "Next.js error 边界",
        questions: &[],
        artifacts: NEXT_ERROR_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "not-found",
        label_en: "Next.js not-found segment",
        label_zh: "Next.js not-found 段",
        questions: &[],
        artifacts: NEXT_NOT_FOUND_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "component",
        label_en: "React component (server component)",
        label_zh: "React 组件（服务端组件）",
        questions: &[],
        artifacts: NEXT_COMPONENT_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "hook",
        label_en: "React hook",
        label_zh: "React hook",
        questions: &[],
        artifacts: NEXT_HOOK_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "middleware",
        label_en: "Next.js middleware",
        label_zh: "Next.js middleware",
        questions: &[],
        artifacts: NEXT_MIDDLEWARE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "api",
        label_en: "Next.js API route",
        label_zh: "Next.js API 路由",
        questions: &[],
        artifacts: NEXT_API_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "nextjs",
        name: "action",
        label_en: "Next.js server action",
        label_zh: "Next.js 服务端 action",
        questions: &[],
        artifacts: NEXT_ACTION_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
];

const GIN_HANDLER_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "internal/handler/{name_snake}_handler.go",
    template: "gin.handler",
    when: None,
}];

const GIN_MODEL_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "internal/model/{name_snake}.go",
    template: "gin.model",
    when: None,
}];

const GIN_REPOSITORY_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "internal/repository/{name_snake}_repository.go",
    template: "gin.repository",
    when: None,
}];

const GIN_SERVICE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "internal/service/{name_snake}_service.go",
    template: "gin.service",
    when: None,
}];

const GIN_MIDDLEWARE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "internal/middleware/{name_snake}.go",
    template: "gin.middleware",
    when: None,
}];

const GIN_ROUTER_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "internal/router/{name_snake}.go",
    template: "gin.router",
    when: None,
}];

/// `gin resource`：一次产出完整分层的多文件产物（B7 的多文件生成器）。
const GIN_RESOURCE_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "internal/model/{name_snake}.go",
        template: "gin.model",
        when: None,
    },
    ArtifactRule {
        path: "internal/repository/{name_snake}_repository.go",
        template: "gin.repository",
        when: None,
    },
    ArtifactRule {
        path: "internal/service/{name_snake}_service.go",
        template: "gin.service",
        when: None,
    },
    ArtifactRule {
        path: "internal/handler/{name_snake}_handler.go",
        template: "gin.handler",
        when: None,
    },
];

const GIN_RESOURCE_REGISTRATIONS: &[RegistrationRule] = &[RegistrationRule {
    target: "internal/router/router.go",
    marker: LANIA_ROUTES_MARKER,
    snippet: "\tregister{Name}(r)",
}];

const GIN_GENERATORS: &[Generator] = &[
    Generator {
        framework: "gin",
        name: "resource",
        label_en: "Gin resource (model + repository + service + handler)",
        label_zh: "Gin 资源（model + repository + service + handler）",
        questions: &[],
        artifacts: GIN_RESOURCE_ARTIFACTS,
        registrations: GIN_RESOURCE_REGISTRATIONS,
        wants_name: true,
    },
    Generator {
        framework: "gin",
        name: "handler",
        label_en: "Gin HTTP handler",
        label_zh: "Gin HTTP handler",
        questions: &[],
        artifacts: GIN_HANDLER_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "gin",
        name: "model",
        label_en: "Gin model",
        label_zh: "Gin 模型",
        questions: &[],
        artifacts: GIN_MODEL_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "gin",
        name: "repository",
        label_en: "Gin repository",
        label_zh: "Gin 仓储层",
        questions: &[],
        artifacts: GIN_REPOSITORY_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "gin",
        name: "service",
        label_en: "Gin service",
        label_zh: "Gin 服务层",
        questions: &[],
        artifacts: GIN_SERVICE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "gin",
        name: "middleware",
        label_en: "Gin middleware",
        label_zh: "Gin 中间件",
        questions: &[],
        artifacts: GIN_MIDDLEWARE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "gin",
        name: "router",
        label_en: "Gin router group",
        label_zh: "Gin 路由组",
        questions: &[],
        artifacts: GIN_ROUTER_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
];

const FASTAPI_ROUTER_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "app/routers/{name_snake}.py",
    template: "fastapi.router",
    when: None,
}];

const FASTAPI_ENDPOINT_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "app/routers/{name_snake}.py",
    template: "fastapi.router",
    when: None,
}];

const FASTAPI_MODEL_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "app/models/{name_snake}.py",
    template: "fastapi.model",
    when: None,
}];

const FASTAPI_SCHEMA_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "app/schemas/{name_snake}.py",
    template: "fastapi.schema",
    when: None,
}];

const FASTAPI_CRUD_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "app/crud/{name_snake}.py",
    template: "fastapi.crud",
    when: None,
}];

const FASTAPI_DEPENDENCY_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "app/dependencies/{name_snake}.py",
    template: "fastapi.dependency",
    when: None,
}];

const FASTAPI_MIDDLEWARE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "app/middleware/{name_snake}.py",
    template: "fastapi.middleware",
    when: None,
}];

/// `fastapi resource`：schema + model + crud + router 一次生成。
const FASTAPI_RESOURCE_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "app/schemas/{name_snake}.py",
        template: "fastapi.schema",
        when: None,
    },
    ArtifactRule {
        path: "app/models/{name_snake}.py",
        template: "fastapi.model",
        when: None,
    },
    ArtifactRule {
        path: "app/crud/{name_snake}.py",
        template: "fastapi.crud",
        when: None,
    },
    ArtifactRule {
        path: "app/routers/{name_snake}.py",
        template: "fastapi.router",
        when: None,
    },
];

const FASTAPI_GENERATORS: &[Generator] = &[
    Generator {
        framework: "fastapi",
        name: "resource",
        label_en: "FastAPI resource (schema + model + crud + router)",
        label_zh: "FastAPI 资源（schema + model + crud + router）",
        questions: &[],
        artifacts: FASTAPI_RESOURCE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "fastapi",
        name: "router",
        label_en: "FastAPI router",
        label_zh: "FastAPI 路由",
        questions: &[],
        artifacts: FASTAPI_ROUTER_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "fastapi",
        name: "endpoint",
        label_en: "FastAPI endpoint module",
        label_zh: "FastAPI 端点模块",
        questions: &[],
        artifacts: FASTAPI_ENDPOINT_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "fastapi",
        name: "model",
        label_en: "SQLAlchemy model",
        label_zh: "SQLAlchemy 模型",
        questions: &[],
        artifacts: FASTAPI_MODEL_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "fastapi",
        name: "schema",
        label_en: "Pydantic schema",
        label_zh: "Pydantic schema",
        questions: &[],
        artifacts: FASTAPI_SCHEMA_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "fastapi",
        name: "crud",
        label_en: "CRUD helpers",
        label_zh: "CRUD 辅助模块",
        questions: &[],
        artifacts: FASTAPI_CRUD_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "fastapi",
        name: "dependency",
        label_en: "FastAPI dependency",
        label_zh: "FastAPI 依赖",
        questions: &[],
        artifacts: FASTAPI_DEPENDENCY_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "fastapi",
        name: "middleware",
        label_en: "FastAPI middleware",
        label_zh: "FastAPI 中间件",
        questions: &[],
        artifacts: FASTAPI_MIDDLEWARE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
];

const AXUM_HANDLER_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/handler/{name_snake}.rs",
    template: "axum.handler",
    when: None,
}];

const AXUM_MODEL_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/model/{name_snake}.rs",
    template: "axum.model",
    when: None,
}];

const AXUM_REPOSITORY_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/repository/{name_snake}.rs",
    template: "axum.repository",
    when: None,
}];

const AXUM_SERVICE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/service/{name_snake}.rs",
    template: "axum.service",
    when: None,
}];

const AXUM_MIDDLEWARE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/middleware/{name_snake}.rs",
    template: "axum.middleware",
    when: None,
}];

const AXUM_ROUTER_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "src/router/{name_snake}.rs",
    template: "axum.router",
    when: None,
}];

const AXUM_RESOURCE_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "src/model/{name_snake}.rs",
        template: "axum.model",
        when: None,
    },
    ArtifactRule {
        path: "src/repository/{name_snake}.rs",
        template: "axum.repository",
        when: None,
    },
    ArtifactRule {
        path: "src/service/{name_snake}.rs",
        template: "axum.service",
        when: None,
    },
    ArtifactRule {
        path: "src/handler/{name_snake}.rs",
        template: "axum.handler",
        when: None,
    },
];

const AXUM_RESOURCE_REGISTRATIONS: &[RegistrationRule] = &[RegistrationRule {
    target: "src/router/mod.rs",
    marker: LANIA_MODS_MARKER,
    snippet: "pub mod {name_snake};",
}];

const AXUM_GENERATORS: &[Generator] = &[
    Generator {
        framework: "axum",
        name: "resource",
        label_en: "Axum resource (model + repository + service + handler)",
        label_zh: "Axum 资源（model + repository + service + handler）",
        questions: &[],
        artifacts: AXUM_RESOURCE_ARTIFACTS,
        registrations: AXUM_RESOURCE_REGISTRATIONS,
        wants_name: true,
    },
    Generator {
        framework: "axum",
        name: "handler",
        label_en: "Axum handler",
        label_zh: "Axum handler",
        questions: &[],
        artifacts: AXUM_HANDLER_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "axum",
        name: "model",
        label_en: "Axum model",
        label_zh: "Axum 模型",
        questions: &[],
        artifacts: AXUM_MODEL_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "axum",
        name: "repository",
        label_en: "Axum repository",
        label_zh: "Axum 仓储层",
        questions: &[],
        artifacts: AXUM_REPOSITORY_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "axum",
        name: "service",
        label_en: "Axum service",
        label_zh: "Axum 服务层",
        questions: &[],
        artifacts: AXUM_SERVICE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "axum",
        name: "middleware",
        label_en: "Axum middleware",
        label_zh: "Axum 中间件",
        questions: &[],
        artifacts: AXUM_MIDDLEWARE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "axum",
        name: "router",
        label_en: "Axum router",
        label_zh: "Axum 路由",
        questions: &[],
        artifacts: AXUM_ROUTER_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
];

const FLUTTER_PAGE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "lib/pages/{name_snake}_page.dart",
    template: "flutter.page",
    when: None,
}];

const FLUTTER_WIDGET_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "lib/widgets/{name_snake}.dart",
    template: "flutter.widget",
    when: None,
}];

const FLUTTER_CONTROLLER_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "lib/controllers/{name_snake}_controller.dart",
    template: "flutter.controller",
    when: None,
}];

const FLUTTER_MODEL_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "lib/models/{name_snake}.dart",
    template: "flutter.model",
    when: None,
}];

const FLUTTER_SERVICE_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "lib/services/{name_snake}_service.dart",
    template: "flutter.service",
    when: None,
}];

const FLUTTER_REPOSITORY_ARTIFACTS: &[ArtifactRule] = &[ArtifactRule {
    path: "lib/repositories/{name_snake}_repository.dart",
    template: "flutter.repository",
    when: None,
}];

/// `flutter feature`：页面 + 控制器 + 模型 + 服务一次生成。
const FLUTTER_FEATURE_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "lib/models/{name_snake}.dart",
        template: "flutter.model",
        when: None,
    },
    ArtifactRule {
        path: "lib/services/{name_snake}_service.dart",
        template: "flutter.service",
        when: None,
    },
    ArtifactRule {
        path: "lib/controllers/{name_snake}_controller.dart",
        template: "flutter.controller",
        when: None,
    },
    ArtifactRule {
        path: "lib/pages/{name_snake}_page.dart",
        template: "flutter.page",
        when: None,
    },
];

const FLUTTER_GENERATORS: &[Generator] = &[
    Generator {
        framework: "flutter",
        name: "feature",
        label_en: "Flutter feature (model + service + controller + page)",
        label_zh: "Flutter 功能（model + service + controller + page）",
        questions: &[],
        artifacts: FLUTTER_FEATURE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "flutter",
        name: "page",
        label_en: "Flutter page",
        label_zh: "Flutter 页面",
        questions: &[],
        artifacts: FLUTTER_PAGE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "flutter",
        name: "widget",
        label_en: "Flutter widget",
        label_zh: "Flutter 组件",
        questions: &[],
        artifacts: FLUTTER_WIDGET_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "flutter",
        name: "controller",
        label_en: "Flutter controller",
        label_zh: "Flutter 控制器",
        questions: &[],
        artifacts: FLUTTER_CONTROLLER_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "flutter",
        name: "model",
        label_en: "Dart model",
        label_zh: "Dart 模型",
        questions: &[],
        artifacts: FLUTTER_MODEL_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "flutter",
        name: "service",
        label_en: "Flutter service",
        label_zh: "Flutter 服务",
        questions: &[],
        artifacts: FLUTTER_SERVICE_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: "flutter",
        name: "repository",
        label_en: "Flutter repository",
        label_zh: "Flutter 仓储层",
        questions: &[],
        artifacts: FLUTTER_REPOSITORY_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
];

const COMPONENT_VARIANTS: &[&str] = &["rfc", "rcc", "v2", "v3", "svelte", "astro"];
const COMPONENT_VARIANT_QUESTION: GeneratorQuestion = GeneratorQuestion {
    field: "variant",
    label_en: "Component flavour",
    label_zh: "组件变体",
    kind: GeneratorQuestionKind::Select,
    required: true,
    choices: COMPONENT_VARIANTS,
};

const COMPONENT_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "src/components/{Name}.tsx",
        template: "component.rfc",
        when: Some("variant=rfc"),
    },
    ArtifactRule {
        path: "src/components/{Name}.tsx",
        template: "component.rcc",
        when: Some("variant=rcc"),
    },
    ArtifactRule {
        path: "src/components/{Name}.vue",
        template: "component.v3",
        when: Some("variant=v3"),
    },
    ArtifactRule {
        path: "src/components/{Name}.vue",
        template: "component.v2",
        when: Some("variant=v2"),
    },
    ArtifactRule {
        path: "src/components/{Name}.svelte",
        template: "component.svelte",
        when: Some("variant=svelte"),
    },
    ArtifactRule {
        path: "src/components/{Name}.astro",
        template: "component.astro",
        when: Some("variant=astro"),
    },
];

const CONFIG_VARIANTS: &[&str] = &[
    "eslint",
    "prettier",
    "stylelint",
    "tsconfig",
    "editorconfig",
    "gitignore",
    "commitizen",
];

const CONFIG_VARIANT_QUESTION: GeneratorQuestion = GeneratorQuestion {
    field: "variant",
    label_en: "Config flavour",
    label_zh: "配置类型",
    kind: GeneratorQuestionKind::Select,
    required: true,
    choices: CONFIG_VARIANTS,
};

const CONFIG_ARTIFACTS: &[ArtifactRule] = &[
    ArtifactRule {
        path: "eslint.config.js",
        template: "config.eslint",
        when: Some("variant=eslint"),
    },
    ArtifactRule {
        path: "prettier.config.js",
        template: "config.prettier",
        when: Some("variant=prettier"),
    },
    ArtifactRule {
        path: "stylelint.config.js",
        template: "config.stylelint",
        when: Some("variant=stylelint"),
    },
    ArtifactRule {
        path: "tsconfig.json",
        template: "config.tsconfig",
        when: Some("variant=tsconfig"),
    },
    ArtifactRule {
        path: ".editorconfig",
        template: "config.editorconfig",
        when: Some("variant=editorconfig"),
    },
    ArtifactRule {
        path: ".gitignore",
        template: "config.gitignore",
        when: Some("variant=gitignore"),
    },
    ArtifactRule {
        path: "cz.config.js",
        template: "config.commitizen",
        when: Some("variant=commitizen"),
    },
];

/// 通用生成器（框架无关）。探测不到框架时**只暴露这些**（§4.5.3 关键决策）。
pub const GENERIC_GENERATORS: &[Generator] = &[
    Generator {
        framework: GENERIC_FRAMEWORK,
        name: "component",
        label_en: "UI component",
        label_zh: "UI 组件",
        questions: &[COMPONENT_VARIANT_QUESTION],
        artifacts: COMPONENT_ARTIFACTS,
        registrations: &[],
        wants_name: true,
    },
    Generator {
        framework: GENERIC_FRAMEWORK,
        name: "config",
        label_en: "Tooling config file",
        label_zh: "工具链配置文件",
        questions: &[CONFIG_VARIANT_QUESTION],
        artifacts: CONFIG_ARTIFACTS,
        registrations: &[],
        wants_name: false,
    },
];

/// 全部生成器（框架专属 + 通用）。
pub fn all_generators() -> impl Iterator<Item = &'static Generator> {
    NEST_GENERATORS
        .iter()
        .chain(NEXT_GENERATORS.iter())
        .chain(GIN_GENERATORS.iter())
        .chain(FASTAPI_GENERATORS.iter())
        .chain(AXUM_GENERATORS.iter())
        .chain(FLUTTER_GENERATORS.iter())
        .chain(GENERIC_GENERATORS.iter())
}

/// 按框架查生成器；`framework` 为空时查通用生成器。
pub fn find_generator(framework: &str, name: &str) -> Option<&'static Generator> {
    all_generators().find(|generator| generator.framework == framework && generator.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_generator_emits_seven_files_in_rest_variant() {
        let generator = find_generator("nestjs", "resource").expect("resource generator");
        let rest = generator
            .artifacts
            .iter()
            .filter(|artifact| artifact.when.is_none())
            .count();
        assert_eq!(rest, 7, "B6 验收：resource 在 REST 变体下生成 7 个文件");
    }

    #[test]
    fn generator_table_is_the_only_definition() {
        let keys: Vec<(&str, &str)> = all_generators()
            .map(|generator| (generator.framework, generator.name))
            .collect();
        let unique: std::collections::HashSet<&(&str, &str)> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len(), "同一框架下生成器名不应重复");
    }

    #[test]
    fn generic_generators_are_framework_agnostic() {
        for generator in GENERIC_GENERATORS {
            assert_eq!(generator.framework, GENERIC_FRAMEWORK);
        }
        assert!(find_generator(GENERIC_FRAMEWORK, "component").is_some());
        assert!(find_generator(GENERIC_FRAMEWORK, "config").is_some());
    }

    /// B7 验收：每个框架至少有**一个多文件生成器**。
    #[test]
    fn every_framework_has_a_multi_file_generator() {
        const FRAMEWORKS: &[&str] = &["nestjs", "nextjs", "gin", "fastapi", "axum", "flutter"];
        for framework in FRAMEWORKS {
            let multi = all_generators()
                .filter(|generator| generator.framework == *framework)
                .filter(|generator| {
                    generator
                        .artifacts
                        .iter()
                        .filter(|artifact| artifact.when.is_none())
                        .count()
                        >= 2
                })
                .count();
            assert!(multi >= 1, "{framework} 缺少多文件生成器");
        }
    }

    #[test]
    fn framework_generator_names_cover_the_design_matrix() {
        for (framework, expected) in [
            (
                "nextjs",
                &[
                    "segment",
                    "page",
                    "layout",
                    "route",
                    "loading",
                    "error",
                    "not-found",
                    "component",
                    "hook",
                    "middleware",
                    "api",
                    "action",
                ][..],
            ),
            (
                "gin",
                &[
                    "handler",
                    "model",
                    "repository",
                    "service",
                    "middleware",
                    "router",
                ][..],
            ),
            (
                "fastapi",
                &[
                    "router",
                    "endpoint",
                    "model",
                    "schema",
                    "crud",
                    "dependency",
                    "middleware",
                ][..],
            ),
            (
                "axum",
                &[
                    "handler",
                    "model",
                    "repository",
                    "service",
                    "middleware",
                    "router",
                ][..],
            ),
            (
                "flutter",
                &[
                    "page",
                    "widget",
                    "controller",
                    "model",
                    "service",
                    "repository",
                ][..],
            ),
        ] {
            for name in expected {
                assert!(
                    find_generator(framework, name).is_some(),
                    "{framework} 缺少生成器 {name}"
                );
            }
        }
    }
}
