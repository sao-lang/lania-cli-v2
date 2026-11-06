use std::time::{SystemTime, UNIX_EPOCH};

use super::{FsService, IgnoreOptions, IgnoreProfile, PathSplit, PlannedFile};

fn temp_path(name: &str) -> std::path::PathBuf {
    // 每个测试都生成独立临时目录，避免测试之间互相污染。
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    std::env::temp_dir().join(format!("lania-fs-{name}-{unique}"))
}

#[test]
fn writes_files_and_reports_conflicts() {
    // 覆盖 `write_files(overwrite = false)` 的关键语义：
    // - 第一次写应成功
    // - 第二次写同一路径不报错，而是落到 conflicts
    let service = FsService;
    let root = temp_path("write");
    let target = root.join("demo.txt");

    let first = service
        .write_files(
            &[PlannedFile {
                path: target.clone(),
                content: "hello".into(),
            }],
            false,
        )
        .expect("first write succeeds");
    assert_eq!(first.written, vec![target.clone()]);

    let second = service
        .write_files(
            &[PlannedFile {
                path: target.clone(),
                content: "world".into(),
            }],
            false,
        )
        .expect("second write returns conflict");
    assert_eq!(second.conflicts, vec![target.clone()]);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn mirrors_path_helpers_from_legacy_utils() {
    // 这些 helper 看起来简单，但很多上层逻辑（路径拆分、绝对路径判定）
    // 都依赖它们保持与旧实现兼容，所以这里做回归测试。
    let service = FsService;

    assert_eq!(service.file_ext("/tmp/demo.ts").as_deref(), Some("ts"));
    assert!(service.is_unix_absolute_dir_path("/tmp/demo"));
    assert!(!service.is_unix_absolute_dir_path("/tmp//demo"));
    assert!(service.is_unix_absolute_file_path("/tmp/demo.ts"));
    assert!(!service.is_unix_absolute_file_path("/tmp/demo"));
    assert_eq!(
        service.split_directory_and_file_name("/tmp/demo.ts"),
        PathSplit {
            directory_path: std::path::PathBuf::from("/tmp"),
            base_name: Some("demo.ts".into()),
        }
    );
    assert_eq!(
        service.split_directory_and_file_name("/tmp/demo/"),
        PathSplit {
            directory_path: std::path::PathBuf::from("/tmp/demo"),
            base_name: None,
        }
    );
}

#[test]
fn traverses_files_recursively() {
    // 验证递归遍历 + 自定义过滤器可以组合工作。
    // 这里故意同时创建 `.ts` 和 `.md`，确保过滤逻辑真的发生了，而不是“目录里本来就只有一个文件”。
    let service = FsService;
    let root = temp_path("traverse");
    let src = root.join("src");
    let nested = src.join("nested");
    std::fs::create_dir_all(&nested).expect("dirs created");
    std::fs::write(src.join("main.ts"), "console.log('hi')").expect("main created");
    std::fs::write(nested.join("notes.md"), "# notes").expect("notes created");

    let files = service
        .list_files_recursive_filtered(&root, |path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("ts")
        })
        .expect("recursive traversal succeeds");

    assert_eq!(files.len(), 1);
    assert!(service.file_exists(src.join("main.ts")));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn combines_standard_ignore_files_and_negative_rules() {
    // 这个测试比较重要：
    // - 同时覆盖标准 ignore 文件读取
    // - 覆盖否定规则 `!important.txt`
    // - 覆盖 sources() 是否能暴露规则来源
    let service = FsService;
    let root = temp_path("ignore-standard");
    std::fs::create_dir_all(root.join("src")).expect("src dir created");
    std::fs::write(root.join(".gitignore"), "dist/\n!important.txt\n").expect("gitignore written");
    std::fs::write(root.join(".eslintignore"), "src/generated.ts\n").expect("eslintignore written");
    std::fs::write(root.join("dist.js"), "keep").expect("file written");
    std::fs::write(root.join("important.txt"), "keep").expect("file written");
    std::fs::write(root.join("src/generated.ts"), "generated").expect("file written");

    let matcher = service
        .build_ignore_matcher(IgnoreOptions {
            root_dir: Some(root.clone()),
            include_standard_files: true,
            ..IgnoreOptions::default()
        })
        .expect("ignore matcher builds");

    assert!(matcher.is_ignored(root.join("src/generated.ts")));
    assert!(!matcher.is_ignored(root.join("important.txt")));
    assert!(!matcher.is_ignored(root.join("dist.js")));
    assert!(matcher
        .sources()
        .iter()
        .any(|source| source.name == ".gitignore"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn supports_profile_based_ignore_patterns() {
    // profile 是“内建忽略规则模板”，不依赖项目真的存在 .gitignore。
    // 这保证了 CLI 在新项目、模板目录甚至临时目录里也能复用同一套排除策略。
    let service = FsService;
    let root = temp_path("ignore-profile");
    std::fs::create_dir_all(root.join("dist")).expect("dist dir created");
    std::fs::create_dir_all(root.join("src")).expect("src dir created");
    std::fs::write(root.join("dist/app.js"), "compiled").expect("dist file written");
    std::fs::write(root.join("src/main.ts"), "source").expect("src file written");

    let matcher = service
        .build_ignore_matcher(IgnoreOptions {
            root_dir: Some(root.clone()),
            profile: IgnoreProfile::Dev,
            ..IgnoreOptions::default()
        })
        .expect("profile matcher builds");

    let files = service
        .list_files_with_ignore(&root, &matcher)
        .expect("files filtered");
    assert!(files.iter().any(|path| path.ends_with("src/main.ts")));
    assert!(!files.iter().any(|path| path.ends_with("dist/app.js")));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn exposes_ignore_profiles() {
    // 这个测试不碰文件系统，只验证 profile -> patterns 的静态映射是否暴露正确。
    let service = FsService;
    let profiles = service.ignore_profiles();
    assert!(profiles[&IgnoreProfile::Build].contains(&"node_modules/".to_string()));
    assert!(profiles[&IgnoreProfile::Publish].contains(&"src/".to_string()));
}
