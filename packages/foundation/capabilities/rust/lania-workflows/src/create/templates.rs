//! Rust 内建的模板声明（create 的 fallback 数据源）。
//!
//! 当 Node bridge 可用时，模板往往来自外部生态；但 bridge 不可用时，
//! `create` 仍需要一个“最小可用”的模板集合。
//!
//! 这个文件用静态数据结构声明：
//! - 模板名与问题（questions）
//! - 依赖与 dev 依赖
//! - 输出任务与文件规则（`PlannedFile` 的来源之一）
//!
//! 注意：这里的模板是“演示级/兜底级”，不追求覆盖所有场景，目标是保证工具可用。
//! `add` 的生成器清单已迁到 `create/add/generators.rs`（单点定义），不在此文件。
//!
//! 非 Node 语言模板（`go-service` / `rust-lib` / `python-cli` / `dart-cli`）的产物
//! 与 `ts/packages/templates/src/templates/<name>/` 对齐：文件集合、条件分支、
//! `lan.config.ts` 内容按同一规则推导，保证「有 bridge / 无 bridge」结果一致。

use std::path::PathBuf;

use anyhow::Result;
use lania_fs::PlannedFile;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy)]
pub(super) struct DeclaredTemplate {
    pub(super) name: &'static str,
    pub(super) questions: &'static [DeclaredQuestion],
    pub(super) dependencies: &'static [&'static str],
    pub(super) dev_dependencies: &'static [&'static str],
    pub(super) output_tasks: &'static [&'static str],
    pub(super) files: &'static [DeclaredFileRule],
    /// 依赖随选项变化时使用；`None` 表示用上面的静态列表。
    pub(super) computed_dependencies: Option<fn(&Value) -> RenderedDependencies>,
}

/// 依赖随选项变化时的渲染结果。
#[derive(Debug, Clone, Default)]
pub(super) struct RenderedDependencies {
    pub(super) dependencies: Vec<String>,
    pub(super) dev_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DeclaredQuestion {
    pub(super) name: &'static str,
    pub(super) question_type: &'static str,
    /// 预置默认值，写成 JSON 字面量；`None` 时交给 `prompt_step_from_question` 兜底。
    ///
    /// 语言模板的 `lintTools` / `testTool` 必须给默认值：非交互场景（`--yes` /
    /// 无 TTY）下多选项拿不到答案，产物会退化成「没有任何工具配置」。
    pub(super) default: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DeclaredFileRule {
    path: &'static str,
    content: &'static str,
    /// 仅在条件成立时产出该文件，语法见 `condition_matches`。
    when: Option<&'static str>,
}

const DECLARED_TEMPLATE_QUESTIONS: &[DeclaredQuestion] = &[
    DeclaredQuestion {
        name: "projectName",
        question_type: "input",
        default: None,
    },
    DeclaredQuestion {
        name: "packageManager",
        question_type: "select",
        default: None,
    },
];

// ---------------------------------------------------------------------------
// Node 生态兜底模板
// ---------------------------------------------------------------------------

const SPA_REACT_TEMPLATE: DeclaredTemplate = DeclaredTemplate {
    name: "spa-react",
    questions: DECLARED_TEMPLATE_QUESTIONS,
    dependencies: &["react", "vite"],
    dev_dependencies: &["typescript", "eslint"],
    output_tasks: &["write-files", "install-deps"],
    computed_dependencies: None,
    files: &[
        DeclaredFileRule {
            path: "src/main.tsx",
            content: "console.log(\"{{projectName}} react app\")\n",
            when: None,
        },
        DeclaredFileRule {
            path: "package.json",
            content: "{\n  \"name\": \"{{projectName}}\",\n  \"version\": \"0.1.0\"\n}\n",
            when: None,
        },
    ],
};

const SPA_VUE_TEMPLATE: DeclaredTemplate = DeclaredTemplate {
    name: "spa-vue",
    questions: DECLARED_TEMPLATE_QUESTIONS,
    dependencies: &["vue", "vite"],
    dev_dependencies: &["typescript", "eslint"],
    output_tasks: &["write-files", "install-deps"],
    computed_dependencies: None,
    files: &[
        DeclaredFileRule {
            path: "src/main.ts",
            content: "console.log(\"{{projectName}} vue app\")\n",
            when: None,
        },
        DeclaredFileRule {
            path: "package.json",
            content: "{\n  \"name\": \"{{projectName}}\",\n  \"version\": \"0.1.0\"\n}\n",
            when: None,
        },
    ],
};

// ---------------------------------------------------------------------------
// 非 Node 语言模板（B12）
//
// 默认值与候选集合同 `ts/packages/templates/src/templates/<name>/options.ts` 一致。
// ---------------------------------------------------------------------------

const GO_SERVICE_QUESTIONS: &[DeclaredQuestion] = &[
    DeclaredQuestion {
        name: "projectName",
        question_type: "input",
        default: None,
    },
    DeclaredQuestion {
        name: "modulePath",
        question_type: "input",
        default: Some("\"example.com/demo\""),
    },
    DeclaredQuestion {
        name: "packageManager",
        question_type: "list",
        default: Some("\"go\""),
    },
    DeclaredQuestion {
        name: "lintTools",
        question_type: "checkbox",
        default: Some("[\"gofmt\", \"go-vet\"]"),
    },
    DeclaredQuestion {
        name: "testTool",
        question_type: "list",
        default: Some("\"go-test\""),
    },
    DeclaredQuestion {
        name: "skipInstall",
        question_type: "confirm",
        default: Some("true"),
    },
];

const RUST_LIB_QUESTIONS: &[DeclaredQuestion] = &[
    DeclaredQuestion {
        name: "projectName",
        question_type: "input",
        default: None,
    },
    DeclaredQuestion {
        name: "packageManager",
        question_type: "list",
        default: Some("\"cargo\""),
    },
    DeclaredQuestion {
        name: "lintTools",
        question_type: "checkbox",
        default: Some("[\"clippy\", \"rustfmt\"]"),
    },
    DeclaredQuestion {
        name: "testTool",
        question_type: "list",
        default: Some("\"builtin\""),
    },
];

const PYTHON_CLI_QUESTIONS: &[DeclaredQuestion] = &[
    DeclaredQuestion {
        name: "projectName",
        question_type: "input",
        default: None,
    },
    DeclaredQuestion {
        name: "packageManager",
        question_type: "list",
        default: Some("\"pip\""),
    },
    DeclaredQuestion {
        name: "lintTools",
        question_type: "checkbox",
        default: Some("[\"ruff\"]"),
    },
    DeclaredQuestion {
        name: "testTool",
        question_type: "list",
        default: Some("\"pytest\""),
    },
];

const DART_CLI_QUESTIONS: &[DeclaredQuestion] = &[
    DeclaredQuestion {
        name: "projectName",
        question_type: "input",
        default: None,
    },
    DeclaredQuestion {
        name: "packageManager",
        question_type: "list",
        default: Some("\"pub\""),
    },
    DeclaredQuestion {
        name: "lintTools",
        question_type: "checkbox",
        default: Some("[\"dart-analyze\"]"),
    },
    DeclaredQuestion {
        name: "testTool",
        question_type: "list",
        default: Some("\"test\""),
    },
];

const GO_SERVICE_TEMPLATE: DeclaredTemplate = DeclaredTemplate {
    name: "go-service",
    questions: GO_SERVICE_QUESTIONS,
    dependencies: &[],
    dev_dependencies: &[],
    output_tasks: &["write-files"],
    computed_dependencies: None,
    files: &[
        DeclaredFileRule {
            path: "go.mod",
            content: "module {{modulePath}}\n\ngo {{goVersion}}\n",
            when: None,
        },
        DeclaredFileRule {
            path: "main.go",
            content: "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"{{projectName}}\")\n}\n",
            when: None,
        },
        DeclaredFileRule {
            path: ".golangci.yml",
            content: "run:\n  timeout: 5m\n\nlinters:\n  enable:\n    - govet\n    - errcheck\n    - staticcheck\n",
            when: Some("lintTools:golangci-lint"),
        },
        DeclaredFileRule {
            path: "main_test.go",
            content: "package main\n\nimport \"testing\"\n\nfunc TestSmoke(t *testing.T) {\n}\n",
            when: Some("testTool:go-test"),
        },
        DeclaredFileRule {
            path: "lan.config.ts",
            content: "export default {\n  runtime: 'go',\n  lintTools: {{lintToolsJson}},\n};\n",
            when: None,
        },
        DeclaredFileRule {
            path: ".gitignore",
            content: "bin/\n",
            when: None,
        },
        DeclaredFileRule {
            path: "README.md",
            content: "# {{projectName}}\n\nGo service scaffolded by lania.\n",
            when: None,
        },
    ],
};

const RUST_LIB_TEMPLATE: DeclaredTemplate = DeclaredTemplate {
    name: "rust-lib",
    questions: RUST_LIB_QUESTIONS,
    dependencies: &[],
    dev_dependencies: &[],
    output_tasks: &["write-files"],
    computed_dependencies: None,
    files: &[
        DeclaredFileRule {
            path: "Cargo.toml",
            content:
                "[package]\nname = \"{{packageName}}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            when: None,
        },
        DeclaredFileRule {
            path: "src/lib.rs",
            content: "pub fn greet() -> &'static str {\n    \"{{projectName}}\"\n}\n",
            when: None,
        },
        DeclaredFileRule {
            path: "rustfmt.toml",
            content: "edition = \"2021\"\nmax_width = 100\n",
            when: Some("lintTools:rustfmt"),
        },
        DeclaredFileRule {
            path: "clippy.toml",
            content: "# Project-specific clippy configuration.\n",
            when: Some("lintTools:clippy"),
        },
        DeclaredFileRule {
            path: "tests/smoke.rs",
            content: "#[test]\nfn smoke() {\n    assert!(true);\n}\n",
            when: Some("testTool:builtin"),
        },
        DeclaredFileRule {
            path: "lan.config.ts",
            content:
                "export default {\n  runtime: 'rust',\n  lintTools: {{lintToolsJson}},\n};\n",
            when: None,
        },
        DeclaredFileRule {
            path: ".gitignore",
            content: "target/\n",
            when: None,
        },
        DeclaredFileRule {
            path: "README.md",
            content: "# {{projectName}}\n\nRust library scaffolded by lania.\n",
            when: None,
        },
    ],
};

/// `python-cli` 的 dev 依赖随 lint / 测试选项变化（对齐 TS 的 `dependencies.ts`）。
fn python_cli_dependencies(context: &Value) -> RenderedDependencies {
    let mut dev_dependencies: Vec<String> = context_string_list(context, "lintTools")
        .into_iter()
        .filter(|tool| matches!(tool.as_str(), "ruff" | "black" | "mypy" | "flake8"))
        .collect();
    if context_string(context, "testTool").as_deref() == Some("pytest") {
        dev_dependencies.push("pytest".into());
    }
    dev_dependencies.sort();
    dev_dependencies.dedup();
    RenderedDependencies {
        dependencies: Vec::new(),
        dev_dependencies,
    }
}

const PYTHON_CLI_TEMPLATE: DeclaredTemplate = DeclaredTemplate {
    name: "python-cli",
    questions: PYTHON_CLI_QUESTIONS,
    dependencies: &[],
    dev_dependencies: &[],
    output_tasks: &["write-files"],
    computed_dependencies: Some(python_cli_dependencies),
    files: &[
        DeclaredFileRule {
            path: "pyproject.toml",
            content: "[project]\nname = \"{{packageName}}\"\nversion = \"0.1.0\"\nrequires-python = \">=3.10\"\ndependencies = []\n",
            when: None,
        },
        DeclaredFileRule {
            path: "src/main.py",
            content: "def main() -> None:\n    print(\"{{projectName}}\")\n\n\nif __name__ == \"__main__\":\n    main()\n",
            when: None,
        },
        DeclaredFileRule {
            path: "requirements.txt",
            content: "",
            when: None,
        },
        DeclaredFileRule {
            path: "requirements-dev.txt",
            content: "{{devRequirementList}}",
            when: Some("packageManager!=poetry"),
        },
        DeclaredFileRule {
            path: "ruff.toml",
            content: "line-length = 100\ntarget-version = \"py310\"\n",
            when: Some("lintTools:ruff"),
        },
        DeclaredFileRule {
            path: ".flake8",
            content: "[flake8]\nmax-line-length = 100\nextend-ignore = E203\n",
            when: Some("lintTools:flake8"),
        },
        DeclaredFileRule {
            path: "mypy.ini",
            content: "[mypy]\npython_version = 3.10\nstrict = true\n",
            when: Some("lintTools:mypy"),
        },
        DeclaredFileRule {
            path: "tests/test_main.py",
            content: "def test_smoke() -> None:\n    assert True\n",
            when: Some("testTool:pytest"),
        },
        DeclaredFileRule {
            path: "lan.config.ts",
            content: "export default {\n  runtime: 'python',\n  lintTools: {{lintToolsJson}},\n};\n",
            when: None,
        },
        DeclaredFileRule {
            path: ".gitignore",
            content: "dist/\n__pycache__/\n",
            when: None,
        },
        DeclaredFileRule {
            path: "README.md",
            content: "# {{projectName}}\n\nPython CLI scaffolded by lania.\n",
            when: None,
        },
    ],
};

const DART_CLI_TEMPLATE: DeclaredTemplate = DeclaredTemplate {
    name: "dart-cli",
    questions: DART_CLI_QUESTIONS,
    dependencies: &[],
    dev_dependencies: &[],
    output_tasks: &["write-files"],
    computed_dependencies: None,
    files: &[
        DeclaredFileRule {
            path: "pubspec.yaml",
            content: "name: {{packageName}}\ndescription: A Dart CLI scaffolded by lania.\nenvironment:\n  sdk: \">=3.0.0 <4.0.0\"\n\ndev_dependencies:\n{{dartDevDependencies}}\n",
            when: None,
        },
        DeclaredFileRule {
            path: "bin/main.dart",
            content: "void main() {\n  print('{{projectName}}');\n}\n",
            when: None,
        },
        DeclaredFileRule {
            path: "analysis_options.yaml",
            content: "include: package:lints/recommended.yaml\n",
            when: Some("lintTools:dart-analyze"),
        },
        DeclaredFileRule {
            path: "test/main_test.dart",
            content: "import 'package:test/test.dart';\n\nvoid main() {\n  test('smoke', () {\n    expect(true, isTrue);\n  });\n}\n",
            when: Some("testTool:test"),
        },
        DeclaredFileRule {
            path: "lan.config.ts",
            content: "export default {\n  runtime: 'dart',\n  lintTools: {{lintToolsJson}},\n};\n",
            when: None,
        },
        DeclaredFileRule {
            path: ".gitignore",
            content: ".dart_tool/\n",
            when: None,
        },
        DeclaredFileRule {
            path: "README.md",
            content: "# {{projectName}}\n\nDart CLI scaffolded by lania.\n",
            when: None,
        },
    ],
};

const DECLARED_TEMPLATES: &[DeclaredTemplate] = &[
    SPA_REACT_TEMPLATE,
    SPA_VUE_TEMPLATE,
    GO_SERVICE_TEMPLATE,
    RUST_LIB_TEMPLATE,
    PYTHON_CLI_TEMPLATE,
    DART_CLI_TEMPLATE,
];

pub(super) fn declared_template(name: &str) -> Option<&'static DeclaredTemplate> {
    DECLARED_TEMPLATES
        .iter()
        .find(|template| template.name == name)
}

/// Rust 内建模板名 —— 同时是 `create --template` 候选里的本地子集。
///
/// 不含 `toolkit` / `toolkit-monorepo` / `nestjs` / `express`：它们只有 bridge
/// 模板运行时才能渲染，列进兜底清单等于给用户一个必然失败的选项。
pub fn declared_template_names() -> Vec<String> {
    DECLARED_TEMPLATES
        .iter()
        .map(|template| template.name.to_string())
        .collect()
}

pub(super) fn render_declared_template(
    template: &DeclaredTemplate,
    context: &Value,
) -> Result<Vec<PlannedFile>> {
    let context = with_defaults(template.name, context);
    template
        .files
        .iter()
        .filter(|rule| condition_matches(rule.when, &context))
        .map(|rule| {
            Ok(PlannedFile {
                path: PathBuf::from(render_declared_text(rule.path, &context)),
                content: render_declared_text(rule.content, &context),
            })
        })
        .collect()
}

/// 声明模板的依赖列表；选项相关模板（如 `python-cli`）走 `computed_dependencies`。
pub(super) fn declared_dependencies(
    template: &DeclaredTemplate,
    context: &Value,
) -> (Vec<String>, Vec<String>) {
    match template.computed_dependencies {
        Some(render) => {
            let rendered = render(&with_defaults(template.name, context));
            (rendered.dependencies, rendered.dev_dependencies)
        }
        None => (
            template
                .dependencies
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            template
                .dev_dependencies
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        ),
    }
}

/// `when` 语法：
/// - `key:value` —— 列表字段包含 `value`（如 `lintTools:ruff`、`testTool:pytest`）
/// - `key=value` —— 字段等于 `value`
/// - `key!=value` —— 字段不等于 `value`（缺字段视为不相等，与 TS 的 `?? 默认值` 语义一致）
fn condition_matches(condition: Option<&'static str>, context: &Value) -> bool {
    let Some(condition) = condition else {
        return true;
    };
    if let Some((key, expected)) = condition.split_once(':') {
        return context_string_list(context, key)
            .iter()
            .any(|value| value == expected);
    }
    if let Some((key, expected)) = condition.split_once("!=") {
        return context_string(context, key).as_deref() != Some(expected);
    }
    if let Some((key, expected)) = condition.split_once('=') {
        return context_string(context, key).as_deref() == Some(expected);
    }
    true
}

/// 补齐语言模板的派生字段：`packageName` / `modulePath` / `goVersion` /
/// `lintToolsJson` / `devRequirementList` / `dartDevDependencies`。
///
/// 这些值在 TS 侧由 `options.ts` / `template-runtime.ts` 推导，兜底路径必须复刻，
/// 否则占位符会原样留在产物里。
fn with_defaults(name: &str, context: &Value) -> Value {
    let mut context = if context.is_object() {
        context.clone()
    } else {
        Value::Object(serde_json::Map::new())
    };
    let project_name = context_string(&context, "projectName")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "lania-app".to_string());
    let package_name = context_string(&context, "packageName")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| project_name.clone());
    let module_path = context_string(&context, "modulePath")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "example.com/demo".to_string());
    let lint_tools = visible_lint_tools(name, &context);
    let lint_tools_json = json!(lint_tools);
    let dev_requirement_list = context_string_list(&context, "devDependencyList")
        .iter()
        .map(|dependency| format!("{dependency}\n"))
        .collect::<String>();
    let dart_dev_dependencies = {
        let mut entries = Vec::new();
        if lint_tools.iter().any(|tool| tool == "dart-analyze") {
            entries.push("  lints: ^4.0.0");
        }
        if context_string(&context, "testTool").as_deref() == Some("test") {
            entries.push("  test: ^1.25.0");
        }
        if entries.is_empty() {
            "  {}\n".to_string()
        } else {
            format!("{}\n", entries.join("\n"))
        }
    };
    let object = context
        .as_object_mut()
        .expect("context is an object after normalization");
    object.insert("projectName".into(), Value::String(project_name));
    object.insert("packageName".into(), Value::String(package_name));
    object.insert("modulePath".into(), Value::String(module_path));
    object
        .entry("goVersion")
        .or_insert_with(|| Value::String("1.22".into()));
    object.insert("lintToolsJson".into(), lint_tools_json);
    object.insert(
        "devRequirementList".into(),
        Value::String(dev_requirement_list),
    );
    object.insert(
        "dartDevDependencies".into(),
        Value::String(dart_dev_dependencies),
    );
    context
}

/// 写入项目 `lan.config.ts` 的 lint 工具集合。
///
/// 两处偏离 TS 模板，都是为了让产物在兜底场景下**可运行**：
/// - `rust-lib` 只留 clippy —— `rustfmt` 不是 `cargo` 子命令，host 无法按
///   `{id, binary, args}` 调用它；
/// - `go-service` 去掉 `gofmt` —— 同样是独立二进制。`go-vet` 保留，它在 lint
///   adaptor 枚举内，toolchain 里就是 `go vet`。
fn visible_lint_tools(name: &str, context: &Value) -> Vec<String> {
    let mut tools = context_string_list(context, "lintTools");
    match name {
        "rust-lib" => tools.retain(|tool| tool == "clippy"),
        "go-service" => tools.retain(|tool| tool != "gofmt"),
        _ => {}
    }
    tools
}

fn context_string(context: &Value, key: &str) -> Option<String> {
    match context.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Bool(value)) => Some(value.to_string()),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn context_string_list(context: &Value, key: &str) -> Vec<String> {
    match context.get(key) {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(|text| text.to_string()))
            .collect(),
        Some(Value::String(value)) => value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(|item| item.to_string())
            .collect(),
        _ => Vec::new(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn render(name: &str, context: Value) -> Vec<PlannedFile> {
        let template = declared_template(name).expect("declared template");
        render_declared_template(template, &context).expect("render")
    }

    fn paths(files: &[PlannedFile]) -> Vec<String> {
        files
            .iter()
            .map(|file| file.path.display().to_string().replace('\\', "/"))
            .collect()
    }

    fn content<'a>(files: &'a [PlannedFile], path: &str) -> &'a str {
        &files
            .iter()
            .find(|file| file.path.display().to_string().replace('\\', "/") == path)
            .expect("file exists")
            .content
    }

    #[test]
    fn declares_the_four_language_templates() {
        let names = declared_template_names();
        for name in ["go-service", "rust-lib", "python-cli", "dart-cli"] {
            assert!(names.iter().any(|item| item == name), "missing {name}");
        }
    }

    #[test]
    fn question_defaults_are_valid_json() {
        for template in DECLARED_TEMPLATES {
            for question in template.questions {
                let Some(raw) = question.default else {
                    continue;
                };
                serde_json::from_str::<Value>(raw).unwrap_or_else(|error| {
                    panic!(
                        "{}/{} default is not JSON: {error}",
                        template.name, question.name
                    )
                });
            }
        }
    }

    #[test]
    fn go_service_renders_manifest_and_config() {
        let files = render(
            "go-service",
            json!({
                "projectName": "demo",
                "modulePath": "example.com/demo",
                "lintTools": ["golangci-lint", "gofmt", "go-vet"],
                "testTool": "go-test",
            }),
        );
        assert_eq!(
            paths(&files),
            vec![
                "go.mod",
                "main.go",
                ".golangci.yml",
                "main_test.go",
                "lan.config.ts",
                ".gitignore",
                "README.md",
            ]
        );
        assert_eq!(
            content(&files, "go.mod"),
            "module example.com/demo\n\ngo 1.22\n"
        );
        // gofmt 是独立二进制，不是 `go` 子命令，写进配置 host 也调不动。
        assert_eq!(
            content(&files, "lan.config.ts"),
            "export default {\n  runtime: 'go',\n  lintTools: [\"golangci-lint\",\"go-vet\"],\n};\n"
        );
    }

    #[test]
    fn go_service_skips_optional_files() {
        let files = render(
            "go-service",
            json!({ "projectName": "demo", "lintTools": ["gofmt"], "testTool": "skip" }),
        );
        let paths = paths(&files);
        assert!(!paths.contains(&".golangci.yml".to_string()));
        assert!(!paths.contains(&"main_test.go".to_string()));
        assert_eq!(
            content(&files, "go.mod"),
            "module example.com/demo\n\ngo 1.22\n",
            "missing modulePath must fall back to example.com/demo"
        );
    }

    #[test]
    fn rust_lib_keeps_only_clippy_in_lan_config() {
        let files = render(
            "rust-lib",
            json!({
                "projectName": "demo",
                "packageName": "demo-lib",
                "lintTools": ["clippy", "rustfmt"],
                "testTool": "builtin",
            }),
        );
        assert_eq!(
            paths(&files),
            vec![
                "Cargo.toml",
                "src/lib.rs",
                "rustfmt.toml",
                "clippy.toml",
                "tests/smoke.rs",
                "lan.config.ts",
                ".gitignore",
                "README.md",
            ]
        );
        assert!(content(&files, "Cargo.toml").contains("name = \"demo-lib\""));
        assert_eq!(
            content(&files, "lan.config.ts"),
            "export default {\n  runtime: 'rust',\n  lintTools: [\"clippy\"],\n};\n"
        );
    }

    #[test]
    fn python_cli_renders_conditional_files_and_dev_dependencies() {
        let files = render(
            "python-cli",
            json!({
                "projectName": "demo",
                "packageManager": "pip",
                "lintTools": ["ruff", "mypy"],
                "testTool": "pytest",
            }),
        );
        let paths = paths(&files);
        assert!(paths.contains(&"ruff.toml".to_string()));
        assert!(paths.contains(&"mypy.ini".to_string()));
        assert!(!paths.contains(&".flake8".to_string()));
        assert!(paths.contains(&"tests/test_main.py".to_string()));

        let template = declared_template("python-cli").expect("template");
        let (dependencies, dev_dependencies) = declared_dependencies(
            template,
            &json!({ "packageManager": "pip", "lintTools": ["ruff", "mypy"], "testTool": "pytest" }),
        );
        assert!(dependencies.is_empty());
        assert_eq!(dev_dependencies, vec!["mypy", "pytest", "ruff"]);
    }

    #[test]
    fn python_cli_poetry_skips_requirements_dev() {
        let files = render(
            "python-cli",
            json!({ "projectName": "demo", "packageManager": "poetry", "lintTools": [], "testTool": "skip" }),
        );
        let paths = paths(&files);
        assert!(paths.contains(&"requirements.txt".to_string()));
        assert!(!paths.contains(&"requirements-dev.txt".to_string()));
        assert!(!paths.contains(&"tests/test_main.py".to_string()));
    }

    #[test]
    fn dart_cli_renders_pubspec_dev_dependencies() {
        let files = render(
            "dart-cli",
            json!({
                "projectName": "demo",
                "packageName": "demo_cli",
                "lintTools": ["dart-analyze"],
                "testTool": "test",
            }),
        );
        assert_eq!(
            paths(&files),
            vec![
                "pubspec.yaml",
                "bin/main.dart",
                "analysis_options.yaml",
                "test/main_test.dart",
                "lan.config.ts",
                ".gitignore",
                "README.md",
            ]
        );
        let pubspec = content(&files, "pubspec.yaml");
        assert!(pubspec.contains("name: demo_cli"));
        assert!(pubspec.contains("  lints: ^4.0.0"));
        assert!(pubspec.contains("  test: ^1.25.0"));
    }

    #[test]
    fn dart_cli_without_tools_emits_empty_dev_dependencies() {
        let files = render(
            "dart-cli",
            json!({ "projectName": "demo", "lintTools": [], "testTool": "skip" }),
        );
        let paths = paths(&files);
        assert!(!paths.contains(&"analysis_options.yaml".to_string()));
        assert!(!paths.contains(&"test/main_test.dart".to_string()));
        assert!(content(&files, "pubspec.yaml").contains("dev_dependencies:\n  {}\n"));
    }

    #[test]
    fn project_name_defaults_when_absent() {
        let files = render("go-service", json!({}));
        assert!(content(&files, "main.go").contains("lania-app"));
    }
}
