use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    directory::{render_directory_listing, DirectoryListingOptions},
    file_kind::inspect_view_file_kind,
    render::{render_hex_preview, render_with_line_numbers},
    DirectoryEntryFilter, DirectorySort, ViewFileKind, ViewMatcher,
};

#[test]
fn renders_text_with_line_numbers() {
    let rendered = render_with_line_numbers(
        "a
b
c
", 2, 1, None, None, None, None,
    );
    assert_eq!(rendered.displayed_lines, 2);
    assert!(rendered.truncated);
    assert_eq!(
        rendered.text,
        "     1 | a
     2 | b"
    );
    assert_eq!(rendered.start_line, 1);
    assert_eq!(rendered.end_line, 2);
}

#[test]
fn renders_text_with_tail_and_range() {
    let range = render_with_line_numbers(
        "a
b
c
d
",
        10,
        2,
        Some(3),
        None,
        None,
        None,
    );
    assert_eq!(
        range.text,
        "     2 | b
     3 | c"
    );
    assert_eq!(range.start_line, 2);
    assert_eq!(range.end_line, 3);
    assert!(!range.truncated);

    let tail = render_with_line_numbers(
        "a
b
c
d
",
        10,
        1,
        None,
        Some(2),
        None,
        None,
    );
    assert_eq!(
        tail.text,
        "     3 | c
     4 | d"
    );
    assert_eq!(tail.start_line, 3);
    assert_eq!(tail.end_line, 4);
}

#[test]
fn renders_text_with_head_and_grep() {
    let head = render_with_line_numbers(
        "alpha
beta
gamma
",
        10,
        1,
        None,
        None,
        Some(2),
        None,
    );
    assert_eq!(
        head.text,
        "     1 | alpha
     2 | beta"
    );
    assert_eq!(head.start_line, 1);
    assert_eq!(head.end_line, 2);

    let matcher = ViewMatcher::Substring {
        needle: "alpha".into(),
        ignore_case: false,
    };
    let grep = render_with_line_numbers(
        "alpha
beta
alphabet
",
        10,
        1,
        None,
        None,
        None,
        Some(&matcher),
    );
    assert_eq!(
        grep.text,
        "     1 | alpha
     3 | alphabet"
    );
    assert_eq!(grep.total_count, 2);
}

#[test]
fn classifies_view_file_types() {
    let png = std::path::PathBuf::from("/tmp/demo.png");
    let pdf = std::path::PathBuf::from("/tmp/demo.pdf");
    let txt = unique_temp_dir("tools-view-kind").join("note.txt");
    fs::create_dir_all(txt.parent().expect("txt parent")).expect("dir created");
    fs::write(
        &txt,
        "hello
world
",
    )
    .expect("txt written");

    assert_eq!(
        inspect_view_file_kind(png.as_path()).expect("png kind"),
        ViewFileKind::Image
    );
    assert_eq!(
        inspect_view_file_kind(pdf.as_path()).expect("pdf kind"),
        ViewFileKind::Pdf
    );
    assert_eq!(
        inspect_view_file_kind(txt.as_path()).expect("text kind"),
        ViewFileKind::Text
    );

    let _ = fs::remove_dir_all(txt.parent().expect("txt parent"));
}

#[test]
fn renders_hex_preview_for_binary_content() {
    let file = unique_temp_dir("tools-view-hex").join("blob.bin");
    fs::create_dir_all(file.parent().expect("parent")).expect("dir created");
    fs::write(&file, [0_u8, 1, 2, 65, 66, 67, 255]).expect("bin written");

    let preview = render_hex_preview(&file, 7).expect("hex preview");
    assert_eq!(preview.line_count, 1);
    assert!(preview.text.contains("00000000"));
    assert!(preview.text.contains("41 42 43"));

    let _ = fs::remove_dir_all(file.parent().expect("parent"));
}

#[test]
fn filters_directory_entries_by_type() {
    let root = unique_temp_dir("tools-view-dir-filter");
    let subdir = root.join("nested");
    let file = root.join("a.txt");
    fs::create_dir_all(&subdir).expect("subdir created");
    fs::write(
        &file, "hello
",
    )
    .expect("file written");

    let files_only = render_directory_listing(
        &root,
        DirectoryListingOptions {
            line_limit: 20,
            start: 1,
            end: None,
            tail: None,
            head: None,
            matcher: None,
            tree: false,
            max_depth: None,
            sort: DirectorySort::Name,
            reverse: false,
            show_hidden: false,
            entry_filter: DirectoryEntryFilter::FilesOnly,
        },
    )
    .expect("files only listing");
    assert!(files_only.text.contains("[file] a.txt"));
    assert!(!files_only.text.contains("[dir] nested/"));

    let dirs_only = render_directory_listing(
        &root,
        DirectoryListingOptions {
            line_limit: 20,
            start: 1,
            end: None,
            tail: None,
            head: None,
            matcher: None,
            tree: false,
            max_depth: None,
            sort: DirectorySort::Name,
            reverse: false,
            show_hidden: false,
            entry_filter: DirectoryEntryFilter::DirsOnly,
        },
    )
    .expect("dirs only listing");
    assert!(dirs_only.text.contains("[dir] nested/"));
    assert!(!dirs_only.text.contains("[file] a.txt"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn tree_filter_preserves_parent_directory_context() {
    let root = unique_temp_dir("tools-view-tree-context");
    let nested = root.join("src/lib");
    let matched = nested.join("alpha.rs");
    let other = root.join("README.md");
    fs::create_dir_all(&nested).expect("nested created");
    fs::write(
        &matched,
        "fn main() {}
",
    )
    .expect("matched file written");
    fs::write(
        &other, "readme
",
    )
    .expect("other file written");

    let matcher = ViewMatcher::Substring {
        needle: "alpha".into(),
        ignore_case: false,
    };
    let tree = render_directory_listing(
        &root,
        DirectoryListingOptions {
            line_limit: 20,
            start: 1,
            end: None,
            tail: None,
            head: None,
            matcher: Some(&matcher),
            tree: true,
            max_depth: None,
            sort: DirectorySort::Name,
            reverse: false,
            show_hidden: false,
            entry_filter: DirectoryEntryFilter::Any,
        },
    )
    .expect("tree listing");

    assert!(tree.text.contains("[dir] src/"));
    assert!(tree.text.contains("[dir] src/lib/"));
    assert!(tree.text.contains("[file] src/lib/alpha.rs"));
    assert!(!tree.text.contains("README.md"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn directory_listing_supports_hidden_sort_and_max_depth() {
    let root = unique_temp_dir("tools-view-dir-options");
    let hidden = root.join(".secret");
    let visible = root.join("visible.txt");
    let nested_dir = root.join("nested");
    let nested_file = nested_dir.join("child.txt");
    fs::create_dir_all(&nested_dir).expect("nested dir created");
    fs::write(&hidden, "123456").expect("hidden file written");
    fs::write(&visible, "1").expect("visible file written");
    fs::write(&nested_file, "child").expect("nested file written");

    let no_hidden = render_directory_listing(
        &root,
        DirectoryListingOptions {
            line_limit: 20,
            start: 1,
            end: None,
            tail: None,
            head: None,
            matcher: None,
            tree: false,
            max_depth: None,
            sort: DirectorySort::Name,
            reverse: false,
            show_hidden: false,
            entry_filter: DirectoryEntryFilter::Any,
        },
    )
    .expect("listing without hidden");
    assert!(!no_hidden.text.contains(".secret"));

    let with_hidden = render_directory_listing(
        &root,
        DirectoryListingOptions {
            line_limit: 20,
            start: 1,
            end: None,
            tail: None,
            head: None,
            matcher: None,
            tree: false,
            max_depth: None,
            sort: DirectorySort::Size,
            reverse: false,
            show_hidden: true,
            entry_filter: DirectoryEntryFilter::Any,
        },
    )
    .expect("listing with hidden");
    assert!(with_hidden.text.contains(".secret"));
    let rendered_lines = with_hidden.text.lines().collect::<Vec<_>>();
    let hidden_index = rendered_lines
        .iter()
        .position(|line| line.contains(".secret"))
        .expect("hidden entry present");
    let visible_index = rendered_lines
        .iter()
        .position(|line| line.contains("visible.txt"))
        .expect("visible entry present");
    assert!(hidden_index < visible_index);

    let limited_tree = render_directory_listing(
        &root,
        DirectoryListingOptions {
            line_limit: 20,
            start: 1,
            end: None,
            tail: None,
            head: None,
            matcher: None,
            tree: true,
            max_depth: Some(0),
            sort: DirectorySort::Name,
            reverse: false,
            show_hidden: true,
            entry_filter: DirectoryEntryFilter::Any,
        },
    )
    .expect("limited tree");
    assert!(limited_tree.text.contains("[dir] nested/"));
    assert!(!limited_tree.text.contains("nested/child.txt"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn tree_files_only_omits_directory_context_lines() {
    let root = unique_temp_dir("tools-view-tree-files-only");
    let nested = root.join("src/lib");
    let matched = nested.join("alpha.rs");
    fs::create_dir_all(&nested).expect("nested created");
    fs::write(
        &matched,
        "fn main() {}
",
    )
    .expect("matched file written");

    let tree = render_directory_listing(
        &root,
        DirectoryListingOptions {
            line_limit: 20,
            start: 1,
            end: None,
            tail: None,
            head: None,
            matcher: None,
            tree: true,
            max_depth: None,
            sort: DirectorySort::Name,
            reverse: false,
            show_hidden: false,
            entry_filter: DirectoryEntryFilter::FilesOnly,
        },
    )
    .expect("tree listing");

    assert!(tree.text.contains("[file] src/lib/alpha.rs"));
    assert!(!tree.text.contains("[dir] src/"));
    assert!(!tree.text.contains("[dir] src/lib/"));

    let _ = fs::remove_dir_all(root);
}

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock works")
        .as_nanos();
    std::env::temp_dir().join(format!("lania-{name}-{unique}"))
}
