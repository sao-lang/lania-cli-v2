//! system prompt 组装。
//!
//! 刻意是**纯函数**：所有上下文（可用命令、skills 索引、memory 摘要）由命令层采集后传进来。
//! 这样 prompt 的措辞可以单测，采集逻辑可以各宿主自行实现（GUI 宿主可能没有 lan 命令表）。

/// skills 索引项：只有 `name + description` 进 prompt，正文由模型按需 `read`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRef {
    pub name: String,
    pub description: String,
    pub path: String,
}

/// 项目上下文快照。
#[derive(Debug, Clone, Default)]
pub struct ProjectContext {
    pub cwd: String,
    /// 例如 `pnpm` / `cargo` / `go`。
    pub build_tool: Option<String>,
    /// 项目里可用的 `lan <command>`。
    pub commands: Vec<String>,
    pub skills: Vec<SkillRef>,
    /// `MEMORY.md` 摘要（已截断）。
    pub memory: Option<String>,
    /// 用户在 `custom.agent.systemPrompt` 里追加的指令。
    pub extra_instructions: Option<String>,
    /// 审批模式，用于向模型解释「哪些操作会打断用户」。
    pub approval_hint: Option<String>,
}

const BASE_PROMPT: &str = "\
You are `lan pi`, a coding agent running inside the lan CLI on a real workspace.

Operating rules:
- Inspect before you edit: read the relevant files before changing them.
- Keep changes minimal and match the surrounding code style.
- Every tool runs in the host process; write/exec tools may require the user's approval.
- If a tool call is denied, do not retry it blindly — explain what you need instead.
- Verify your work with the project's own commands before claiming success.
- Answer in the language the user writes in. Be concise; no filler.";

/// 组装完整的 system prompt。空上下文也返回可用的 base prompt。
pub fn build_system_prompt(context: &ProjectContext) -> String {
    let mut prompt = String::from(BASE_PROMPT);

    if !context.cwd.trim().is_empty() {
        prompt.push_str(&format!("\n\nWorkspace: `{}`", context.cwd.trim()));
    }
    if let Some(build_tool) = context
        .build_tool
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        prompt.push_str(&format!("\nBuild tool: {build_tool}"));
    }
    if let Some(hint) = context
        .approval_hint
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        prompt.push_str(&format!("\nApproval mode: {hint}"));
    }

    if !context.commands.is_empty() {
        prompt.push_str("\n\nProject commands (prefer these over raw shell):\n");
        for command in context.commands.iter().take(24) {
            prompt.push_str(&format!("- lan {command}\n"));
        }
    }

    if !context.skills.is_empty() {
        prompt.push_str(
            "\n\nSkills (read the SKILL.md at its path when the task matches; do not guess):\n",
        );
        for skill in context.skills.iter().take(40) {
            prompt.push_str(&format!(
                "- {} — {} ({})\n",
                skill.name,
                skill.description.trim(),
                skill.path
            ));
        }
    }

    if let Some(memory) = context.memory.as_deref().map(str::trim) {
        if !memory.is_empty() {
            prompt.push_str("\n\nProject memory:\n");
            prompt.push_str(memory);
            prompt.push('\n');
        }
    }

    if let Some(extra) = context
        .extra_instructions
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\n\nAdditional user instructions (highest priority):\n");
        prompt.push_str(extra);
        prompt.push('\n');
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_prompt_has_no_empty_sections() {
        let prompt = build_system_prompt(&ProjectContext::default());
        assert!(prompt.starts_with("You are `lan pi`"));
        assert!(!prompt.contains("Skills"));
        assert!(!prompt.contains("Project memory"));
    }

    #[test]
    fn sections_appear_only_when_present() {
        let context = ProjectContext {
            cwd: "/repo".into(),
            build_tool: Some("pnpm".into()),
            commands: vec!["build".into(), "test".into()],
            skills: vec![SkillRef {
                name: "pdf".into(),
                description: "read pdfs".into(),
                path: ".lan/skills/pdf/SKILL.md".into(),
            }],
            memory: Some("- prefers tabs".into()),
            extra_instructions: Some("always write tests".into()),
            approval_hint: Some("write tools ask first".into()),
        };
        let prompt = build_system_prompt(&context);
        assert!(prompt.contains("Workspace: `/repo`"));
        assert!(prompt.contains("- lan build"));
        assert!(prompt.contains("pdf — read pdfs"));
        assert!(prompt.contains("prefers tabs"));
        assert!(prompt.contains("always write tests"));
        // 用户指令必须在最后，权重最高。
        assert!(prompt.find("always write tests").unwrap() > prompt.find("prefers tabs").unwrap());
    }
}
