//! 版本号：解析、比较、匹配（`20` → `20.x.y` 中的最高版本）。

use serde::{Deserialize, Serialize};

/// 上游版本的 tag 与发布通道（`stable` / `nightly` / `rc`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionTag {
    /// 原始文本（保留上游写法，例如 `1.86.0`、`3.12.4`）。
    pub raw: String,
    /// 语义化数字段，用于比较与 `20` → `20.11.1` 的匹配。
    pub parts: Vec<u64>,
    /// 是否为预发布（beta / rc / nightly 等）。
    pub prerelease: bool,
}

impl VersionTag {
    /// 解析版本号。允许前缀 `v`；带 `-alpha` / `-rc1` 之类后缀的记为预发布。
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }
        let trimmed = trimmed.strip_prefix('v').unwrap_or(trimmed);
        let end = trimmed
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(trimmed.len());
        let core = trimmed[..end].trim_end_matches('.');
        if core.is_empty() {
            return None;
        }
        let parts: Vec<u64> = core
            .split('.')
            .map(|item| item.parse::<u64>())
            .collect::<Result<_, _>>()
            .ok()?;
        Some(Self {
            raw: trimmed.to_string(),
            parts,
            prerelease: end < trimmed.len(),
        })
    }

    /// 用户输入的版本约束是否匹配这个具体版本（`20` 匹配 `20.11.1`）。
    pub fn matches(&self, wanted: &VersionTag) -> bool {
        if wanted.parts.len() > self.parts.len() {
            return false;
        }
        self.parts[..wanted.parts.len()] == wanted.parts[..]
    }

    pub fn same(&self, other: &VersionTag) -> bool {
        self.parts == other.parts
    }
}

impl std::fmt::Display for VersionTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}

/// 从一组候选版本里挑出满足约束的最高版本。
///
/// 传入的列表按「新 → 旧」排序（上游 API 的常规顺序）。优先精确命中，
/// 其次取第一个前缀匹配项；**预发布版本只在没有稳定版可选时**才使用，
/// 避免 `lan env install go 1.22` 装到 `1.22rc2`。
pub fn pick_best<'a>(available: &'a [VersionTag], wanted: &VersionTag) -> Option<&'a VersionTag> {
    pick_best_filtered(available, wanted, true)
        .or_else(|| pick_best_filtered(available, wanted, false))
}

fn pick_best_filtered<'a>(
    available: &'a [VersionTag],
    wanted: &VersionTag,
    stable_only: bool,
) -> Option<&'a VersionTag> {
    available
        .iter()
        .filter(|candidate| !stable_only || !candidate.prerelease)
        .find(|candidate| candidate.same(wanted))
        .or_else(|| {
            available
                .iter()
                .filter(|candidate| !stable_only || !candidate.prerelease)
                .find(|candidate| candidate.matches(wanted))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions(values: &[&str]) -> Vec<VersionTag> {
        values.iter().filter_map(|v| VersionTag::parse(v)).collect()
    }

    #[test]
    fn parses_common_shapes() {
        assert_eq!(VersionTag::parse("20").unwrap().parts, vec![20]);
        assert_eq!(
            VersionTag::parse("v20.11.1").unwrap().parts,
            vec![20, 11, 1]
        );
        assert_eq!(VersionTag::parse("3.12.4").unwrap().parts, vec![3, 12, 4]);
        assert!(!VersionTag::parse("20.11.1").unwrap().prerelease);
        assert!(VersionTag::parse("20.11.1-rc1").unwrap().prerelease);
        assert!(VersionTag::parse("stable").is_none());
        assert!(VersionTag::parse("").is_none());
    }

    #[test]
    fn matching_is_prefix_based() {
        let full = VersionTag::parse("20.11.1").unwrap();
        assert!(full.matches(&VersionTag::parse("20").unwrap()));
        assert!(full.matches(&VersionTag::parse("20.11").unwrap()));
        assert!(!full.matches(&VersionTag::parse("21").unwrap()));
        assert!(!full.matches(&VersionTag::parse("20.11.1.1").unwrap()));
    }

    #[test]
    fn pick_best_prefers_exact_then_first_prefix() {
        let available = versions(&["22.21.1", "20.18.1", "20.11.1", "18.20.8"]);
        let wanted = VersionTag::parse("20").unwrap();
        assert_eq!(pick_best(&available, &wanted).unwrap().raw, "20.18.1");

        let exact = VersionTag::parse("20.11.1").unwrap();
        assert_eq!(pick_best(&available, &exact).unwrap().raw, "20.11.1");
    }

    #[test]
    fn pick_best_prefers_stable_over_prerelease() {
        let available = versions(&["1.22rc2", "1.22.12", "1.21.13"]);
        // `1.22rc2` 解析成 1.22.0 的预发布：前缀匹配时应跳过它。
        let wanted = VersionTag::parse("1.22").unwrap();
        assert_eq!(pick_best(&available, &wanted).unwrap().raw, "1.22.12");

        // 只有当该前缀下没有稳定版时才回退到预发布。
        let only_pre = versions(&["1.23rc1"]);
        let wanted = VersionTag::parse("1.23").unwrap();
        assert_eq!(pick_best(&only_pre, &wanted).unwrap().raw, "1.23rc1");
    }
}
