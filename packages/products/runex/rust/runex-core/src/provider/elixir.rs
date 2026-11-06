//! Elixir Provider：官方预编译包（按 OTP 大版本分发）。
//!
//! - 版本清单：`api.github.com/repos/elixir-lang/elixir/releases`
//! - 产物：同一个 release 下有 `elixir-otp-<N>.zip`，取 N 最大的一个
//! - 校验：同名 `.sha256sum` sidecar
//! - 目录：zip 根下就是 `bin/` / `lib/`
//! - 依赖：运行需要 Erlang/OTP；当前 lania 尚未自管 Erlang，与 Kotlin 依赖 JDK 同理。

use super::github;
use super::model::{ArtifactPlan, Layout, Release, ResolveContext};
use super::{ProviderError, RuntimeProvider};

pub const ID: &str = "elixir";
const REPO: &str = "elixir-lang/elixir";

pub struct ElixirProvider;

impl ElixirProvider {
    pub fn new() -> Self {
        Self
    }

    /// 从资产列表里选最高 OTP 版本的 zip（`elixir-otp-27.zip` → 27）。
    fn pick_asset(assets: &[github::Asset]) -> Option<github::Asset> {
        assets
            .iter()
            .filter_map(|asset| {
                let major = asset
                    .name
                    .strip_prefix("elixir-otp-")?
                    .strip_suffix(".zip")?
                    .parse::<u32>()
                    .ok()?;
                Some((major, asset.clone()))
            })
            .max_by_key(|(major, _)| *major)
            .map(|(_, asset)| asset)
    }
}

impl RuntimeProvider for ElixirProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn upstream(&self) -> &'static str {
        "github.com/elixir-lang/elixir/releases"
    }

    fn entrypoints(&self) -> &'static [&'static str] {
        &["elixir", "elixirc", "mix", "iex"]
    }

    fn list_releases(&self, ctx: &ResolveContext<'_>) -> Result<Vec<Release>, ProviderError> {
        github::releases(REPO, ctx, github::plain_version)
    }

    fn resolve_artifact(
        &self,
        release: &Release,
        ctx: &ResolveContext<'_>,
    ) -> Result<ArtifactPlan, ProviderError> {
        let assets = github::assets(REPO, &release.tag, ctx)?;
        let asset = Self::pick_asset(&assets).ok_or_else(|| {
            ProviderError::Unsupported(format!(
                "Elixir {} has no precompiled elixir-otp-*.zip",
                release.version
            ))
        })?;
        let checksum_name = format!("{}.sha256sum", asset.name);
        github::plan(&assets, &asset.name, Some(&checksum_name), ctx)
    }

    fn install_layout(
        &self,
        _extracted: &std::path::Path,
        _target: &std::path::Path,
    ) -> Result<Layout, ProviderError> {
        let mut layout = Layout::new("bin", self.entrypoints());
        layout.notes.push(
            "Elixir requires Erlang/OTP; install it separately (lania does not manage Erlang yet)"
                .to_string(),
        );
        Ok(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_highest_otp_build() {
        let assets = [
            github::Asset {
                name: "elixir-otp-25.zip".into(),
                url: "https://example.invalid/25".into(),
            },
            github::Asset {
                name: "elixir-otp-27.zip".into(),
                url: "https://example.invalid/27".into(),
            },
            github::Asset {
                name: "elixir-otp-26.zip.sha256sum".into(),
                url: "https://example.invalid/26".into(),
            },
            github::Asset {
                name: "Docs.zip".into(),
                url: "https://example.invalid/docs".into(),
            },
        ];
        assert_eq!(
            ElixirProvider::pick_asset(&assets).unwrap().name,
            "elixir-otp-27.zip"
        );
    }
}
