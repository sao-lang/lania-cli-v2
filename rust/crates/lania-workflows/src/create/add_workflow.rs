//! `add` 工作流：在现有项目里生成框架感知的多文件产物（§4.5）。
//!
//! 与旧实现的差异：
//! - **不再**是「选一个平铺模板 → 渲染一个文件」，而是「框架探测 → 选生成器 → N 个文件」。
//! - 生成器表只存在于 Rust 侧（`create::add::generators`），TS 通过 bridge 读取。
//! - 支持在已有文件里做**锚点插入**注册（`// <lania:imports>`），不做 AST 改写。
//!
//! 兼容：`--template rfc` 这类旧的平铺模板名仍然可用，映射到 `component` 生成器的 `variant`。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use lania_format::{FormatMode, FormatOptions, FormatService};
use lania_fs::PlannedFile;
use serde_json::{json, Value};

use crate::models::{
    redact_prompt_answers, AddWorkflow, AddWorkflowInput, WorkflowExecution, WorkflowServices,
    WorkflowState,
};
use crate::workflow_hooks::{call_files_prepare, write_files_with_hooks};

use super::add::{
    all_generators, detect_framework, find_generator, insert_after_marker, name_variants,
    render_generator, render_registrations, FrameworkDetection, Generator, NameVariants,
    ProjectRenderContext, RenderContext, LANIA_IMPORTS_MARKER,
};
use super::helpers::*;
use super::prompts::run_add_prompt;

/// 组件变体（旧的一级模板名，§4.5.6）。
const COMPONENT_VARIANTS: &[&str] = &["rfc", "rcc", "v2", "v3", "svelte", "astro"];
/// 配置变体（旧的一级模板名）。
const CONFIG_VARIANTS: &[&str] = &[
    "eslint",
    "prettier",
    "stylelint",
    "tsconfig",
    "editorconfig",
    "gitignore",
    "commitizen",
];

impl AddWorkflow {
    pub async fn run(
        &self,
        services: &WorkflowServices,
        input: AddWorkflowInput,
    ) -> Result<WorkflowExecution> {
        services.progress.begin("add", Some(5));
        let config = load_add_template_context(services, &input.cwd).await?;
        let detection = detect_framework(&input.cwd, Some(&config.raw));
        let generator = resolve_generator(&detection, &input)?;
        services.progress.advance("add", 1);

        let prompt_state = {
            let _progress_guard = services.progress.suspend_terminal_guard();
            run_add_prompt(
                &services.prompt,
                services.locale.as_str(),
                &input,
                generator,
                &detection,
            )?
        };
        let name = resolve_name(&prompt_state, &input, generator)?;
        let names = name_variants(&name);
        let target_dir = resolve_target_dir(&input, &prompt_state, generator, &names);
        validate_relative_target(
            target_dir
                .strip_prefix(&input.cwd)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default()
                .as_str(),
        )?;
        services.progress.advance("add", 2);

        let answers = resolved_answers(generator, &prompt_state, &input);
        let context = RenderContext {
            names: names.clone(),
            answers,
            project: ProjectRenderContext {
                language: config.language.clone(),
                css_processor: config.css_processor.clone(),
                project_name: config.project_name.clone(),
            },
        };
        let rendered = render_generator(generator, &context)?;
        let artifact_count = rendered.files.len();
        // 生成器产物路径相对 cwd；这里统一挂到 cwd 下拉平成绝对路径。
        let mut files = rendered
            .files
            .into_iter()
            .map(|file| PlannedFile {
                path: input.cwd.join(&file.path),
                content: file.content,
            })
            .collect::<Vec<_>>();
        let mut notes = Vec::new();
        services.progress.advance("add", 3);

        let formatter = FormatService;
        let format_report = formatter.format_planned_files(
            &services.exec,
            &mut files,
            &FormatOptions {
                enabled: true,
                mode: FormatMode::BestEffort,
                // 格式化根目录必须是 cwd：产物路径是 `src/...`，锚点注册还会改到
                // 已存在的 `src/app.module.ts`。用 `src/users/...` 做根会把相对路径算错。
                root_dir: Some(input.cwd.clone()),
            },
        )?;
        let conflicts = services.fs.find_conflicts(&files);
        call_files_prepare(services, "add", &target_dir, &mut files).await?;
        let report =
            write_files_with_hooks(services, "add", &target_dir, &files, input.force).await?;
        // 注册在写盘之后：它改的是用户已有文件，与本次生成的冲突检查是两件事。
        apply_registrations(generator, &context, &input.cwd, &mut notes);
        services.progress.advance("add", 4);

        let mut workflow_notes = vec![
            format!(
                "framework: {} ({})",
                framework_label(&detection),
                detection.reason()
            ),
            format!(
                "generator: {} ({} artifacts)",
                generator.name, artifact_count
            ),
            format!(
                "normalized target: {}",
                target_dir_relative(&target_dir, &input.cwd)
            ),
            format!("language: {}", config.language),
            format!("css processor: {}", config.css_processor),
        ];
        workflow_notes.extend(notes);
        if format_report.formatted_count() > 0 {
            workflow_notes.push(format!(
                "formatted {} generated files",
                format_report.formatted_count()
            ));
        }
        if format_report.failed_count() > 0 {
            workflow_notes.push(format!(
                "formatters failed for {} files (best-effort: kept original content)",
                format_report.failed_count()
            ));
        }
        if !conflicts.is_empty() && !input.force {
            workflow_notes.push(
                "conflicting files were skipped; rerun with --force to overwrite".to_string(),
            );
        }
        services.progress.finish("add");

        Ok(WorkflowExecution {
            workflow: "add".into(),
            state: WorkflowState::Completed,
            target_dir: target_dir.display().to_string(),
            prompts: redact_prompt_answers(&prompt_state, &services.prompt.secret_fields()),
            bridge_steps: Vec::new(),
            written_files: report
                .written
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
            conflicts: conflicts
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
            command_plans: Vec::new(),
            git_status: None,
            notes: workflow_notes,
            interactive_rendered: false,
        })
    }
}

fn framework_label(detection: &FrameworkDetection) -> String {
    match detection {
        FrameworkDetection::Detected { framework, .. } => framework.clone(),
        FrameworkDetection::Generic => "generic".into(),
    }
}

/// 解析生成器：
/// 1. `--template <generator>`（新写法）
/// 2. `--template <legacy archetype>`（`rfc` → `component` + `variant=rfc`）
/// 3. 探测到框架时默认 `resource`，否则 `component`
fn resolve_generator(
    detection: &FrameworkDetection,
    input: &AddWorkflowInput,
) -> Result<&'static Generator> {
    if let Some(requested) = input.template.as_deref().map(str::trim) {
        if !requested.is_empty() {
            if let Some(generator) = find_generator(detection.framework(), requested)
                .or_else(|| find_generator("", requested))
            {
                return Ok(generator);
            }
            if COMPONENT_VARIANTS.contains(&requested) || CONFIG_VARIANTS.contains(&requested) {
                let generator_name = legacy_generator_for_variant(requested);
                return find_generator("", generator_name)
                    .ok_or_else(|| anyhow!("generic generator {generator_name} missing"));
            }
            return Err(anyhow!(
                "unknown generator or template: {requested}. Try `lan add --list`."
            ));
        }
    }
    if !detection.is_generic() {
        if let Some(generator) = find_generator(detection.framework(), "resource") {
            return Ok(generator);
        }
    }
    find_generator("", "component").ok_or_else(|| anyhow!("generic component generator missing"))
}

fn resolve_name(
    prompt_state: &BTreeMap<String, Value>,
    input: &AddWorkflowInput,
    generator: &Generator,
) -> Result<String> {
    let from_prompt = prompt_state
        .get("name")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let resolved = input.name.clone().or(from_prompt).unwrap_or_default();
    if resolved.trim().is_empty() {
        if generator.wants_name {
            return Err(anyhow!(
                "missing name for generator {} (pass a positional name or --name)",
                generator.name
            ));
        }
        return Ok("config".into());
    }
    Ok(resolved)
}

/// 目标目录：`--target` 优先，其次 prompt，最后按框架约定推导。
fn resolve_target_dir(
    input: &AddWorkflowInput,
    prompt_state: &BTreeMap<String, Value>,
    generator: &Generator,
    names: &NameVariants,
) -> PathBuf {
    let raw = input
        .target
        .clone()
        .or_else(|| {
            prompt_state
                .get("target")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| default_target(generator, names));
    input.cwd.join(normalize_add_target(&raw))
}

fn default_target(generator: &Generator, names: &NameVariants) -> String {
    if generator.framework == "nestjs" {
        format!("src/{}", names.name_kebab)
    } else {
        "src".to_string()
    }
}

fn target_dir_relative(target: &Path, cwd: &Path) -> String {
    target
        .strip_prefix(cwd)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| target.display().to_string())
}

/// 把 prompt 答案 + CLI 选项合成生成器答案。
fn resolved_answers(
    generator: &Generator,
    prompt_state: &BTreeMap<String, Value>,
    input: &AddWorkflowInput,
) -> BTreeMap<String, Value> {
    let mut answers = BTreeMap::new();
    if let Some(template) = input.template.as_deref() {
        // `--template rfc` 同时表达「组件变体」。
        if COMPONENT_VARIANTS.contains(&template) || CONFIG_VARIANTS.contains(&template) {
            answers.insert("variant".to_string(), json!(template));
        }
    }
    for (key, value) in prompt_state {
        if key == "target" || key == "name" || key == "generator" {
            continue;
        }
        answers.insert(key.clone(), value.clone());
    }
    if !answers.contains_key("variant") {
        if let Some(question) = generator
            .questions
            .iter()
            .find(|question| question.field == "variant")
        {
            if let Some(first) = question.choices.first() {
                answers.insert("variant".to_string(), json!(first));
            }
        }
    }
    if generator
        .questions
        .iter()
        .any(|question| question.field == "crud")
        && !answers.contains_key("crud")
    {
        answers.insert("crud".to_string(), json!(true));
    }
    answers
}

/// 锚点注册：只对**已写盘存在**的文件做纯文本插入，且**不把目标文件加入计划列表**。
///
/// 后一点很关键：注册目标是用户已有的文件（`src/app.module.ts`），
/// 若把它塞进本次写盘清单，`--force=false` 时会被冲突检查判定为 conflict 而整体跳过，
/// 结果「注册提示说成功了，文件却没变」。
fn apply_registrations(
    generator: &Generator,
    context: &RenderContext,
    cwd: &Path,
    notes: &mut Vec<String>,
) {
    let needs_root_module = generator
        .registrations
        .iter()
        .any(|rule| rule.marker == LANIA_IMPORTS_MARKER && rule.target == "src/app.module.ts");
    let root_module_exists = cwd.join("src/app.module.ts").exists();
    for patch in render_registrations(generator, context) {
        let target_path = cwd.join(&patch.target);
        let Ok(current) = std::fs::read_to_string(&target_path) else {
            notes.push(format!(
                "please register manually in {}: {}",
                patch.target, patch.snippet
            ));
            continue;
        };
        match insert_after_marker(&current, &patch.marker, &patch.snippet) {
            Some(updated) => match std::fs::write(&target_path, updated) {
                Ok(()) => notes.push(format!(
                    "updated {} (registered {})",
                    patch.target, patch.snippet
                )),
                Err(error) => notes.push(format!(
                    "failed to update {}: {error}; please register manually: {}",
                    patch.target, patch.snippet
                )),
            },
            None => notes.push(format!(
                "please register manually in {} (anchor `{}` not found): {}",
                patch.target, patch.marker, patch.snippet
            )),
        }
    }
    if needs_root_module && !root_module_exists {
        notes.push(
            "src/app.module.ts not found: register the generated module in your root module"
                .to_string(),
        );
    }
}

/// `lan add --list` 与 bridge `addGenerator.list` 共用的清单视图。
pub fn generator_catalog(cwd: &Path, config_raw: Option<&Value>) -> Value {
    let detection = detect_framework(cwd, config_raw);
    let items = all_generators()
        .map(|generator| {
            json!({
                "name": generator.name,
                "framework": generator.framework,
                "label": generator.label_en,
                "labelZh": generator.label_zh,
                "wantsName": generator.wants_name,
                "artifactCount": generator.artifacts.len(),
                "questions": generator.questions.iter().map(|question| json!({
                    "field": question.field,
                    "label": question.label_en,
                    "labelZh": question.label_zh,
                    "kind": match question.kind {
                        super::add::generators::GeneratorQuestionKind::Text => "input",
                        super::add::generators::GeneratorQuestionKind::Select => "select",
                        super::add::generators::GeneratorQuestionKind::Confirm => "confirm",
                    },
                    "required": question.required,
                    "choices": question.choices,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "framework": detection.framework(),
        "frameworkReason": detection.reason(),
        "generic": detection.is_generic(),
        "generators": items,
    })
}

/// 供 prompt 层复用的「框架 → 可用生成器」列表。
pub(crate) fn available_generators(detection: &FrameworkDetection) -> Vec<&'static Generator> {
    all_generators()
        .filter(|generator| {
            generator.framework.is_empty() || generator.framework == detection.framework()
        })
        .collect()
}

/// 旧的平铺模板名是否是组件/配置变体（`rfc` / `eslint` …）。
pub(crate) fn legacy_variant(name: &str) -> Option<&'static str> {
    COMPONENT_VARIANTS
        .iter()
        .chain(CONFIG_VARIANTS.iter())
        .copied()
        .find(|variant| *variant == name)
}

/// 旧模板名所属的生成器名（`rfc` → `component`，`eslint` → `config`）。
pub(crate) fn legacy_generator_for_variant(name: &str) -> &'static str {
    if CONFIG_VARIANTS.contains(&name) {
        "config"
    } else {
        "component"
    }
}
