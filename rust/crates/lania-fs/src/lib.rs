//! 文件系统规划与写入抽象，跟踪目录创建、文件落盘和冲突。
//!
//! 主要导出：is_ignored、root_dir、sources、file_ext、ensure_dir、exists。
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    // “计划写入的文件”：
    // - path：最终要写到哪里
    // - content：最终要写的内容（通常已经过 formatter/hook 处理）
    //
    // FsService 的核心定位是“只负责落盘与冲突判断”，不负责生成内容。
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReport {
    // 写盘结果分成两类：
    // - written：真正写成功的文件路径
    // - conflicts：因为 overwrite=false 且目标已存在而跳过的路径
    //
    // 这种“可部分成功”返回对脚手架类工具很重要：
    // - 用户可以先看冲突列表，再决定是否 `--force` 重跑
    pub written: Vec<PathBuf>,
    pub conflicts: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSplit {
    pub directory_path: PathBuf,
    pub base_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IgnoreProfile {
    #[default]
    None,
    Dev,
    Build,
    Lint,
    Publish,
    Template,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoreRuleSource {
    pub name: String,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct IgnoreOptions {
    pub root_dir: Option<PathBuf>,
    pub ignore_file_path: Option<PathBuf>,
    pub ignore_patterns: Vec<String>,
    pub include_standard_files: bool,
    pub profile: IgnoreProfile,
}

#[derive(Debug)]
pub struct IgnoreMatcher {
    root_dir: PathBuf,
    sources: Vec<IgnoreRuleSource>,
    matcher: Gitignore,
}

impl IgnoreMatcher {
    pub fn is_ignored(&self, path: impl AsRef<Path>) -> bool {
        // `ignore` crate 的 matcher 需要一个“尽量规范化”的绝对路径：
        // - 如果传入的是相对路径，先基于 root_dir 拼成绝对路径
        // - 再 canonicalize（失败就保留原值），尽量消除 `.`/`..` 带来的差异
        //
        // 这样可以提高 ignore 判断的稳定性，避免同一个文件因为路径表现不同而匹配结果不一致。
        let absolute = if path.as_ref().is_absolute() {
            path.as_ref().to_path_buf()
        } else {
            self.root_dir.join(path)
        };
        let absolute = absolute.canonicalize().unwrap_or(absolute);
        self.matcher
            .matched_path_or_any_parents(&absolute, absolute.is_dir())
            .is_ignore()
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn sources(&self) -> &[IgnoreRuleSource] {
        &self.sources
    }
}

#[derive(Debug, Clone, Default)]
pub struct FsService;

impl FsService {
    pub fn file_ext(&self, path: impl AsRef<Path>) -> Option<String> {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(ToOwned::to_owned)
    }

    pub fn ensure_dir(&self, path: impl AsRef<Path>) -> Result<()> {
        fs::create_dir_all(path.as_ref())
            .with_context(|| format!("failed to create directory {}", path.as_ref().display()))
    }

    pub fn exists(&self, path: impl AsRef<Path>) -> bool {
        path.as_ref().exists()
    }

    pub fn file_exists(&self, path: impl AsRef<Path>) -> bool {
        path.as_ref().is_file()
    }

    pub fn is_unix_absolute_dir_path(&self, path: &str) -> bool {
        // 这个 helper 的目标不是做完整 POSIX 路径校验，
        // 而是回答“它是否看起来像一个可接受的绝对目录路径”。
        // 因此规则偏保守，主要服务于 CLI 输入预检与兼容旧工具行为。
        is_valid_unix_absolute_path(path) && !path[1..].contains("//")
    }

    pub fn is_unix_absolute_file_path(&self, path: &str) -> bool {
        if !is_valid_unix_absolute_path(path) {
            return false;
        }

        // 这里用“最后一段里带点号”粗略判断文件路径。
        // 它不追求 100% 文件系统语义正确，而是满足当前 CLI 场景下
        // 对“像不像文件路径”的快速区分。
        let last_segment = path.rsplit('/').next().unwrap_or_default();
        last_segment.contains('.') && !last_segment.ends_with('.')
    }

    pub fn split_directory_and_file_name(&self, path: &str) -> PathSplit {
        if path.is_empty() {
            return PathSplit {
                directory_path: PathBuf::new(),
                base_name: None,
            };
        }

        let has_trailing_slash = path.ends_with('/');
        let normalized = if has_trailing_slash && path.len() > 1 {
            &path[..path.len() - 1]
        } else {
            path
        };

        if has_trailing_slash {
            // 带结尾 `/` 的输入被明确视为“目录语义”，即使最后一段看起来像文件名。
            return PathSplit {
                directory_path: PathBuf::from(normalized),
                base_name: None,
            };
        }

        match normalized.rsplit_once('/') {
            Some((directory, file_name)) => PathSplit {
                directory_path: PathBuf::from(directory),
                base_name: Some(file_name.to_string()),
            },
            None => PathSplit {
                directory_path: PathBuf::new(),
                base_name: Some(normalized.to_string()),
            },
        }
    }

    pub fn list_files_recursive(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        self.list_files_recursive_filtered(dir, |_| true)
    }

    pub fn list_files_recursive_filtered<F>(
        &self,
        dir: impl AsRef<Path>,
        mut filter: F,
    ) -> Result<Vec<PathBuf>>
    where
        F: FnMut(&Path) -> bool,
    {
        let mut files = Vec::new();
        // 递归遍历和过滤逻辑拆开，意味着调用方可以把“怎么走目录”和“保留哪些文件”
        // 当成两个独立问题处理。
        self.walk(dir.as_ref(), &mut files, &mut filter)?;
        Ok(files)
    }

    pub fn list_files_with_ignore(
        &self,
        dir: impl AsRef<Path>,
        matcher: &IgnoreMatcher,
    ) -> Result<Vec<PathBuf>> {
        self.list_files_recursive_filtered(dir, |path| !matcher.is_ignored(path))
    }

    pub fn build_ignore_matcher(&self, options: IgnoreOptions) -> Result<IgnoreMatcher> {
        // ignore matcher 的输入可能来自多处：
        // - 某个显式 ignore 文件（options.ignore_file_path）
        // - 标准 ignore 文件（.gitignore/.eslintignore/...）
        // - profile 预置（dev/build/lint/publish/template）
        // - CLI 临时传入的 ignore_patterns
        //
        // 这里统一把它们合并成一个 `ignore::Gitignore` matcher，
        // 并把来源 sources 记录下来，方便调试“到底是哪个规则把文件排除掉了”。
        let root_dir = options
            .root_dir
            .unwrap_or_else(|| std::env::current_dir().expect("cwd available"));
        let root_dir = root_dir.canonicalize().unwrap_or_else(|_| root_dir.clone());

        let mut builder = GitignoreBuilder::new(&root_dir);
        let mut sources = Vec::new();

        if let Some(ignore_file) = options.ignore_file_path.as_ref() {
            let content = fs::read_to_string(ignore_file)
                .with_context(|| format!("failed to read ignore file {}", ignore_file.display()))?;
            let patterns = content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            for pattern in &patterns {
                builder
                    .add_line(Some(ignore_file.to_path_buf()), pattern)
                    .with_context(|| format!("invalid ignore rule `{pattern}`"))?;
            }
            sources.push(IgnoreRuleSource {
                name: ignore_file.display().to_string(),
                patterns,
            });
        }

        if options.include_standard_files {
            for file_name in [
                ".gitignore",
                ".eslintignore",
                ".prettierignore",
                ".npmignore",
            ] {
                let path = root_dir.join(file_name);
                if path.exists() {
                    let content = fs::read_to_string(&path)
                        .with_context(|| format!("failed to read {}", path.display()))?;
                    let patterns = content
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>();
                    for pattern in &patterns {
                        builder
                            .add_line(Some(path.clone()), pattern)
                            .with_context(|| format!("invalid ignore rule `{pattern}`"))?;
                    }
                    sources.push(IgnoreRuleSource {
                        name: file_name.into(),
                        patterns,
                    });
                }
            }
        }

        let profile_patterns = profile_patterns(options.profile);
        if !profile_patterns.is_empty() {
            for pattern in &profile_patterns {
                builder
                    .add_line(None, pattern)
                    .with_context(|| format!("invalid profile ignore rule `{pattern}`"))?;
            }
            sources.push(IgnoreRuleSource {
                name: format!("profile:{:?}", options.profile),
                patterns: profile_patterns,
            });
        }

        if !options.ignore_patterns.is_empty() {
            for pattern in &options.ignore_patterns {
                builder
                    .add_line(None, pattern)
                    .with_context(|| format!("invalid ignore rule `{pattern}`"))?;
            }
            sources.push(IgnoreRuleSource {
                name: "inline".into(),
                patterns: options.ignore_patterns,
            });
        }

        let matcher = builder.build().with_context(|| {
            format!("failed to build ignore matcher for {}", root_dir.display())
        })?;

        Ok(IgnoreMatcher {
            root_dir,
            sources,
            matcher,
        })
    }

    pub fn find_conflicts(&self, files: &[PlannedFile]) -> Vec<PathBuf> {
        // 这个函数只做“存在性检测”，不做权限/内容比较。
        // 设计意图：上层在真正写之前可以先快速给用户一个冲突预览。
        files
            .iter()
            .filter(|file| file.path.exists())
            .map(|file| file.path.clone())
            .collect()
    }

    pub fn write_files(&self, files: &[PlannedFile], overwrite: bool) -> Result<WriteReport> {
        // 写文件采用逐个写入，而不是批量事务：
        // - CLI 的写入通常是幂等的，且文件间不存在强事务一致性要求
        // - 逐个写入能让错误信息更明确（哪个文件失败、为什么失败）
        //
        // 注意：这里的 overwrite 语义是“是否允许覆盖已存在文件”。
        // 若 overwrite=false，遇到已存在文件不会报错，而是记录到 conflicts 并跳过。
        let mut written = Vec::new();
        let mut conflicts = Vec::new();

        for file in files {
            if file.path.exists() && !overwrite {
                // 这里把“已存在且不允许覆盖”视为业务冲突，而不是系统错误。
                // 这样调用方还能拿到一个完整 `WriteReport`，决定是提示用户重试 `--force`
                // 还是先人工处理局部冲突。
                conflicts.push(file.path.clone());
                continue;
            }

            if let Some(parent) = file.path.parent() {
                // 逐文件确保父目录存在，意味着上层不用提前关心写入顺序：
                // 即使先写深层文件、后写浅层文件，也能正常工作。
                self.ensure_dir(parent)?;
            }

            fs::write(&file.path, &file.content)
                .with_context(|| format!("failed to write file {}", file.path.display()))?;
            written.push(file.path.clone());
        }

        Ok(WriteReport { written, conflicts })
    }

    pub fn ignore_profiles(&self) -> BTreeMap<IgnoreProfile, Vec<String>> {
        [
            IgnoreProfile::Dev,
            IgnoreProfile::Build,
            IgnoreProfile::Lint,
            IgnoreProfile::Publish,
            IgnoreProfile::Template,
        ]
        .into_iter()
        .map(|profile| (profile, profile_patterns(profile)))
        .collect()
    }

    fn walk<F>(&self, dir: &Path, files: &mut Vec<PathBuf>, filter: &mut F) -> Result<()>
    where
        F: FnMut(&Path) -> bool,
    {
        for entry in fs::read_dir(dir)
            .with_context(|| format!("failed to read directory {}", dir.display()))?
        {
            let entry =
                entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .with_context(|| format!("failed to read metadata for {}", path.display()))?;
            if metadata.is_dir() {
                // 当前实现是深度优先遍历（DFS）。
                // 对 CLI 这类场景来说，顺序本身通常不重要，重要的是实现简单且行为稳定。
                self.walk(&path, files, filter)?;
            } else if metadata.is_file() && filter(&path) {
                files.push(path);
            }
        }
        Ok(())
    }
}

fn profile_patterns(profile: IgnoreProfile) -> Vec<String> {
    match profile {
        IgnoreProfile::None => Vec::new(),
        IgnoreProfile::Dev => vec!["dist/".into(), "coverage/".into()],
        IgnoreProfile::Build => vec!["node_modules/".into(), ".cache/".into()],
        IgnoreProfile::Lint => vec!["dist/".into(), "coverage/".into(), "node_modules/".into()],
        IgnoreProfile::Publish => vec!["src/".into(), "tests/".into(), "*.log".into()],
        IgnoreProfile::Template => vec!["node_modules/".into(), ".git/".into()],
    }
}

fn is_valid_unix_absolute_path(path: &str) -> bool {
    if path.trim().is_empty() || !path.starts_with('/') || path.contains('\\') {
        return false;
    }

    if path.contains('\0') {
        return false;
    }

    if path == "/" {
        return true;
    }

    !path.split('/').skip(1).any(|segment| segment.is_empty())
}

#[cfg(test)]
mod tests;
