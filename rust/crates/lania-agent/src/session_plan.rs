//! 会话规划：决定「这次跑哪个会话、要不要恢复历史」。
//!
//! 是产品策略，不是 CLI 细节，所以留在产品层。

use anyhow::{anyhow, Result};
use serde_json::Value;

use lania_agent_core::{generate_session_id, SessionStore};

use crate::options::AgentOptions;

/// 规划结果。
#[derive(Debug, Clone)]
pub struct SessionPlan {
    pub session_id: String,
    pub is_new: bool,
    pub messages: Vec<Value>,
}

impl SessionPlan {
    pub fn resolve(options: &AgentOptions) -> Result<Self> {
        if options.no_session {
            return Ok(Self::fresh());
        }

        let store = SessionStore::new(&options.cwd);
        let explicit = if options.new_session {
            None
        } else if let Some(resume) = options.resume.as_ref() {
            Some(resume.clone())
        } else if options.continue_latest {
            store.latest()
        } else {
            None
        };

        match explicit {
            Some(session_id) => {
                // 文件不存在 = 会话不存在，`header()` 会把缺失文件报成 I/O 错误；
                // 这里先判存在性，避免把底层的 os error 直接暴露给用户。
                let header = if store.layout().session_file(&session_id).exists() {
                    store
                        .header(&session_id)
                        .map_err(|error| anyhow!("cannot resume session {session_id}: {error}"))?
                } else {
                    None
                };
                if header.is_none() {
                    return Err(anyhow!(
                        "unknown session `{session_id}`; run `lan pi --sessions` to list them"
                    ));
                }
                let messages = store.messages(&session_id).unwrap_or_default();
                Ok(Self {
                    session_id,
                    is_new: false,
                    messages,
                })
            }
            None => Ok(Self::fresh()),
        }
    }

    fn fresh() -> Self {
        Self {
            session_id: generate_session_id(),
            is_new: true,
            messages: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lania_agent_core::SessionHeader;

    fn workspace(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lania-agent-plan-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_session_is_always_fresh() {
        let dir = workspace("nosession");
        let options = AgentOptions {
            cwd: dir.display().to_string(),
            no_session: true,
            continue_latest: true,
            ..AgentOptions::default()
        };
        // `--no-session` 与 `--continue` 互斥由 validate 拦，这里模拟绕过的情况。
        let plan = SessionPlan::resolve(&AgentOptions {
            continue_latest: false,
            ..options
        })
        .unwrap();
        assert!(plan.is_new);
        assert!(plan.messages.is_empty());
    }

    #[test]
    fn continue_picks_latest_session() {
        let dir = workspace("continue");
        let store = SessionStore::new(&dir);
        store.create(&SessionHeader::new("s1", &dir.display().to_string())).unwrap();
        let options = AgentOptions {
            cwd: dir.display().to_string(),
            continue_latest: true,
            ..AgentOptions::default()
        };
        let plan = SessionPlan::resolve(&options).unwrap();
        assert_eq!(plan.session_id, "s1");
        assert!(!plan.is_new);
    }

    #[test]
    fn unknown_resume_is_an_error() {
        let dir = workspace("unknown");
        let options = AgentOptions {
            cwd: dir.display().to_string(),
            resume: Some("nope".into()),
            ..AgentOptions::default()
        };
        let error = SessionPlan::resolve(&options).unwrap_err();
        assert!(error.to_string().contains("unknown session"), "{error}");
    }

    #[test]
    fn new_overrides_continue() {
        let dir = workspace("new");
        let store = SessionStore::new(&dir);
        store.create(&SessionHeader::new("s1", &dir.display().to_string())).unwrap();
        let options = AgentOptions {
            cwd: dir.display().to_string(),
            continue_latest: true,
            new_session: true,
            ..AgentOptions::default()
        };
        let plan = SessionPlan::resolve(&options).unwrap();
        assert!(plan.is_new);
        assert_ne!(plan.session_id, "s1");
    }
}
