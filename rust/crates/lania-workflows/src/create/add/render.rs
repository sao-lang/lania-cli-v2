//! 生成器渲染与副作用注册（§4.5.2 / §4.5.5）。
//!
//! 纯函数：输入 `Generator` + 命名变体 + 答案，输出 `Vec<PlannedFile>` 与提示。
//! 之所以不在这里写盘，是为了让「冲突检查 → 用户确认 → 写盘」仍由 workflow 统一负责。

use std::path::Path;

use anyhow::{anyhow, Result};
use lania_fs::PlannedFile;
use serde_json::Value;

use super::generators::{ArtifactRule, Generator, RegistrationRule};
use super::names::NameVariants;
use super::templates::find_template;

/// 渲染上下文：命名变体 + 生成器答案。
#[derive(Debug, Clone)]
pub struct RenderContext {
    pub names: NameVariants,
    /// 已解析的答案（`variant` / `crud` / 其他自定义问题）。
    pub answers: std::collections::BTreeMap<String, Value>,
    /// 项目级上下文（`language` / `cssProcessor` / `projectName`）。
    pub project: ProjectRenderContext,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectRenderContext {
    pub language: String,
    pub css_processor: String,
    pub project_name: String,
}

/// 渲染结果：文件 + 需要用户手动处理的事项。
#[derive(Debug, Clone, Default)]
pub struct RenderOutput {
    pub files: Vec<PlannedFile>,
}

/// 渲染一个生成器的全部产物。
pub fn render_generator(generator: &Generator, context: &RenderContext) -> Result<RenderOutput> {
    let mut output = RenderOutput::default();
    for artifact in generator.artifacts {
        if !artifact_matches(artifact, context) {
            continue;
        }
        let path = render_text(artifact.path, context);
        let template = find_template(artifact.template).ok_or_else(|| {
            anyhow!(
                "generator {} references unknown template {}",
                generator.name,
                artifact.template
            )
        })?;
        let content = render_text(template.content, context);
        output.files.push(PlannedFile {
            path: Path::new(&path).to_path_buf(),
            content,
        });
    }
    if output.files.is_empty() {
        return Err(anyhow!(
            "generator {} produced no files for the selected options",
            generator.name
        ));
    }
    Ok(output)
}

/// `when` 语法目前只支持 `option=value`；缺失 `when` 表示无条件生成。
fn artifact_matches(artifact: &ArtifactRule, context: &RenderContext) -> bool {
    let Some(condition) = artifact.when else {
        return true;
    };
    let Some((key, expected)) = condition.split_once('=') else {
        return true;
    };
    let actual = match context.answers.get(key) {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    actual == expected
}

/// 命名占位符 + 项目上下文占位符替换。
///
/// 只做单趟替换：模板里若出现 `{name}` 与 `{name_kebab}`，必须先替换长的键，
/// 否则 `{name}` 抢先匹配会把 `{name_kebab}` 破坏成 `<value>_kebab`。
pub fn render_text(template: &str, context: &RenderContext) -> String {
    let mut pairs: Vec<(String, String)> = context
        .names
        .placeholders()
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    pairs.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
    let mut rendered = template.to_string();
    for (key, value) in pairs {
        rendered = rendered.replace(&format!("{{{key}}}"), &value);
    }
    rendered = rendered.replace("{cssProcessor}", &context.project.css_processor);
    rendered = rendered.replace("{language}", &context.project.language);
    rendered = rendered.replace("{projectName}", &context.project.project_name);
    rendered
}

/// 注册副作用的渲染结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationPatch {
    /// 目标文件（相对 cwd）。
    pub target: String,
    /// 需要在锚点后插入的文本。
    pub snippet: String,
    /// 锚点标记。
    pub marker: String,
}

/// 构造注册补丁。
///
/// 是否真的存在目标文件由调用方在写盘后判断（存在才插入，否则提示手动注册）。
pub fn render_registrations(
    generator: &Generator,
    context: &RenderContext,
) -> Vec<RegistrationPatch> {
    generator
        .registrations
        .iter()
        .map(|rule| registration_patch(rule, context))
        .collect()
}

fn registration_patch(rule: &RegistrationRule, context: &RenderContext) -> RegistrationPatch {
    RegistrationPatch {
        target: render_text(rule.target, context),
        snippet: render_text(rule.snippet, context),
        marker: rule.marker.to_string(),
    }
}

/// 在 `content` 的锚点后插入 `snippet`。
///
/// 返回 `None` 表示锚点不存在 —— 此时**不做任何猜测性插入**（§4.5.5 禁止 AST 改写），
/// 由调用方提示用户手动注册。
pub fn insert_after_marker(content: &str, marker: &str, snippet: &str) -> Option<String> {
    let index = content.find(marker)?;
    let insert_at = index + marker.len();
    let mut out = String::with_capacity(content.len() + snippet.len() + 1);
    out.push_str(&content[..insert_at]);
    out.push('\n');
    out.push_str(snippet);
    out.push_str(&content[insert_at..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::add::generators::find_generator;

    fn context(name: &str, variant: &str) -> RenderContext {
        RenderContext {
            names: super::super::names::name_variants(name),
            answers: [("variant".to_string(), Value::String(variant.into()))]
                .into_iter()
                .collect(),
            project: ProjectRenderContext {
                language: "ts".into(),
                css_processor: "css".into(),
                project_name: "demo".into(),
            },
        }
    }

    #[test]
    fn renders_seven_files_for_nest_resource() {
        let generator = find_generator("nestjs", "resource").expect("resource");
        let output = render_generator(generator, &context("users", "rest")).expect("render");
        assert_eq!(output.files.len(), 7);
        let paths: Vec<String> = output
            .files
            .iter()
            .map(|file| file.path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.contains(&"src/users/users.module.ts".to_string()));
        assert!(paths.contains(&"src/users/users.controller.ts".to_string()));
        assert!(paths.contains(&"src/users/users.service.ts".to_string()));
        assert!(paths.contains(&"src/users/dto/create-user.dto.ts".to_string()));
        assert!(paths.contains(&"src/users/dto/update-user.dto.ts".to_string()));
        assert!(paths.contains(&"src/users/entities/user.entity.ts".to_string()));
        assert!(paths.contains(&"src/users/users.controller.spec.ts".to_string()));
    }

    #[test]
    fn graphql_variant_adds_resolver() {
        let generator = find_generator("nestjs", "resource").expect("resource");
        let output = render_generator(generator, &context("users", "graphql")).expect("render");
        assert_eq!(output.files.len(), 8);
        assert!(output
            .files
            .iter()
            .any(|file| file.path.to_string_lossy().ends_with("users.resolver.ts")));
    }

    #[test]
    fn rendered_content_uses_class_names_from_the_name() {
        let generator = find_generator("nestjs", "resource").expect("resource");
        let output =
            render_generator(generator, &context("user-profiles", "rest")).expect("render");
        let controller = output
            .files
            .iter()
            .find(|file| {
                file.path
                    .to_string_lossy()
                    .ends_with("user-profiles.controller.ts")
            })
            .expect("controller file");
        assert!(controller
            .content
            .contains("export class UserProfilesController"));
        assert!(controller.content.contains("'user-profiles'"));
    }

    #[test]
    fn registration_inserts_after_marker_only() {
        let content = "import { Module } from '@nestjs/common';\n// <lania:imports>\n\n@Module({})\nexport class AppModule {}\n";
        let updated = insert_after_marker(
            content,
            "// <lania:imports>",
            "import { UsersModule } from './users/users.module';",
        )
        .expect("marker present");
        assert!(updated
            .contains("// <lania:imports>\nimport { UsersModule } from './users/users.module';"));
        assert!(insert_after_marker(content, "// <lania:none>", "x").is_none());
    }

    #[test]
    fn component_generator_uses_variant_template() {
        let generator = find_generator("", "component").expect("component");
        let output = render_generator(generator, &context("button", "rfc")).expect("render");
        assert_eq!(output.files.len(), 1);
        assert!(output.files[0].content.contains("export function Button"));
    }
}
