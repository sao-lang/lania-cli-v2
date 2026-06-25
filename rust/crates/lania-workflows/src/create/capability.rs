//! `create` 工作流的“模板能力”适配层：Rust 内建声明 + Node bridge 动态扩展。
//!
//! 为什么需要这一层：
//! - Node bridge 能提供更丰富的模板生态（动态模板、模板问题、渲染输出等）
//! - 但 CLI 不能硬依赖 bridge，否则 bridge 不可用时 create/add 就完全不能用
//! - 所以这里采用 fallback 策略：优先 bridge，失败时退回 Rust 内建的 `declared_template*`
//!
//! 你可以把这里理解成“能力网关”：
//! - `list/questions/dependencies/output_tasks/render` 都返回 (结果, 可选的 bridge 记录 step)
//! - 上层 workflow 收集这些 step，用于在输出里展示“我跟 bridge 交互了什么”

use std::path::PathBuf;
use std::path::Path;

use anyhow::{anyhow, Result};
use lania_fs::PlannedFile;
use lania_node_bridge::{AddTemplateBridgeCapability, NodeBridgeClient, TemplateBridgeCapability};
use serde_json::{json, Value};

use crate::models::{step, TemplateCapability, WorkflowBridgeStep};

use super::templates::{declared_template, declared_template_names, render_declared_template};

#[derive(Debug, Clone)]
pub(super) struct AddTemplateCapability<'a> {
    bridge: &'a NodeBridgeClient,
}

#[derive(Debug, Clone)]
pub(super) struct RenderedAddTemplate {
    pub(super) template: String,
    pub(super) filename: Option<String>,
    pub(super) extname: Option<String>,
    pub(super) content: String,
}

impl<'a> TemplateCapability<'a> {
    pub fn new(bridge: &'a NodeBridgeClient) -> Self {
        Self { bridge }
    }

    pub async fn list(&self, cwd: &Path) -> Result<(Vec<String>, Option<WorkflowBridgeStep>)> {
        // 这里先拿 Rust 内建模板，再尝试向 Node bridge 查询外部模板。
        // 这种“本地声明 + bridge 扩展”的组合能保证：
        // - 没有 bridge 时，CLI 仍有最小可用能力
        // - 有 bridge 时，又能无缝扩展到更完整的模板生态
        let mut templates = declared_template_names();
        let request = self
            .bridge
            .template_list_request(cwd.display().to_string());
        match self.bridge.list_templates(cwd.display().to_string()).await {
            Ok(exchange) => {
                let bridge_templates = exchange
                    .response
                    .result
                    .as_ref()
                    .and_then(|result| result["templates"].as_array())
                    .ok_or_else(|| anyhow!("template.list returned no templates"))?
                    .iter()
                    .filter_map(|value: &Value| value.as_str().map(|text| text.to_string()));
                templates.extend(bridge_templates);
                templates.sort();
                templates.dedup();
                Ok((templates, Some(step(request, exchange))))
            }
            Err(_error) if !templates.is_empty() => Ok((templates, None)),
            Err(error) => Err(error),
        }
    }

    pub async fn questions(
        &self,
        template: &str,
        options: serde_json::Value,
    ) -> Result<(Vec<Value>, Option<WorkflowBridgeStep>)> {
        let request = self
            .bridge
            .template_questions_request(template, options.clone());
        match self
            .bridge
            .get_template_questions(template.to_string(), options)
            .await
        {
            Ok(exchange) => {
                let questions = exchange
                    .response
                    .result
                    .as_ref()
                    .and_then(|result| result["questions"].as_array())
                    .ok_or_else(|| anyhow!("template.getQuestions returned no questions"))?
                    .clone();
                Ok((questions, Some(step(request, exchange))))
            }
            Err(error) => {
                // fallback 的思路与 list 一样：
                // 优先用 Node 返回的动态能力；失败时退回 Rust 内建声明模板。
                if let Some(template) = declared_template(template) {
                    let questions = template
                        .questions
                        .iter()
                        .map(|question| {
                            json!({
                                "name": question.name,
                                "type": question.question_type,
                            })
                        })
                        .collect();
                    Ok((questions, None))
                } else {
                    Err(error)
                }
            }
        }
    }

    pub async fn dependencies(
        &self,
        template: &str,
        options: serde_json::Value,
    ) -> Result<(Vec<String>, Vec<String>, Option<WorkflowBridgeStep>)> {
        let request = self
            .bridge
            .template_dependencies_request(template, options.clone());
        match self
            .bridge
            .get_template_dependencies(template.to_string(), options)
            .await
        {
            Ok(exchange) => {
                let result = exchange
                    .response
                    .result
                    .as_ref()
                    .ok_or_else(|| anyhow!("template.getDependencies returned no payload"))?;
                let dependencies = result["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str().map(|text| text.to_string()))
                    .collect();
                let dev_dependencies = result["devDependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str().map(|text| text.to_string()))
                    .collect();
                Ok((
                    dependencies,
                    dev_dependencies,
                    Some(step(request, exchange)),
                ))
            }
            Err(error) => {
                if let Some(template) = declared_template(template) {
                    Ok((
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
                        None,
                    ))
                } else {
                    Err(error)
                }
            }
        }
    }

    pub async fn output_tasks(
        &self,
        template: &str,
        options: serde_json::Value,
    ) -> Result<(Vec<String>, Option<WorkflowBridgeStep>)> {
        let request = self
            .bridge
            .template_output_tasks_request(template, options.clone());
        match self
            .bridge
            .get_template_output_tasks(template.to_string(), options)
            .await
        {
            Ok(exchange) => {
                let tasks = exchange
                    .response
                    .result
                    .as_ref()
                    .and_then(|result| result["tasks"].as_array())
                    .ok_or_else(|| anyhow!("template.getOutputTasks returned no tasks"))?
                    .iter()
                    .filter_map(|value| value.as_str().map(|text| text.to_string()))
                    .collect();
                Ok((tasks, Some(step(request, exchange))))
            }
            Err(error) => {
                if let Some(template) = declared_template(template) {
                    Ok((
                        template
                            .output_tasks
                            .iter()
                            .map(|value| (*value).to_string())
                            .collect(),
                        None,
                    ))
                } else {
                    Err(error)
                }
            }
        }
    }

    pub async fn render(
        &self,
        template: &str,
        context: Value,
        options: serde_json::Value,
    ) -> Result<(Vec<PlannedFile>, Option<WorkflowBridgeStep>)> {
        let request =
            self.bridge
                .template_render_request(template, context.clone(), options.clone());
        match self
            .bridge
            .render_template(template.to_string(), context.clone(), options)
            .await
        {
            Ok(exchange) => {
                let files = exchange
                    .response
                    .result
                    .as_ref()
                    .and_then(|result| result["files"].as_array())
                    .ok_or_else(|| anyhow!("template.render returned no files"))?
                    .iter()
                    .map(|file| -> Result<PlannedFile> {
                        Ok(PlannedFile {
                            path: PathBuf::from(
                                file["path"]
                                    .as_str()
                                    .ok_or_else(|| anyhow!("template file path missing"))?,
                            ),
                            content: file["content"]
                                .as_str()
                                .ok_or_else(|| anyhow!("template file content missing"))?
                                .to_string(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok((files, Some(step(request, exchange))))
            }
            Err(error) => {
                // 这里的 Rust fallback 不是“完整替代 Node 模板运行时”，
                // 而是让部分声明式模板在 bridge 不可用时仍可工作。
                if let Some(template) = declared_template(template) {
                    Ok((render_declared_template(template, &context)?, None))
                } else {
                    Err(error)
                }
            }
        }
    }
}

impl<'a> AddTemplateCapability<'a> {
    pub(super) fn new(bridge: &'a NodeBridgeClient) -> Self {
        Self { bridge }
    }

    pub(super) async fn render(
        &self,
        template: &str,
        context: Value,
    ) -> Result<(RenderedAddTemplate, WorkflowBridgeStep)> {
        let request = self
            .bridge
            .add_template_render_request(template, context.clone());
        let exchange = self
            .bridge
            .render_add_template(template.to_string(), context)
            .await?;
        let result = exchange
            .response
            .result
            .as_ref()
            .ok_or_else(|| anyhow!("addTemplate.render returned no payload"))?;
        Ok((
            RenderedAddTemplate {
                template: result["template"].as_str().unwrap_or(template).to_string(),
                filename: result["filename"].as_str().map(ToOwned::to_owned),
                extname: result["extname"].as_str().map(ToOwned::to_owned),
                content: result["content"]
                    .as_str()
                    .ok_or_else(|| anyhow!("addTemplate.render returned no content"))?
                    .to_string(),
            },
            step(request, exchange),
        ))
    }
}
