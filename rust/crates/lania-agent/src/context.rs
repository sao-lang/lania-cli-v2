//! 项目上下文采集（唯一允许读盘的地方之一）。
//!
//! core 只负责**排版**，这里负责**取材**：skills 索引、memory 摘要、构建工具。
//! 采集是 best-effort：任何一项读不到都静默跳过，绝不因为缺目录让 `lan pi` 起不来。

use std::path::{Path, PathBuf};

use lania_agent_core::{ApprovalMode, ProjectContext, SkillRef};

use crate::options::AgentOptions;

/// skill 索引里单条描述的长度上限（防止某个 SKILL.md 写了一屏）。
const MAX_DESCRIPTION_CHARS: usize = 160;
/// memory 注入上限（字符），约等于 1200 token。
const MAX_MEMORY_CHARS: usize = 4_800;

/// 组装这次运行能看到的一切上下文。
pub fn collect(options: &AgentOptions) -> ProjectContext {
    let cwd = options.cwd.as_str();
    ProjectContext {
        cwd: cwd.to_string(),
        build_tool: options.build_tool.clone(),
        commands: Vec::new(),
        skills: collect_skills(cwd, &options.skill_dirs),
        memory: if options.memory_enabled {
            read_memory(cwd)
        } else {
            None
        },
        extra_instructions: options.system_prompt.clone(),
        approval_hint: Some(describe_approval(options.approval_mode)),
    }
}

fn describe_approval(mode: ApprovalMode) -> String {
    match mode {
        ApprovalMode::Yolo => {
            "yolo — tools run without asking; be extra careful with writes and commands".to_string()
        }
        ApprovalMode::Write => {
            "write — file edits are auto-approved, shell commands still ask".to_string()
        }
        ApprovalMode::AlwaysAsk => {
            "always-ask — file edits and shell commands both wait for the user".to_string()
        }
    }
}

/// 扫描 `<dir>/<name>/SKILL.md`，只取 name + description。
///
/// 扫描顺序：用户级 `~/.lan/skills` → 项目配置的目录 → 项目默认 `.lan/skills`。
/// 同名以先出现的为准（项目覆盖用户）。
pub fn collect_skills(cwd: &str, configured_dirs: &[String]) -> Vec<SkillRef> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = home_dir() {
        dirs.push(home.join(".lan").join("skills"));
    }
    for dir in configured_dirs {
        dirs.push(resolve(cwd, dir));
    }
    dirs.push(Path::new(cwd).join(".lan").join("skills"));

    let mut skills: Vec<SkillRef> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path().join("SKILL.md");
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if skills.iter().any(|skill| skill.name == name) {
                continue;
            }
            let description = std::fs::read_to_string(&path)
                .ok()
                .map(|text| extract_description(&text))
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "(no description)".to_string());
            skills.push(SkillRef {
                name,
                description,
                path: relative_label(cwd, &path),
            });
        }
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    skills
}

fn resolve(cwd: &str, target: &str) -> PathBuf {
    let path = Path::new(target);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    }
}

fn relative_label(cwd: &str, path: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// 从 SKILL.md 里抽描述：优先 YAML front-matter 的 `description:`，否则第一段正文。
pub fn extract_description(text: &str) -> String {
    let mut lines = text.lines();
    let mut in_front_matter = false;
    let mut description = None;

    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed == "---" {
            in_front_matter = !in_front_matter;
            if !in_front_matter {
                break;
            }
            continue;
        }
        if in_front_matter {
            if let Some(rest) = trimmed.strip_prefix("description:") {
                description = Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
            }
            continue;
        }
        break;
    }

    let description = description.unwrap_or_else(|| first_paragraph(text));

    let collapsed = description.split_whitespace().collect::<Vec<_>>().join(" ");
    lania_agent_core::tokens::truncate_chars(&collapsed, MAX_DESCRIPTION_CHARS)
}

/// 没有 front-matter 时取「第一个正文段落」（连续非空行，跳过标题与分隔线）。
fn first_paragraph(text: &str) -> String {
    let mut lines = text
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty() || line.starts_with('#') || *line == "---");

    let mut paragraph: Vec<&str> = Vec::new();
    for line in lines.by_ref() {
        if line.is_empty() {
            break;
        }
        paragraph.push(line);
    }
    paragraph.join(" ")
}

/// 读 `.lan/agent/memory/MEMORY.md`（+ `learned.md` 最近若干行）。
pub fn read_memory(cwd: &str) -> Option<String> {
    let dir = Path::new(cwd).join(".lan").join("agent").join("memory");
    let mut parts: Vec<String> = Vec::new();
    for name in ["MEMORY.md", "learned.md"] {
        let path = dir.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        parts.push(format!("### {name}\n{trimmed}"));
    }
    if parts.is_empty() {
        return None;
    }
    let combined = parts.join("\n\n");
    Some(lania_agent_core::tokens::truncate_chars(
        &combined,
        MAX_MEMORY_CHARS,
    ))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_description_wins() {
        let text = "---\nname: pdf\ndescription: Read PDFs safely\n---\n\n# PDF\nbody";
        assert_eq!(extract_description(text), "Read PDFs safely");
    }

    #[test]
    fn falls_back_to_first_body_line() {
        let text = "# Title\n\nThis skill   does\nthings.";
        assert_eq!(extract_description(text), "This skill does things.");
    }

    #[test]
    fn skills_are_discovered_and_deduped() {
        let root = std::env::temp_dir().join(format!("lania-agent-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let skill_dir = root.join(".lan").join("skills").join("demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: demo skill\n---\n",
        )
        .unwrap();

        let cwd = root.display().to_string();
        // 用户目录与项目目录里都有 demo 时只保留一个。
        let skills = collect_skills(&cwd, &[".lan/skills".to_string()]);
        let demo: Vec<_> = skills.iter().filter(|skill| skill.name == "demo").collect();
        assert_eq!(demo.len(), 1, "demo skill must be deduped");
        assert_eq!(demo[0].description, "demo skill");
        assert_eq!(demo[0].path, ".lan/skills/demo/SKILL.md");
    }
}
