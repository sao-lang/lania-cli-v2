//! 只负责编排 git 参数，不触发任何执行。
//!
//! 这些 helper 让上层可以先生成命令计划，再决定是否真正调用执行层。

use super::GitService;

impl GitService {
    pub fn plan_add_all(&self) -> Vec<String> {
        vec!["add".into(), "-A".into()]
    }

    pub fn plan_init(&self) -> Vec<String> {
        vec!["init".into()]
    }

    pub fn plan_commit_message(&self, message: &str) -> Vec<String> {
        vec!["commit".into(), "-m".into(), message.to_string()]
    }

    pub fn plan_commit_amend(&self, message: Option<&str>, no_edit: bool) -> Vec<String> {
        let mut args = vec!["commit".into(), "--amend".into()];
        if let Some(message) = message {
            args.push("-m".into());
            args.push(message.to_string());
        } else if no_edit {
            args.push("--no-edit".into());
        }
        args
    }

    pub fn plan_push(&self, remote: &str, branch: &str) -> Vec<String> {
        vec!["push".into(), remote.to_string(), branch.to_string()]
    }

    pub fn plan_push_tag(&self, remote: &str, tag: &str) -> Vec<String> {
        vec!["push".into(), remote.to_string(), tag.to_string()]
    }

    pub fn plan_tag_create_lightweight(&self, tag: &str) -> Vec<String> {
        vec!["tag".into(), tag.to_string()]
    }

    pub fn plan_tag_create_annotated(&self, tag: &str, message: &str) -> Vec<String> {
        vec![
            "tag".into(),
            tag.to_string(),
            "-m".into(),
            message.to_string(),
        ]
    }

    pub fn plan_tag_delete(&self, tag: &str) -> Vec<String> {
        vec!["tag".into(), "-d".into(), tag.to_string()]
    }
}
