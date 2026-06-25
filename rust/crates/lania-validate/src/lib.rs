//! Validation helpers shared across crates.
//!
//! This crate intentionally exports a camelCase wrapper (`validateVersionName`) because some
//! callers (especially bridge / JSON-driven layers) prefer that naming style.
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionNameRule {
    /// Accept common semver formats (with optional leading `v`), e.g. `1.2.3`, `v1.2.3`,
    /// `1.2.3-beta.1`, `1.2.3+build.5`.
    Semver,
    /// Validate a git ref name (suitable for `refs/tags/<name>`), a practical subset of
    /// `git check-ref-format` rules.
    GitTag,
    /// Validate npm/pnpm dist-tag (e.g. `latest`, `next`). Also rejects strings that look like
    /// semver to avoid ambiguity.
    NpmDistTag,
    /// Minimal safety: non-empty, no whitespace/control chars.
    Loose,
}

impl fmt::Display for VersionNameRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Semver => "semver",
            Self::GitTag => "git_tag",
            Self::NpmDistTag => "npm_dist_tag",
            Self::Loose => "loose",
        };
        write!(f, "{value}")
    }
}

/// Validate a version-like name with the given rule.
pub fn validate_version_name(value: &str, rule: VersionNameRule) -> Result<()> {
    let value = value.trim();
    if value.is_empty() {
        return Err(anyhow!("version name is empty"));
    }

    // Common sanity checks across all strategies.
    if value.chars().any(|ch| ch.is_ascii_control()) {
        return Err(anyhow!("version name contains control characters"));
    }
    if value.chars().any(|ch| ch.is_whitespace()) {
        return Err(anyhow!("version name contains whitespace"));
    }
    // Avoid extremely long tokens being used in command lines / refs.
    if value.len() > 200 {
        return Err(anyhow!("version name is too long (max 200 chars)"));
    }

    match rule {
        VersionNameRule::Loose => Ok(()),
        VersionNameRule::Semver => validate_semver_like(value)
            .map_err(|message| anyhow!("invalid semver version name `{}`: {}", value, message)),
        VersionNameRule::GitTag => validate_git_ref_name(value)
            .map_err(|message| anyhow!("invalid git tag name `{}`: {}", value, message)),
        VersionNameRule::NpmDistTag => validate_npm_dist_tag(value)
            .map_err(|message| anyhow!("invalid npm dist-tag `{}`: {}", value, message)),
    }
}

/// CamelCase wrapper to match the requested "tool" naming.
#[allow(non_snake_case)]
pub fn validateVersionName(value: &str, rule: VersionNameRule) -> Result<()> {
    validate_version_name(value, rule)
}

fn validate_semver_like(value: &str) -> std::result::Result<(), Cow<'static, str>> {
    // Allow a leading 'v' as a common tag/version prefix.
    let value = value.strip_prefix('v').unwrap_or(value);
    if value.is_empty() {
        return Err("missing version after leading `v`".into());
    }

    let mut split = value.splitn(2, '+');
    let core_and_pre = split.next().unwrap_or_default();
    let build = split.next();
    if let Some(build) = build {
        if build.is_empty() {
            return Err("empty build metadata".into());
        }
        if !build
            .split('.')
            .all(|seg| !seg.is_empty() && seg.chars().all(is_semver_ident_char))
        {
            return Err("invalid build metadata".into());
        }
    }

    let mut split = core_and_pre.splitn(2, '-');
    let core = split.next().unwrap_or_default();
    let pre = split.next();

    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err("core version must be MAJOR.MINOR.PATCH".into());
    }
    for part in parts {
        if part.is_empty() {
            return Err("empty numeric identifier".into());
        }
        // Semver forbids leading zeros except for zero itself.
        if part.len() > 1 && part.starts_with('0') {
            return Err("numeric identifier has leading zeros".into());
        }
        u64::from_str(part).map_err(|_| "numeric identifier is not a number")?;
    }

    if let Some(pre) = pre {
        if pre.is_empty() {
            return Err("empty pre-release section".into());
        }
        for ident in pre.split('.') {
            if ident.is_empty() {
                return Err("empty pre-release identifier".into());
            }
            if !ident.chars().all(is_semver_ident_char) {
                return Err("invalid pre-release identifier".into());
            }
            // Pre-release numeric identifiers also forbid leading zeros.
            if ident.chars().all(|ch| ch.is_ascii_digit())
                && ident.len() > 1
                && ident.starts_with('0')
            {
                return Err("pre-release numeric identifier has leading zeros".into());
            }
        }
    }

    Ok(())
}

fn is_semver_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-'
}

fn validate_npm_dist_tag(value: &str) -> std::result::Result<(), Cow<'static, str>> {
    // npm dist-tag is usually URL-ish: allow ascii alnum + . _ - (no slashes).
    if value.len() > 100 {
        return Err("dist-tag is too long (max 100 chars)".into());
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err("dist-tag may only contain [A-Za-z0-9._-]".into());
    }
    if value.starts_with('.') || value.starts_with('_') || value.starts_with('-') {
        return Err("dist-tag must not start with '.', '_' or '-'".into());
    }
    if value.ends_with('.') {
        return Err("dist-tag must not end with '.'".into());
    }
    // Dist-tag must not look like a semver version, otherwise npm treats it ambiguously.
    if validate_semver_like(value).is_ok() {
        return Err("dist-tag must not be a semver-like version string".into());
    }
    Ok(())
}

fn validate_git_ref_name(value: &str) -> std::result::Result<(), Cow<'static, str>> {
    // Practical subset of `git check-ref-format` restrictions:
    // - no ASCII control, space, ~ ^ : ? * [ \
    // - no consecutive dots ("..")
    // - no "@{" sequence
    // - no double slashes
    // - no components starting with '.' or ending with '.lock'
    // - not start/end with '/'
    if value.starts_with('/') || value.ends_with('/') {
        return Err("ref name must not start or end with '/'".into());
    }
    if value.contains("..") {
        return Err("ref name must not contain '..'".into());
    }
    if value.contains("@{") {
        return Err("ref name must not contain '@{'".into());
    }
    if value.contains("//") {
        return Err("ref name must not contain '//'".into());
    }
    if value.ends_with(".lock") {
        return Err("ref name must not end with '.lock'".into());
    }

    for ch in value.chars() {
        if matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\') {
            return Err("ref name contains forbidden characters".into());
        }
        if ch.is_whitespace() {
            return Err("ref name contains whitespace".into());
        }
        if ch.is_ascii_control() {
            return Err("ref name contains control characters".into());
        }
    }

    for part in value.split('/') {
        if part.is_empty() {
            return Err("ref name contains empty path component".into());
        }
        if part.starts_with('.') {
            return Err("ref name path component must not start with '.'".into());
        }
        if part.ends_with('.') {
            return Err("ref name path component must not end with '.'".into());
        }
        if part.ends_with(".lock") {
            return Err("ref name path component must not end with '.lock'".into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_version_name, VersionNameRule};

    #[test]
    fn accepts_common_semver_formats() {
        for value in ["1.2.3", "1.2.3-beta.1", "1.2.3+build.5", "v1.2.3"] {
            validate_version_name(value, VersionNameRule::Semver).expect("valid semver");
        }
    }

    #[test]
    fn rejects_invalid_semver() {
        for value in ["", "1.2", "01.2.3", "1.2.03", "1.2.3-", "v"] {
            validate_version_name(value, VersionNameRule::Semver).expect_err("invalid semver");
        }
    }

    #[test]
    fn validates_npm_dist_tag() {
        validate_version_name("latest", VersionNameRule::NpmDistTag).expect("valid tag");
        validate_version_name("next-1", VersionNameRule::NpmDistTag).expect("valid tag");
        validate_version_name("1.2.3", VersionNameRule::NpmDistTag)
            .expect_err("semver-like tag rejected");
        validate_version_name("bad tag", VersionNameRule::NpmDistTag)
            .expect_err("whitespace rejected");
    }

    #[test]
    fn validates_git_tag_name() {
        validate_version_name("v1.2.3", VersionNameRule::GitTag).expect("valid tag");
        validate_version_name("release/v1.2.3", VersionNameRule::GitTag).expect("valid tag");
        validate_version_name("bad tag", VersionNameRule::GitTag).expect_err("whitespace rejected");
        validate_version_name("a..b", VersionNameRule::GitTag).expect_err(".. rejected");
        validate_version_name("@{", VersionNameRule::GitTag).expect_err("@{ rejected");
    }
}
