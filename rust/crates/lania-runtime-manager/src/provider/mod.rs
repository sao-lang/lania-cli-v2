//! Provider trait 与注册表。
//!
//! ```text
//! 下载什么  → list_releases + resolve_artifact
//! 怎么解压  → 由 archive 按后缀自动处理，Provider 不关心
//! 目录怎么摆 → install_layout
//! 需要什么  → Layout::{bin_dir, env, entrypoints}
//! ```

pub mod model;

pub mod bun;
pub mod deno;
pub mod go;
pub mod node;
pub mod python;
pub mod rust;
pub mod zig;

use std::path::Path;

use model::{ArtifactPlan, Layout, Release, ResolveContext};

use crate::{Platform, RuntimeError};

pub use model::ProviderInfo;

pub use model::{
    ArtifactPlan as ProviderArtifact, Layout as ProviderLayout, Release as ProviderRelease,
    ResolveContext as ProviderContext,
};

/// Provider 内部失败：统一收敛成 [`RuntimeError`] 之前，先保留上下文。
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("{0}")]
    Upstream(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("unexpected upstream payload: {detail}")]
    Unexpected { detail: String },
    #[error("cannot lay out the extracted payload: {detail}")]
    Layout { detail: String },
}

impl From<RuntimeError> for ProviderError {
    fn from(error: RuntimeError) -> Self {
        Self::Upstream(error.to_string())
    }
}

/// 一个语言的运行时提供者。
pub trait RuntimeProvider: Send + Sync {
    /// 语言 id，与 [`crate::Language::id`] 保持一致（如 `node`、`python`）。
    fn id(&self) -> &'static str;

    /// 上游分发渠道（展示用）。
    fn upstream(&self) -> &'static str;

    /// 需要暴露到 shim 的入口命令。`use` 时会重新生成这些 shim。
    fn entrypoints(&self) -> &'static [&'static str];

    /// 查询全部可用版本（新 → 旧）。
    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError>;

    /// 解析某个版本在本平台的下载产物（含官方校验和）。
    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError>;

    /// 把解压结果整理成统一布局。
    ///
    /// `extracted` 是临时解压目录，`target` 是最终版本目录（用于登记需要绝对路径的项）。
    fn install_layout(&self, extracted: &Path, target: &Path) -> Result<Layout, ProviderError>;

    /// 额外需要下载的产物（例如 Rust 的编译器与 cargo 是两个独立组件包）。
    fn extra_artifacts(
        &self,
        _release: &Release,
        _ctx: &ResolveContext<'_>,
    ) -> Result<Vec<ArtifactPlan>, ProviderError> {
        Ok(Vec::new())
    }
}

impl ProviderError {
    /// 把 Provider 内部错误收敛为带语言上下文的运行时错误。
    pub fn into_runtime(self, language: &str) -> RuntimeError {
        match self {
            Self::Upstream(detail) => RuntimeError::Upstream {
                url: language.to_string(),
                detail,
            },
            Self::Unsupported(detail) => RuntimeError::other(format!("{language}: {detail}")),
            Self::Unexpected { detail } => RuntimeError::Upstream {
                url: language.to_string(),
                detail,
            },
            Self::Layout { detail } => RuntimeError::other(format!("{language}: {detail}")),
        }
    }
}

/// 上游分发渠道（用于计划说明与 `lan env list` 展示）。
pub fn upstream_for(language_id: &str) -> &'static str {
    match language_id {
        node::ID => "nodejs.org/dist",
        python::ID => "python-build-standalone",
        go::ID => "go.dev/dl",
        rust::ID => "static.rust-lang.org",
        deno::ID => "github.com/denoland/deno/releases",
        bun::ID => "github.com/oven-sh/bun/releases",
        zig::ID => "ziglang.org/download",
        _ => "unknown upstream",
    }
}

/// 全部第一批语言的 Provider 工厂，按语言 id 索引。
pub fn provider_for(language_id: &str, platform: Platform) -> Option<Box<dyn RuntimeProvider>> {
    Some(match language_id {
        node::ID => Box::new(node::NodeProvider::new(platform)),
        python::ID => Box::new(python::PythonProvider::new(platform)),
        go::ID => Box::new(go::GoProvider::new(platform)),
        rust::ID => Box::new(rust::RustProvider::new(platform)),
        deno::ID => Box::new(deno::DenoProvider::new(platform)),
        bun::ID => Box::new(bun::BunProvider::new(platform)),
        zig::ID => Box::new(zig::ZigProvider::new(platform)),
        _ => return None,
    })
}

/// 第一批（自管下载）已实现的全部语言 id。
pub fn supported_language_ids() -> &'static [&'static str] {
    &[
        node::ID,
        python::ID,
        go::ID,
        rust::ID,
        deno::ID,
        bun::ID,
        zig::ID,
    ]
}

/// 描述某个语言在本平台的 Provider 能力（供 `lan env list` / TUI 展示）。
pub fn describe(language_id: &str, platform: Platform) -> Option<ProviderInfo> {
    let provider = provider_for(language_id, platform)?;
    let unsupported = platform_limitation(language_id, platform);
    Some(ProviderInfo {
        id: provider.id().to_string(),
        upstream: provider.upstream().to_string(),
        entrypoints: provider
            .entrypoints()
            .iter()
            .map(|item| item.to_string())
            .collect(),
        supported: unsupported.is_none(),
        unsupported_reason: unsupported,
    })
}

/// 该语言是否在计划矩阵里还没实现（第一批之外）。
pub fn is_planned(language_id: &str) -> bool {
    matches!(
        language_id,
        "java" | "kotlin" | "scala" | "dart" | "swift" | "d" | "v" | "julia" | "haskell"
    )
}

/// 各上游在特定平台上的已知缺口（返回原因文本）。
///
/// 只在「上游确实不发布该平台产物」时才返回 `Some`，避免出现过早拒绝。
pub fn platform_limitation(language_id: &str, platform: Platform) -> Option<String> {
    match language_id {
        // Zig 官方只发布 x64/arm64 的 linux / macOS / Windows 产物，这里全部覆盖。
        zig::ID if platform.os == crate::Os::Linux && platform.libc == crate::Libc::Musl => {
            Some("zig ships glibc-linked binaries only".to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arch, Libc, Os};

    fn platform(os: Os, arch: Arch) -> Platform {
        Platform {
            os,
            arch,
            libc: if os == Os::Linux {
                Libc::Gnu
            } else {
                Libc::None
            },
        }
    }

    #[test]
    fn every_batch_one_language_has_a_provider() {
        let platform = platform(Os::Windows, Arch::X64);
        for id in supported_language_ids() {
            assert!(
                provider_for(id, platform).is_some(),
                "missing provider for {id}"
            );
        }
        assert!(provider_for("java", platform).is_none());
    }

    #[test]
    fn unsupported_platforms_are_reported() {
        // Zig 没有 macOS x64 之外的问题，但 Deno 在 linux-arm64 上有产物；
        // 这里只验证描述结构完整。
        let info = describe("node", platform(Os::Linux, Arch::Arm64)).expect("node provider");
        assert_eq!(info.id, "node");
        assert!(info.supported);
        assert!(info.unsupported_reason.is_none());
    }
}
