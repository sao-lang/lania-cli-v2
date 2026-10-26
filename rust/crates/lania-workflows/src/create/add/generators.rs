//! `add` 生成器表（§4.5.2 / §4.5.3）：**仓库内唯一一处生成器定义**。
//!
//! 三级结构：`框架` → `生成器` → `产物（N 个文件）`。
//! Node bridge 通过 bridge 方法 `addGenerator.list` 读取这张表，
//! TS 侧不再维护第二份平铺清单（§4.5.6）。
//!
//! 范围：B6 先落地 **nestjs**（第一个框架）与**通用生成器**（`component` / `config`），
//! 其余框架（nextjs / gin / fastapi / axum / flutter）留到 B7。

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
    NEST_GENERATORS.iter().chain(GENERIC_GENERATORS.iter())
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
}
