use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

// 只负责把一个输入路径扩展为最终参与编译的源文件列表。
// 这里不解析 schema 内容，也不理解 module entry 语义，保持为纯文件发现层。
pub(super) fn collect_source_files(
    base_path: &Path,
    include_patterns: &[String],
) -> Result<Vec<PathBuf>> {
    // 允许直接把单个文件作为输入，这样调用方不必为了只生成一个 schema
    // 再额外创建目录结构。
    if base_path.is_file() {
        return Ok(vec![base_path.to_path_buf()]);
    }
    if !base_path.exists() {
        return Err(anyhow!(
            "source path does not exist: {}",
            base_path.display()
        ));
    }
    let mut files = Vec::new();
    collect_source_files_recursive(base_path, base_path, include_patterns, &mut files)?;
    files.sort();
    files.dedup();
    Ok(files)
}

// 递归扫描时统一转成相对 root 的 `/` 风格路径，再用 include pattern 匹配。
// 这样 Windows/Unix 的路径分隔符差异不会泄漏到匹配逻辑中。
fn collect_source_files_recursive(
    root: &Path,
    current: &Path,
    include_patterns: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in std::fs::read_dir(current)
        .with_context(|| format!("failed to read source directory {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_source_files_recursive(root, &path, include_patterns, files)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if include_patterns
            .iter()
            .any(|pattern| matches_module_include_pattern(&relative, pattern))
        {
            files.push(path);
        }
    }
    Ok(())
}

// 当前只支持一组轻量级 include 规则，目标是覆盖模块生成配置里的常见写法，
// 而不是实现一个完整的 glob 引擎。
fn matches_module_include_pattern(relative: &str, pattern: &str) -> bool {
    let normalized = pattern.replace('\\', "/");
    if let Some(ext) = normalized.strip_prefix("**/*.") {
        return relative.ends_with(&format!(".{ext}"));
    }
    if let Some(ext) = normalized.strip_prefix("*.") {
        return relative.ends_with(&format!(".{ext}"));
    }
    if normalized.contains('*') {
        let suffix = normalized.trim_start_matches("**/");
        return relative.ends_with(suffix.trim_start_matches('*'));
    }
    relative == normalized || relative.ends_with(&normalized)
}
