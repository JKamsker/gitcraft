use super::*;

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn markdown_files_are_known_by_their_extension() {
    assert!(is_markdown("README.md"));
    assert!(is_markdown("docs/Guide.MarkDown"));
    assert!(!is_markdown("src/main.rs"));
    assert!(!is_markdown("md"));
}

#[test]
fn blocks_are_the_top_level_ones_and_each_item_of_a_list() {
    let text = "# Title\n\nSome text\non two lines.\n\n- one\n- two\n\n---\n\n```rust\nfn x() {}\n```\n\n> quoted\n> - not split\n";
    let (blocks, defs) = split_blocks(text);
    assert_eq!(
        blocks,
        strings(&[
            "# Title",
            "Some text\non two lines.",
            "- one",
            "- two",
            "---",
            "```rust\nfn x() {}\n```",
            "> quoted\n> - not split",
        ])
    );
    assert!(defs.is_empty());
}

#[test]
fn link_definitions_go_along_with_the_blocks() {
    let (blocks, defs) = split_blocks("See [the docs][d].\n\n[d]: https://example.org\n");
    assert_eq!(blocks, strings(&["See [the docs][d]."]));
    assert_eq!(defs, "[d]: https://example.org");
}

#[test]
fn the_changed_blocks_are_removed_then_added_between_those_kept() {
    let old = strings(&["a", "b", "c", "d"]);
    let new = strings(&["a", "B", "c", "d", "e"]);
    assert_eq!(
        diff_blocks(&old, &new),
        vec![
            (Change::Kept, 0),
            (Change::Removed, 1),
            (Change::Added, 1),
            (Change::Kept, 2),
            (Change::Kept, 3),
            (Change::Added, 4),
        ]
    );
    // A new file, a deleted one.
    assert_eq!(diff_blocks(&[], &strings(&["x"])), vec![(Change::Added, 0)]);
    assert_eq!(
        diff_blocks(&strings(&["x"]), &[]),
        vec![(Change::Removed, 0)]
    );
    // A block moved: kept where the longest run of the same ones is.
    assert_eq!(
        diff_blocks(&strings(&["a", "b", "c"]), &strings(&["b", "c", "a"])),
        vec![
            (Change::Removed, 0),
            (Change::Kept, 0),
            (Change::Kept, 1),
            (Change::Added, 2),
        ]
    );
}

#[test]
fn images_are_read_from_the_repository_only() {
    assert_eq!(
        repo_path("docs/guide.md", "shot.png").as_deref(),
        Some("docs/shot.png")
    );
    assert_eq!(
        repo_path("docs/guide.md", "./img/a%20b.png?raw=1#x").as_deref(),
        Some("docs/img/a b.png")
    );
    assert_eq!(
        repo_path("docs/guide.md", "../assets/logo.svg").as_deref(),
        Some("assets/logo.svg")
    );
    assert_eq!(
        repo_path("docs/guide.md", "/assets/logo.svg").as_deref(),
        Some("assets/logo.svg")
    );
    for elsewhere in [
        "https://example.org/x.png",
        "//example.org/x.png",
        "../../outside.png",
        "file:///etc/passwd",
        "C:/x.png",
        "\\\\server\\share\\x.png",
        "%5C%5Cserver%5Cshare%5Cx.png",
        "data:image/png;base64,AAAA",
        "",
    ] {
        assert_eq!(repo_path("docs/guide.md", elsewhere), None, "{elsewhere}");
    }
}

#[test]
fn a_repository_image_stays_and_any_other_becomes_a_link() {
    let text = "![logo](logo.png) ![pixel](https://evil.example/p.png)\n\n[![badge](badge.svg)](https://ci.example)";
    let (safe, kept) = pr::safe_local_markdown(text, &mut |url| {
        (!url.contains("://")).then(|| format!("bytes://markdown/1/{url}"))
    });
    assert_eq!(
        safe,
        "![logo](bytes://markdown/1/logo.png) [pixel](https://evil.example/p.png)\n\n\
             [![badge](bytes://markdown/1/badge.svg)](https://ci.example)"
    );
    assert_eq!(kept.len(), 2);
}

/// A folder of the working tree that is a link reads nothing through it; a file in a plain
/// folder is read.
#[cfg(unix)]
#[test]
fn working_tree_images_never_go_through_a_linked_folder() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-md-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("docs")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("docs/s.png"), "picture").map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("/", dir.join("docs/l")).map_err(|e| e.to_string())?;
    let plain = working_tree_image(&dir, "docs/s.png").is_some();
    let linked = working_tree_image(&dir, "docs/l/etc/hostname").is_some();
    let missing = working_tree_image(&dir, "docs/none.png").is_some();
    // And as the rendered file's images are read: nothing through the link.
    let mut images = Images::default();
    let side = Side {
        rev: None,
        path: "docs/a.md".to_owned(),
    };
    let through_link = images.local(&dir, &side, "l/etc/hostname");
    let read_after_link = images.found.len();
    let beside = images.local(&dir, &side, "s.png");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(plain);
    assert!(!linked);
    assert!(!missing);
    assert_eq!((through_link, read_after_link), (None, 0));
    assert!(beside.is_some());
    assert_eq!(images.found.len(), 1);
    Ok(())
}

#[test]
fn a_file_reads_with_its_pictures_on_each_side() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-rendered-md-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("docs")).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(args)?;
    }
    let write =
        |path: &str, text: &str| std::fs::write(dir.join(path), text).map_err(|e| e.to_string());
    write("docs/a.md", "# A\n\nOld text.\n\n![s](s.png)\n")?;
    write("docs/s.png", "old picture")?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "one"])?;
    write("docs/a.md", "# A\n\nNew text.\n\n![s](s.png)\n")?;
    write("docs/s.png", "new picture")?;
    git(&["commit", "-qam", "two"])?;
    let key = DiffKey::Commit {
        hash: git(&["rev-parse", "HEAD"])?.trim().to_owned(),
        path: "docs/a.md".to_owned(),
    };
    let md = load(&dir, &key);
    let _ = std::fs::remove_dir_all(&dir);
    let md = md?;
    let changes: Vec<Change> = md.blocks.iter().map(|(c, _)| *c).collect();
    // The picture changed too: its block shows it before and after.
    assert_eq!(
        changes,
        [
            Change::Kept,
            Change::Removed,
            Change::Removed,
            Change::Added,
            Change::Added
        ]
    );
    assert!(md.file.contains("New text."));
    assert!(!md.before);
    assert_eq!(md.images.len(), 2);
    assert!(md.images.iter().all(|(uri, _)| uri.ends_with("/s.png")));
    Ok(())
}
