//! 内置项目模板的展示文案（唯一事实源）。
//!
//! `create` 命令的 `--template` 候选来自 `BRIDGE_TEMPLATES` +
//! `declared_template_names()`（见 `packages/commands/create/rust/src/lib.rs`），
//! 当前共 10 个：前端 2 + Node 4 + Go/Rust/Python/Dart 各 1。
//!
//! 此前 `create/rust/src/template.rs` 与 `output/human/template.rs` 各维护了一份
//! 只为前 4 个模板写简介的 match；新增的 6 个多运行时模板因此没有简介。
//! 现在两处都调用本模块，数据只写一遍，且新增模板不会再次落后。

/// 与 `lan create --template` 的候选保持一致的 10 个内置模板。
pub const BUILTIN_TEMPLATES: &[&str] = &[
    "spa-react",
    "spa-vue",
    "toolkit",
    "toolkit-monorepo",
    "nestjs",
    "express",
    "go-service",
    "rust-lib",
    "python-cli",
    "dart-cli",
];

struct TemplateCopy {
    name: &'static str,
    zh_summary: &'static str,
    en_summary: &'static str,
    zh_highlights: &'static [&'static str],
    en_highlights: &'static [&'static str],
}

const CATALOG: &[TemplateCopy] = &[
    TemplateCopy {
        name: "spa-react",
        zh_summary: "基于 React 的单页应用项目模板，适合快速启动前端业务项目。",
        en_summary: "A React single-page application template for quickly bootstrapping frontend projects.",
        zh_highlights: &[
            "使用 React 组件结构，默认包含 `App` 与入口文件",
            "支持 TypeScript 或 JavaScript",
            "支持 Vite 或 Webpack 构建",
            "可选 CSS 预处理器、Tailwind 和常见 lint 工具",
        ],
        en_highlights: &[
            "Uses a React component structure with an `App` entry",
            "Supports TypeScript or JavaScript",
            "Supports Vite or Webpack",
            "Optional CSS preprocessors, Tailwind, and common lint tools",
        ],
    },
    TemplateCopy {
        name: "spa-vue",
        zh_summary: "基于 Vue 3 的单文件组件项目模板，适合快速启动前端业务项目。",
        en_summary: "A Vue 3 single-file component template for quickly bootstrapping frontend projects.",
        zh_highlights: &[
            "使用 Vue 3 + 单文件组件（`App.vue`）结构",
            "支持 TypeScript 或 JavaScript",
            "支持 Vite 或 Webpack 构建",
            "可选 CSS 预处理器、Tailwind 和常见 lint 工具",
        ],
        en_highlights: &[
            "Uses Vue 3 with a single-file component (`App.vue`) structure",
            "Supports TypeScript or JavaScript",
            "Supports Vite or Webpack",
            "Optional CSS preprocessors, Tailwind, and common lint tools",
        ],
    },
    TemplateCopy {
        name: "toolkit",
        zh_summary: "面向工具库或 SDK 的单包项目模板，适合构建可复用的前端工具模块。",
        en_summary: "A single-package toolkit template for building reusable utilities or SDKs.",
        zh_highlights: &[
            "单包目录结构，适合工具库或 SDK",
            "默认基于 Vite 构建",
            "支持 TypeScript，可选 Vitest",
            "内置常见工程化与发布配置",
        ],
        en_highlights: &[
            "Single-package structure for utilities or SDKs",
            "Built around Vite by default",
            "Supports TypeScript with optional Vitest",
            "Includes common engineering and release setup",
        ],
    },
    TemplateCopy {
        name: "toolkit-monorepo",
        zh_summary: "面向多包协作场景的 monorepo 模板，适合组件库、工具集或多模块工程。",
        en_summary: "A monorepo template for multi-package toolkits, component libraries, or modular projects.",
        zh_highlights: &[
            "多包目录结构，默认包含 `packages/core`",
            "支持 pnpm workspace 与 changesets 发布流程",
            "适合组件库、工具集或多模块协作",
            "内置常见工程化与提交规范配置",
        ],
        en_highlights: &[
            "Multi-package structure with a default `packages/core` workspace",
            "Supports pnpm workspace and changesets release flow",
            "Suitable for component libraries, toolkits, or modular repos",
            "Includes common engineering and commit tooling",
        ],
    },
    TemplateCopy {
        name: "nestjs",
        zh_summary: "基于 NestJS 的服务端应用模板，适合构建模块化的 Node.js 后端服务。",
        en_summary: "A NestJS server application template for building modular Node.js backends.",
        zh_highlights: &[
            "模块化 controller / service / module 结构",
            "支持 npm / pnpm / yarn / bun",
            "内置 Vitest 测试与常见 lint 工具",
            "产出项目配置统一为 lan.config.ts",
        ],
        en_highlights: &[
            "Modular controller / service / module structure",
            "Supports npm / pnpm / yarn / bun",
            "Includes Vitest tests and common lint tooling",
            "Generated projects use lan.config.ts",
        ],
    },
    TemplateCopy {
        name: "express",
        zh_summary: "基于 Express 的轻量服务端应用模板，适合快速搭建 HTTP API。",
        en_summary: "A lightweight Express server application template for building HTTP APIs quickly.",
        zh_highlights: &[
            "极简路由与中间件结构",
            "支持 npm / pnpm / yarn / bun",
            "内置 Vitest 测试与常见 lint 工具",
            "产出项目配置统一为 lan.config.ts",
        ],
        en_highlights: &[
            "Minimal router and middleware structure",
            "Supports npm / pnpm / yarn / bun",
            "Includes Vitest tests and common lint tooling",
            "Generated projects use lan.config.ts",
        ],
    },
    TemplateCopy {
        name: "go-service",
        zh_summary: "面向 Go 的服务或命令行项目模板，支持 go mod tidy 与常见 lint / 测试工具。",
        en_summary: "A Go service/CLI project template with go mod tidy and common lint/test tooling.",
        zh_highlights: &[
            "生成 go.mod 与 main.go 入口",
            "包管理器使用 go mod tidy",
            "可选 golangci-lint 与 go test",
            "Rust 声明式兜底，无需 Node 依赖也能创建",
        ],
        en_highlights: &[
            "Generates go.mod and a main.go entry point",
            "Dependencies resolved with go mod tidy",
            "Optional golangci-lint and go test",
            "Rust declarative fallback works without Node",
        ],
    },
    TemplateCopy {
        name: "rust-lib",
        zh_summary: "面向 Rust 库的项目模板，支持 cargo 构建、clippy 与单元测试。",
        en_summary: "A Rust library project template with cargo build, clippy and unit tests.",
        zh_highlights: &[
            "生成 Cargo.toml 与 src/lib.rs",
            "包管理器使用 cargo fetch",
            "可选 clippy / rustfmt 与 cargo test",
            "Rust 声明式兜底，无需 Node 依赖也能创建",
        ],
        en_highlights: &[
            "Generates Cargo.toml and src/lib.rs",
            "Dependencies resolved with cargo fetch",
            "Optional clippy / rustfmt and cargo test",
            "Rust declarative fallback works without Node",
        ],
    },
    TemplateCopy {
        name: "python-cli",
        zh_summary: "面向 Python 命令行项目的模板，支持 pip / uv / poetry、ruff 与 pytest。",
        en_summary: "A Python CLI project template supporting pip/uv/poetry, ruff and pytest.",
        zh_highlights: &[
            "生成 pyproject.toml 与 src/main.py",
            "包管理器可选 pip / uv / poetry",
            "可选 ruff format 与 pytest",
            "Rust 声明式兜底，无需 Node 依赖也能创建",
        ],
        en_highlights: &[
            "Generates pyproject.toml and src/main.py",
            "Choose pip / uv / poetry as package manager",
            "Optional ruff format and pytest",
            "Rust declarative fallback works without Node",
        ],
    },
    TemplateCopy {
        name: "dart-cli",
        zh_summary: "面向 Dart 命令行项目的模板，支持 dart pub get、lints 与 test。",
        en_summary: "A Dart CLI project template with dart pub get, lints and test.",
        zh_highlights: &[
            "生成 pubspec.yaml 与 bin/main.dart",
            "包管理器使用 dart pub get",
            "可选 dart format / lints 与 dart test",
            "Rust 声明式兜底，无需 Node 依赖也能创建",
        ],
        en_highlights: &[
            "Generates pubspec.yaml and bin/main.dart",
            "Dependencies resolved with dart pub get",
            "Optional dart format / lints and dart test",
            "Rust declarative fallback works without Node",
        ],
    },
];

fn entry(name: &str) -> Option<&'static TemplateCopy> {
    CATALOG.iter().find(|item| item.name == name)
}

fn is_zh(locale: &str) -> bool {
    locale.eq_ignore_ascii_case("zh") || locale.starts_with("zh-")
}

/// 模板简介；未知模板回退为原名，避免误导。
pub fn template_summary(name: &str, locale: &str) -> String {
    match entry(name) {
        Some(item) if is_zh(locale) => item.zh_summary.to_string(),
        Some(item) => item.en_summary.to_string(),
        None => name.to_string(),
    }
}

/// 模板关键特征；未知模板返回空列表。
pub fn template_highlights(name: &str, locale: &str) -> Vec<String> {
    let Some(item) = entry(name) else {
        return Vec::new();
    };
    let source = if is_zh(locale) {
        item.zh_highlights
    } else {
        item.en_highlights
    };
    source.iter().map(|line| (*line).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_every_builtin_template() {
        for name in BUILTIN_TEMPLATES {
            assert!(entry(name).is_some(), "missing catalog entry for {name}");
            assert_ne!(template_summary(name, "zh"), *name);
            assert_ne!(template_summary(name, "en"), *name);
            assert!(!template_highlights(name, "zh").is_empty());
            assert!(!template_highlights(name, "en").is_empty());
        }
    }

    #[test]
    fn locale_switches_copy_and_unknown_template_falls_back() {
        assert!(template_summary("go-service", "zh").contains("Go"));
        assert!(template_summary("go-service", "en").starts_with("A Go"));
        assert_eq!(template_summary("custom", "zh"), "custom");
        assert!(template_highlights("custom", "zh").is_empty());
    }
}
