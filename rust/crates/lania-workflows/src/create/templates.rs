//! Rust 内建的模板声明（create/add 的 fallback 数据源）。
//!
//! 当 Node bridge 可用时，模板往往来自外部生态；但 bridge 不可用时，
//! `create`/`add` 仍需要一个“最小可用”的模板集合。
//!
//! 这个文件用静态数据结构声明：
//! - 模板名与问题（questions）
//! - 依赖与 dev 依赖
//! - 输出任务与文件规则（`PlannedFile` 的来源之一）
//!
//! 注意：这里的模板是“演示级/兜底级”，不追求覆盖所有场景，目标是保证工具可用。

use std::path::PathBuf;

use anyhow::Result;
use lania_fs::PlannedFile;
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub(super) struct DeclaredTemplate {
    pub(super) name: &'static str,
    pub(super) questions: &'static [DeclaredQuestion],
    pub(super) dependencies: &'static [&'static str],
    pub(super) dev_dependencies: &'static [&'static str],
    pub(super) output_tasks: &'static [&'static str],
    pub(super) files: &'static [DeclaredFileRule],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DeclaredQuestion {
    pub(super) name: &'static str,
    pub(super) question_type: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DeclaredFileRule {
    path: &'static str,
    content: &'static str,
}

const DECLARED_TEMPLATE_QUESTIONS: &[DeclaredQuestion] = &[
    DeclaredQuestion {
        name: "projectName",
        question_type: "input",
    },
    DeclaredQuestion {
        name: "packageManager",
        question_type: "select",
    },
];

const SPA_REACT_TEMPLATE: DeclaredTemplate = DeclaredTemplate {
    name: "spa-react",
    questions: DECLARED_TEMPLATE_QUESTIONS,
    dependencies: &["react", "vite"],
    dev_dependencies: &["typescript", "eslint"],
    output_tasks: &["write-files", "install-deps"],
    files: &[
        DeclaredFileRule {
            path: "src/main.tsx",
            content: "console.log(\"{{projectName}} react app\")\n",
        },
        DeclaredFileRule {
            path: "package.json",
            content: "{\n  \"name\": \"{{projectName}}\",\n  \"version\": \"0.1.0\"\n}\n",
        },
    ],
};

const SPA_VUE_TEMPLATE: DeclaredTemplate = DeclaredTemplate {
    name: "spa-vue",
    questions: DECLARED_TEMPLATE_QUESTIONS,
    dependencies: &["vue", "vite"],
    dev_dependencies: &["typescript", "eslint"],
    output_tasks: &["write-files", "install-deps"],
    files: &[
        DeclaredFileRule {
            path: "src/main.ts",
            content: "console.log(\"{{projectName}} vue app\")\n",
        },
        DeclaredFileRule {
            path: "package.json",
            content: "{\n  \"name\": \"{{projectName}}\",\n  \"version\": \"0.1.0\"\n}\n",
        },
    ],
};

const DECLARED_TEMPLATES: &[DeclaredTemplate] = &[SPA_REACT_TEMPLATE, SPA_VUE_TEMPLATE];

#[derive(Debug, Clone, Copy)]
pub(super) struct AddTemplateSpec {
    pub(super) name: &'static str,
    pub(super) label: &'static str,
}

pub(super) const ADD_TEMPLATE_SPECS: &[AddTemplateSpec] = &[
    AddTemplateSpec {
        name: "v2",
        label: "vue2模板组件",
    },
    AddTemplateSpec {
        name: "v3",
        label: "vue3模板组件",
    },
    AddTemplateSpec {
        name: "rcc",
        label: "react类组件",
    },
    AddTemplateSpec {
        name: "rfc",
        label: "react函数式组件",
    },
    AddTemplateSpec {
        name: "svelte",
        label: "svelte模板组件",
    },
    AddTemplateSpec {
        name: "astro",
        label: "astro组件",
    },
    AddTemplateSpec {
        name: "prettier",
        label: "prettier配置文件",
    },
    AddTemplateSpec {
        name: "eslint",
        label: "eslint配置文件",
    },
    AddTemplateSpec {
        name: "stylelint",
        label: "stylelint配置文件",
    },
    AddTemplateSpec {
        name: "editorconfig",
        label: "editorconfig配置文件",
    },
    AddTemplateSpec {
        name: "gitignore",
        label: ".gitignore文件",
    },
    AddTemplateSpec {
        name: "tsconfig",
        label: "tsconfig配置文件",
    },
    AddTemplateSpec {
        name: "commitizen",
        label: "commitizen配置文件",
    },
];

pub(super) fn declared_template(name: &str) -> Option<&'static DeclaredTemplate> {
    DECLARED_TEMPLATES
        .iter()
        .find(|template| template.name == name)
}

pub(super) fn declared_template_names() -> Vec<String> {
    DECLARED_TEMPLATES
        .iter()
        .map(|template| template.name.to_string())
        .collect()
}

pub(super) fn render_declared_template(
    template: &DeclaredTemplate,
    context: &Value,
) -> Result<Vec<PlannedFile>> {
    template
        .files
        .iter()
        .map(|rule| {
            Ok(PlannedFile {
                path: PathBuf::from(render_declared_text(rule.path, context)),
                content: render_declared_text(rule.content, context),
            })
        })
        .collect()
}

fn render_declared_text(template: &str, context: &Value) -> String {
    let mut rendered = template.to_string();
    for (key, value) in context_as_pairs(context) {
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), &value);
    }
    rendered
}

fn context_as_pairs(context: &Value) -> Vec<(String, String)> {
    context
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, value)| {
            let value = match value {
                Value::String(value) => value.clone(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                Value::Null => String::new(),
                _ => value.to_string(),
            };
            (key.clone(), value)
        })
        .collect()
}
