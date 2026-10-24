//! 运行时管理错误。
//!
//! 所有失败都显式分类，这样 CLI 能给出「换一个版本 / 换一个语言」这类可操作提示，
//! 而不是把上游的网络响应原样抛给用户。

use thiserror::Error;

use crate::Platform;

/// 运行时管理的顶层错误。
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Platform(#[from] PlatformError),

    #[error("`{language}` is not a self-managed runtime yet")]
    UnsupportedLanguage { language: String },

    #[error(transparent)]
    Network(#[from] NetworkError),

    #[error(transparent)]
    Verify(#[from] VerifyError),

    #[error(transparent)]
    Archive(#[from] ArchiveError),

    #[error("no release of {language} matches version `{version}`")]
    VersionNotFound { language: String, version: String },

    #[error("no artifact of {language} {version} for platform {platform}")]
    ArtifactNotFound {
        language: String,
        version: String,
        platform: Platform,
    },

    #[error("{language} {version} is not installed in {root}")]
    NotInstalled {
        language: String,
        version: String,
        root: String,
    },

    #[error("unexpected upstream response from {url}: {detail}")]
    Upstream { url: String, detail: String },

    #[error("the lifecycle lock for {language} is held by another lania process ({path})")]
    Locked { language: String, path: String },

    #[error("{message}")]
    Other { message: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("operating system `{0}` is not supported by the self-managed runtime manager")]
    UnsupportedOs(String),
    #[error("architecture `{0}` is not supported by the self-managed runtime manager")]
    UnsupportedArch(String),
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("request to {url} failed: {detail}")]
    Request { url: String, detail: String },
    #[error("request to {url} returned HTTP {status}")]
    Status { url: String, status: u16 },
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("checksum mismatch for {artifact}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        artifact: String,
        expected: String,
        actual: String,
    },
    #[error("no checksum is published for {artifact}")]
    MissingChecksum { artifact: String },
}

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("cannot open {path}: {detail}")]
    Unsupported { path: String, detail: String },
    #[error("cannot extract {path}: {detail}")]
    Extract { path: String, detail: String },
}

impl From<anyhow::Error> for RuntimeError {
    fn from(error: anyhow::Error) -> Self {
        Self::other(error.to_string())
    }
}

impl RuntimeError {
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            message: message.into(),
        }
    }
}
