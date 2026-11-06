use super::*;
use crate::git::lfs_fixtures::{git, lfs_track, pushed_and_cloned};

#[test]
fn files_listed_as_json() -> Result<(), String> {
    let json = r#"{"files":[
        {"name":"sub dir/a b.bin","size":5000,"checkout":false,"downloaded":false},
        {"name":"ü.bin","size":7000,"checkout":true,"downloaded":true},
        {"name":"old\\x.bin","size":1,"checkout":true}]}"#;
    let files = parse_files_json(json)?;
    let short: Vec<(&str, u64, bool)> = files
        .iter()
        .map(|f| (f.path.as_str(), f.size, f.downloaded))
        .collect();
    assert_eq!(
        short,
        [
            ("sub dir/a b.bin", 5000, false),
            ("ü.bin", 7000, true),
            ("old/x.bin", 1, true)
        ]
    );
    assert!(parse_files_json("")?.is_empty());
    assert!(parse_files_json("{}")?.is_empty());
    assert!(parse_files_json("nope").is_err());
    Ok(())
}

#[test]
fn files_listed_as_text() {
    let text = "aa26 - sub dir/a b (1).bin (5.0 KB)\n4ef5 * ü.bin (300 B)\nbad\n";
    let files = parse_files_text(text);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "sub dir/a b (1).bin");
    assert_eq!(files[0].size, 5120);
    assert!(!files[0].downloaded);
    assert_eq!((files[1].size, files[1].downloaded), (300, true));
}

#[test]
fn human_sizes_read_back() {
    assert_eq!(parse_human_size("300 B"), Some(300));
    assert_eq!(parse_human_size("1.5 KB"), Some(1536));
    assert_eq!(parse_human_size("2 GB"), Some(2 << 30));
    assert_eq!(parse_human_size("lots"), None);
    assert_eq!(parse_human_size("3 parsecs"), None);
}

#[test]
fn the_media_dir_from_lfs_env() {
    let env = "git-lfs/3.4.1\n\nLocalWorkingDir=/r\nLocalMediaDir=/r/.git/lfs/objects\n";
    assert_eq!(media_dir(env).as_deref(), Some("/r/.git/lfs/objects"));
    assert_eq!(media_dir("LocalMediaDir=\n"), None);
    assert_eq!(parse_du("12\t3\n"), Some((12 * 1024, 3)));
    assert_eq!(parse_du("x"), None);
}

#[test]
fn prune_plans() {
    let out =
        "prune: 3 local objects, 2 retained, done.\n\n * c844 (300 B), done.\n * ab12 (1.0 KB)\n";
    assert_eq!(
        parse_prune(out),
        Some(PrunePlan {
            local: 3,
            retained: 2,
            objects: 2,
            bytes: 300 + 1024
        })
    );
    assert_eq!(
        parse_prune("prune: 0 local objects, 0 retained, done.\n"),
        Some(PrunePlan::default())
    );
    assert_eq!(parse_prune("something else"), None);
}

#[test]
fn folder_and_track_patterns() {
    assert_eq!(folder_pattern("./art/chars/").as_deref(), Some("art/chars"));
    assert_eq!(folder_pattern("art\\ü x").as_deref(), Some("art/ü x"));
    assert_eq!(folder_pattern("/"), None);
    assert_eq!(folder_pattern("a,b"), None);
    assert_eq!(split_list(" a, b ,,c"), ["a", "b", "c"]);
    assert_eq!(track_pattern("art/hero.PSD").as_deref(), Some("*.PSD"));
    assert_eq!(track_pattern("Makefile").as_deref(), Some("Makefile"));
    assert_eq!(
        track_pattern("dir/my file").as_deref(),
        Some("dir/my[[:space:]]file")
    );
    assert_eq!(track_pattern(".env").as_deref(), Some(".env"));
    assert_eq!(track_pattern(""), None);
}

/// The checkout's LFS files and whether they were downloaded, the storage's size, a file
/// downloaded on its own, fetch filters and pruning, through git-lfs.
#[test]
fn storage_through_git_lfs() -> Result<(), String> {
    let Some((work, clone)) = pushed_and_cloned("storage")? else {
        return Ok(());
    };
    let files = lfs_files(&clone)?;
    let listed: Vec<(&str, bool)> = files
        .iter()
        .map(|f| (f.path.as_str(), f.downloaded))
        .collect();
    assert_eq!(listed, [("art dir/a b.bin", false), ("ü.bin", false)]);
    assert_eq!(lfs_usage(&clone)?.objects, 0);

    // One file downloaded on its own.
    git(
        &clone,
        &["lfs", "pull", "--include", "ü.bin", "--exclude", ""],
    )?;
    let files = lfs_files(&clone)?;
    assert!(files.iter().any(|f| f.path == "ü.bin" && f.downloaded));
    assert!(
        files
            .iter()
            .any(|f| f.path == "art dir/a b.bin" && !f.downloaded)
    );
    let usage = lfs_usage(&clone)?;
    assert_eq!((usage.objects, usage.bytes), (1, 700));

    // Where to fetch from: set, read back, taken out.
    set_fetch_filter(&clone, false, &["art dir".to_owned(), "levels".to_owned()])?;
    set_fetch_filter(&clone, true, &["art dir/old".to_owned()])?;
    assert_eq!(
        fetch_filters(&clone),
        (
            vec!["art dir".to_owned(), "levels".to_owned()],
            vec!["art dir/old".to_owned()]
        )
    );
    assert!(set_fetch_filter(&clone, false, &["a,b".to_owned()]).is_err());
    set_fetch_filter(&clone, false, &[])?;
    set_fetch_filter(&clone, false, &[])?;
    assert!(fetch_filters(&clone).0.is_empty());

    // The source repository has an old version of ü.bin, pushed: prune may let it go.
    let plan = lfs_prune_plan(&work)?;
    assert_eq!((plan.local, plan.objects), (3, 1), "{plan:?}");
    assert_eq!(plan.bytes, 300);
    git(&work, &["lfs", "prune"])?;
    assert_eq!(lfs_usage(&work)?.objects, 2);
    if let Some(base) = work.parent() {
        let _ = std::fs::remove_dir_all(base);
    }
    Ok(())
}

#[test]
fn an_empty_track_pattern_is_refused() {
    assert!(lfs_track(&std::env::temp_dir(), " ").is_err());
}
