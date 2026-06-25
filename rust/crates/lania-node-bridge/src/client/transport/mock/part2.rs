//! mock transport 的第二部分：补全模板、脚手架等更高层的方法。
//!
//! 这里的 mock 数据通常更偏“产品演示/脚手架默认值”：
//! - 模板列表
//! - 模板问题
//! - 依赖、输出任务、渲染结果
//!
//! 和真实 bridge 相比，它更像“静态样本返回器”，重点是让 Rust 侧调用链不断。

use crate::client::NodeBridgeClient;
use crate::protocol::{BridgeExchange, BridgeRequest};

use crate::protocol::{BridgeError, BridgeEvent, BridgeEventMethod, BridgeResponse};

pub(super) fn handle_part2(_client: &NodeBridgeClient, request: BridgeRequest) -> BridgeExchange {
    match request.method.as_str() {
        "template.list" => BridgeExchange {
            response: BridgeResponse {
                id: request.id,
                result: Some(serde_json::json!({
                    "templates": ["spa-react", "spa-vue", "toolkit", "toolkit-monorepo"],
                })),
                error: None,
            },
            events: vec![],
        },
        "template.getQuestions" => {
            let template = request.params["template"].as_str().unwrap_or("spa-react");
            let options = request.params["options"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            let skip_git = options
                .get("skipGit")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let zh = options
                .get("locale")
                .and_then(|value| value.as_str())
                .is_some_and(|locale| locale.eq_ignore_ascii_case("zh"));
            let package_manager = options
                .get("packageManager")
                .and_then(|value| value.as_str());
            let package_manager_message = if zh {
                "请选择包管理器："
            } else {
                "Please select a packaging tool:"
            };
            let repository_message = if zh {
                "请输入仓库地址："
            } else {
                "Please input the repository:"
            };
            let css_processor_message = if zh {
                "请选择 CSS 预处理器："
            } else {
                "Please select a css processor:"
            };
            let css_tools_message = if zh {
                "请选择 CSS 工具："
            } else {
                "Please select a css tool:"
            };
            let lint_tools_message = if zh {
                "请选择 lint 工具："
            } else {
                "Please select the lint tools:"
            };
            let build_tool_message = if zh {
                "请选择构建工具："
            } else {
                "Please select a build tool:"
            };
            let maybe_package_manager_question = || {
                if package_manager.is_some() {
                    None
                } else {
                    Some(serde_json::json!({
                        "name": "packageManager",
                        "message": package_manager_message,
                        "type": "select",
                        "choices": ["pnpm", "npm", "yarn", "bun"],
                        "default": "npm"
                    }))
                }
            };
            let repository_question = || {
                serde_json::json!({
                    "name": "repository",
                    "message": repository_message,
                    "type": "input",
                    "default": ""
                })
            };
            let questions = match template {
                "toolkit" => {
                    let mut questions = vec![
                        serde_json::json!({
                            "name": "lintTools",
                            "type": "checkbox",
                            "choices": ["eslint", "prettier", "commitlint", "editorconfig"],
                            "default": ["eslint", "prettier", "commitlint", "editorconfig"]
                        }),
                        serde_json::json!({
                            "name": "unitTestTool",
                            "type": "list",
                            "choices": ["vitest", "skip"],
                            "default": "vitest"
                        }),
                    ];
                    if let Some(question) = maybe_package_manager_question() {
                        questions.push(question);
                    }
                    if !skip_git {
                        questions.push(repository_question());
                    }
                    serde_json::Value::Array(questions)
                }
                "toolkit-monorepo" => {
                    let mut questions = vec![
                        serde_json::json!({
                            "name": "lintTools",
                            "type": "checkbox",
                            "choices": ["eslint", "prettier", "commitlint", "editorconfig"],
                            "default": ["eslint", "prettier", "commitlint", "editorconfig"]
                        }),
                        serde_json::json!({
                            "name": "unitTestTool",
                            "type": "select",
                            "choices": ["vitest"],
                            "default": "vitest"
                        }),
                    ];
                    if let Some(question) = maybe_package_manager_question() {
                        questions.push(question);
                    }
                    serde_json::Value::Array(questions)
                }
                "spa-vue" => {
                    let mut questions = vec![
                        serde_json::json!({
                            "name": "cssProcessor",
                            "message": css_processor_message,
                            "type": "list",
                            "choices": ["css", "less", "scss", "stylus"],
                            "default": "css"
                        }),
                        serde_json::json!({
                            "name": "cssTools",
                            "message": css_tools_message,
                            "type": "checkbox",
                            "choices": ["tailwindcss", "postcss", "autoprefixer"],
                            "default": []
                        }),
                        serde_json::json!({
                            "name": "lintTools",
                            "message": lint_tools_message,
                            "type": "checkbox",
                            "choices": ["eslint", "prettier", "stylelint", "commitlint", "editorconfig"],
                            "default": ["eslint", "prettier", "stylelint", "commitlint", "editorconfig"]
                        }),
                        serde_json::json!({
                            "name": "buildTool",
                            "message": build_tool_message,
                            "type": "list",
                            "choices": ["webpack", "vite"],
                            "default": "vite"
                        }),
                    ];
                    if let Some(question) = maybe_package_manager_question() {
                        questions.push(question);
                    }
                    if !skip_git {
                        questions.push(repository_question());
                    }
                    serde_json::Value::Array(questions)
                }
                "spa-react" => {
                    let mut questions = vec![
                        serde_json::json!({
                            "name": "cssProcessor",
                            "message": css_processor_message,
                            "type": "list",
                            "choices": ["css", "less", "scss", "stylus"],
                            "default": "css"
                        }),
                        serde_json::json!({
                            "name": "cssTools",
                            "message": css_tools_message,
                            "type": "checkbox",
                            "choices": ["tailwindcss", "postcss", "autoprefixer"],
                            "default": []
                        }),
                        serde_json::json!({
                            "name": "lintTools",
                            "message": lint_tools_message,
                            "type": "checkbox",
                            "choices": ["eslint", "prettier", "stylelint", "commitlint", "editorconfig"],
                            "default": ["eslint", "prettier", "stylelint", "commitlint", "editorconfig"]
                        }),
                        serde_json::json!({
                            "name": "buildTool",
                            "message": build_tool_message,
                            "type": "list",
                            "choices": ["webpack", "vite"],
                            "default": "vite"
                        }),
                    ];
                    if let Some(question) = maybe_package_manager_question() {
                        questions.push(question);
                    }
                    if !skip_git {
                        questions.push(repository_question());
                    }
                    serde_json::Value::Array(questions)
                }
                _ => {
                    let mut questions = Vec::new();
                    if let Some(question) = maybe_package_manager_question() {
                        questions.push(question);
                    }
                    serde_json::Value::Array(questions)
                }
            };
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "template": template,
                        "questions": questions,
                    })),
                    error: None,
                },
                events: vec![],
            }
        }
        "template.getDependencies" => {
            let template = request.params["template"].as_str().unwrap_or("spa-react");
            let options = request.params["options"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            let css_processor = options
                .get("cssProcessor")
                .and_then(|value| value.as_str())
                .unwrap_or("css");
            let lint_tools = options
                .get("lintTools")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            let (dependencies, mut dev_dependencies): (Vec<String>, Vec<String>) = match template {
                "spa-vue" => (vec!["vue".into(), "vite".into()], vec!["typescript".into()]),
                "toolkit" => (
                    vec!["tslib".into()],
                    vec![
                        "typescript".into(),
                        "eslint".into(),
                        "vite".into(),
                        "conventional-changelog-cli".into(),
                    ],
                ),
                "toolkit-monorepo" => (
                    vec!["tslib".into()],
                    vec![
                        "typescript".into(),
                        "eslint".into(),
                        "vite".into(),
                        "@changesets/cli".into(),
                    ],
                ),
                _ => (
                    vec!["react".into(), "vite".into()],
                    vec!["typescript".into()],
                ),
            };
            if css_processor == "less" {
                dev_dependencies.push("less".into());
            }
            if css_processor == "sass" {
                dev_dependencies.push("sass".into());
            }
            if css_processor == "stylus" {
                dev_dependencies.push("stylus".into());
            }
            for tool in lint_tools {
                if !dev_dependencies.iter().any(|item| item == &tool) {
                    dev_dependencies.push(tool);
                }
            }
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "template": template,
                        "dependencies": dependencies,
                        "devDependencies": dev_dependencies,
                    })),
                    error: None,
                },
                events: vec![],
            }
        }
        "template.getOutputTasks" => {
            let template = request.params["template"].as_str().unwrap_or("spa-react");
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "template": template,
                        "tasks": ["write-files", "install-deps"],
                    })),
                    error: None,
                },
                events: vec![],
            }
        }
        "template.render" => {
            let template = request.params["template"].as_str().unwrap_or("spa-react");
            let context = &request.params["context"];
            let options = request.params["options"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            let project_name = context["projectName"].as_str().unwrap_or("lania-app");
            let css_processor = options
                .get("cssProcessor")
                .and_then(|value| value.as_str())
                .unwrap_or("css");
            let lint_tools = options
                .get("lintTools")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            let files = match template {
                "toolkit" => vec![
                    serde_json::json!({"path": "src/index.ts", "content": format!("export function getToolkitInfo() {{\n  return {{ name: \"{project_name}\", version: \"0.0.1\" }};\n}}\n")}),
                    serde_json::json!({"path": "package.json", "content": format!("{{\n  \"name\": \"{project_name}\",\n  \"version\": \"0.1.0\",\n  \"scripts\": {{\n    \"release\": \"lan release run --profile package --apply --yes --publish\"\n  }}\n}}\n")}),
                ],
                "toolkit-monorepo" => vec![
                    serde_json::json!({"path": "package.json", "content": format!("{{\n  \"name\": \"{project_name}\",\n  \"private\": true,\n  \"scripts\": {{\n    \"version-packages\": \"changeset version\"\n  }}\n}}\n")}),
                    serde_json::json!({"path": "packages/core/src/index.ts", "content": format!("export const packageName = \"{project_name}-core\";\n")}),
                ],
                "spa-vue" => vec![
                    serde_json::json!({"path": "src/main.ts", "content": format!("console.log(\"{project_name} vue app\")\n")}),
                    serde_json::json!({"path": "package.json", "content": format!("{{\n  \"name\": \"{project_name}\",\n  \"version\": \"0.1.0\"\n}}\n")}),
                ],
                _ => {
                    let mut files = vec![
                        serde_json::json!({"path": "src/main.tsx", "content": format!("console.log(\"{project_name} react app\")\n")}),
                        serde_json::json!({"path": "package.json", "content": format!("{{\n  \"name\": \"{project_name}\",\n  \"version\": \"0.1.0\"\n}}\n")}),
                    ];
                    files.push(match css_processor {
                            "less" => serde_json::json!({"path": "src/App.less", "content": format!(".app {{\n  content: \"{project_name}\";\n}}\n")}),
                            "sass" => serde_json::json!({"path": "src/App.scss", "content": format!(".app {{\n  content: \"{project_name}\";\n}}\n")}),
                            "stylus" => serde_json::json!({"path": "src/App.styl", "content": format!(".app\n  content \"{project_name}\"\n")}),
                            _ => serde_json::json!({"path": "src/App.css", "content": format!(".app {{\n  content: \"{project_name}\";\n}}\n")}),
                        });
                    if lint_tools.iter().any(|tool| tool == "eslint") {
                        files.push(serde_json::json!({"path": "eslint.config.js", "content": "export default [];\n"}));
                    }
                    files
                }
            };
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "template": template,
                        "files": files,
                    })),
                    error: None,
                },
                events: vec![BridgeEvent {
                    method: BridgeEventMethod::Log,
                    params: serde_json::json!({
                        "level": "info",
                        "message": "Rendered template output",
                    }),
                }],
            }
        }
        "addTemplate.render" => {
            let template = request.params["template"].as_str().unwrap_or("rfc");
            let context = &request.params["context"];
            let language = context["language"].as_str().unwrap_or("ts");
            let content = match template {
                    "rcc" => format!(
                        "import React, {{ Component }} from 'react';\n\nclass MyComponent extends Component{} {{\n    render() {{\n        return (\n            <div></div>\n        );\n    }}\n}}\n\nexport default MyComponent;\n",
                        if language == "ts" { "<{}, {}>" } else { "" }
                    ),
                    "rfc" => "import React from 'react';\n\nconst MyComponent = () => {\n    return (\n        <div></div>\n    );\n};\n\nexport default MyComponent;\n".into(),
                    "v2" | "v3" => "<template>\n    <div></div>\n</template>\n".into(),
                    _ => "export {};\n".into(),
                };
            let extname = match template {
                "rcc" | "rfc" => {
                    if language == "js" {
                        "jsx"
                    } else {
                        "tsx"
                    }
                }
                "v2" | "v3" => "vue",
                "svelte" => "svelte",
                "astro" => "astro",
                "prettier" => "js",
                "eslint" => "js",
                "stylelint" => "js",
                "tsconfig" => "json",
                "commitizen" => "js",
                _ => "",
            };
            let filename = match template {
                "prettier" => Some("prettier.config.js"),
                "eslint" => Some("eslint.config.js"),
                "stylelint" => Some("stylelint.config.js"),
                "editorconfig" => Some(".editorconfig"),
                "gitignore" => Some(".gitignore"),
                "tsconfig" => Some("tsconfig.json"),
                "commitizen" => Some("cz.config.js"),
                _ => None,
            };
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "template": template,
                        "schemaVersion": 1,
                        "filename": filename,
                        "extname": if extname.is_empty() { serde_json::Value::Null } else { serde_json::json!(extname) },
                        "content": content,
                    })),
                    error: None,
                },
                events: vec![],
            }
        }
        "commitizen.run" => {
            let kind = request.params["kind"].as_str().unwrap_or("chore");
            let scope = request.params["scope"].as_str();
            let subject = request.params["subject"].as_str().unwrap_or("sync changes");
            let message = if let Some(scope) = scope {
                format!("{kind}({scope}): {subject}")
            } else {
                format!("{kind}: {subject}")
            };
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "accepted": true,
                        "tool": "commitizen",
                        "message": message,
                    })),
                    error: None,
                },
                events: vec![BridgeEvent {
                    method: BridgeEventMethod::Log,
                    params: serde_json::json!({
                        "level": "info",
                        "message": "Commitizen workflow prepared",
                    }),
                }],
            }
        }
        "commitlint.run" => {
            let message = request.params["message"].as_str().unwrap_or_default();
            let valid = message.contains(':');
            BridgeExchange {
                response: BridgeResponse {
                    id: request.id,
                    result: Some(serde_json::json!({
                        "accepted": true,
                        "tool": "commitlint",
                        "valid": valid,
                        "message": message,
                    })),
                    error: None,
                },
                events: vec![BridgeEvent {
                    method: BridgeEventMethod::Log,
                    params: serde_json::json!({
                        "level": "info",
                        "message": "Commitlint check completed",
                    }),
                }],
            }
        }
        _ => BridgeExchange {
            response: BridgeResponse {
                id: request.id,
                result: None,
                error: Some(BridgeError {
                    code: "E_METHOD_NOT_FOUND".into(),
                    message: format!("Unsupported method: {}", request.method),
                    data: None,
                }),
            },
            events: vec![],
        },
    }
}
