//! Path safety.

use super::*;

#[cfg(unix)]
#[test]
pub(super) fn changed_symlinks_and_conflicted_indexes_fail_without_following_targets_or_touching_index()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    let outside = f.0.join(".git/outside");
    std::fs::write(&outside, b"outside must stay intact").map_err(|e| e.to_string())?;
    std::fs::remove_file(f.0.join("file")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&outside, f.0.join("file")).map_err(|e| e.to_string())?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    assert!(capture(&f.0, "Pause", &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(&outside).map_err(|e| e.to_string())?,
        b"outside must stay intact"
    );
    assert!(list(&f.0, &|| true)?.is_empty());
    std::fs::remove_file(f.0.join("file")).map_err(|e| e.to_string())?;
    f.write("file", b"base\n")?;
    let oid = git::run(Some(&f.0), &["rev-parse", "HEAD:file"])?;
    let oid = oid.trim();
    let input = format!(
        "0 {}\tfile\n100644 {oid} 1\tfile\n100644 {oid} 2\tfile\n100644 {oid} 3\tfile\n",
        "0".repeat(40)
    );
    git::draft_bytes(
        &f.0,
        &["update-index", "--index-info"],
        &[],
        input.into_bytes(),
        &[0],
        &|| true,
    )?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    assert!(capture(&f.0, "Conflict", &|| true).is_err_and(|e| e.contains("conflicts")));
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert!(list(&f.0, &|| true)?.is_empty());
    Ok(())
}

#[test]
pub(super) fn limits_invalid_paths_and_cancellation_leave_all_real_state_untouched()
-> Result<(), String> {
    for path in [
        "../escape",
        "a/../b",
        ".git/config",
        "a/.Git/x",
        "C:/file",
        "a\\file",
        "line\nbreak",
        "a//b",
        "/absolute",
    ] {
        assert!(valid_path(path).is_err(), "{path}");
    }
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    std::fs::File::create(f.0.join("large"))
        .and_then(|f| f.set_len(FILE_LIMIT as u64 + 1))
        .map_err(|e| e.to_string())?;
    assert!(capture(&f.0, "Large", &|| true).is_err());
    std::fs::remove_file(f.0.join("large")).map_err(|e| e.to_string())?;
    f.write("file", b"cancelled working data\n")?;
    let raw = git::draft_bytes(
        &f.0,
        &["hash-object", "--no-filters", "--stdin"],
        &[],
        b"cancelled working data\n".to_vec(),
        &[0],
        &|| true,
    )?;
    let hash = String::from_utf8(raw).map_err(|e| e.to_string())?;
    let hash = hash.trim();
    let object = f.0.join(".git/objects").join(&hash[..2]).join(&hash[2..]);
    assert!(!object.exists());
    assert!(capture(&f.0, "Cancelled", &|| !object.exists()).is_err());
    assert!(list(&f.0, &|| true)?.is_empty());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"cancelled working data\n"
    );
    Ok(())
}

#[test]
pub(super) fn private_index_cache_handles_split_and_sparse_indexes_without_rewriting_the_real_index()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("keep/file", b"base\n")?;
    f.write("omit/file", b"outside sparse checkout\n")?;
    f.commit()?;
    git::run(Some(&f.0), &["update-index", "--split-index"])?;
    git::run(
        Some(&f.0),
        &["update-index", "--assume-unchanged", "keep/file"],
    )?;
    f.write("keep/file", b"hidden working edit\n")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let saved = capture(&f.0, "Split index", &|| true)?.ok_or("No split-index draft")?;
    assert_eq!(
        git::run_bytes(
            Some(&f.0),
            &[
                "cat-file",
                "blob",
                &format!("{}:keep/file", saved.metadata.work_tree)
            ]
        )?,
        b"hidden working edit\n"
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    git::run(
        Some(&f.0),
        &["update-index", "--no-assume-unchanged", "keep/file"],
    )?;
    git::run(Some(&f.0), &["restore", "keep/file"])?;
    git::run(Some(&f.0), &["update-index", "--no-split-index"])?;
    git::run(
        Some(&f.0),
        &["sparse-checkout", "set", "--cone", "--sparse-index", "keep"],
    )?;
    f.write("keep/file", b"sparse edit\n")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let saved = capture(&f.0, "Sparse index", &|| true)?.ok_or("No sparse-index draft")?;
    assert_eq!(
        git::run_bytes(
            Some(&f.0),
            &[
                "cat-file",
                "blob",
                &format!("{}:keep/file", saved.metadata.work_tree)
            ]
        )?,
        b"sparse edit\n"
    );
    assert!(
        !saved
            .metadata
            .files
            .iter()
            .any(|file| file.path == "omit/file")
    );
    assert!(!f.0.join("omit/file").exists());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[test]
pub(super) fn recovery_detects_intent_to_add_changes_even_when_index_object_records_match()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("empty", b"")?;
    git::run(Some(&f.0), &["add", "-N", "empty"])?;
    f.write("file", b"saved\n")?;
    let saved = capture(&f.0, "Intent to add", &|| true)?.ok_or("No draft")?;
    f.write("file", b"newer\n")?;
    let view = preview(&f.0, &saved.id, "file", false, &|| true)?;
    let records = git::run(Some(&f.0), &["ls-files", "-v", "--stage", "-z"])?;
    git::run(Some(&f.0), &["add", "empty"])?;
    assert_eq!(
        git::run(Some(&f.0), &["ls-files", "-v", "--stage", "-z"])?,
        records
    );
    assert!(restore(&f.0, &view, &[0], &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"newer\n"
    );
    Ok(())
}

#[test]
pub(super) fn unchanged_large_tracked_files_are_not_copied_as_working_changes() -> Result<(), String>
{
    let f = Fixture::new()?;
    std::fs::File::create(f.0.join("large"))
        .and_then(|file| file.set_len(FILE_LIMIT as u64 + 1))
        .map_err(|e| e.to_string())?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("file", b"small working edit\n")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let saved = capture(&f.0, "Small change", &|| true)?.ok_or("No draft")?;
    assert_eq!(saved.metadata.files.len(), 1);
    assert_eq!(saved.metadata.files[0].path, "file");
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[cfg(unix)]
#[test]
pub(super) fn typed_remote_reader_quotes_paths_and_never_confuses_links_with_missing_files()
-> Result<(), String> {
    use std::process::Command;
    let f = Fixture::new()?;
    let root = f.0.join("space ' quote $(touch trap) ü");
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    std::fs::write(root.join("literal ' $x ü"), b"bytes\0raw").map_err(|e| e.to_string())?;
    let read = |rel: &str, limit| -> Result<std::process::Output, String> {
        Command::new("sh")
            .arg("-c")
            .arg(read_script(root.to_str().ok_or("utf8")?, rel, limit)?)
            .output()
            .map_err(|e| e.to_string())
    };
    let out = read("literal ' $x ü", 20)?;
    assert!(out.status.success(), "{:?}", out.stderr);
    assert_eq!(out.stdout, b"F100644\nbytes\0raw");
    assert_eq!(read("absent/folder/file", 20)?.stdout, b"M\n");
    assert!(!read("literal ' $x ü", 2)?.stdout.ends_with(b"bytes\0raw"));
    std::os::unix::fs::symlink("absent", root.join("link")).map_err(|e| e.to_string())?;
    assert!(!read("link/file", 20)?.status.success());
    assert!(!read("link", 20)?.status.success());
    assert!(missing_inside(&root, "link/file").is_err());
    assert!(missing_inside(&root, "absent/file").is_ok());
    assert!(!f.0.join("trap").exists());
    Ok(())
}

#[test]
pub(super) fn nested_untracked_repositories_are_excluded_without_hiding_outer_changes()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("nested/private", b"separate repository content")?;
    git::run(Some(&f.0.join("nested")), &["init", "-q"])?;
    f.write("outer", b"saved outer bytes")?;
    let saved = capture(&f.0, "Pause", &|| true)?.ok_or("No draft")?;
    assert!(saved.metadata.files.iter().any(|file| file.path == "outer"));
    assert!(
        !saved
            .metadata
            .files
            .iter()
            .any(|file| file.path.starts_with("nested"))
    );
    Ok(())
}

#[cfg(unix)]
#[test]
pub(super) fn missing_paths_below_symlinks_fail_closed_and_do_not_publish_deletions()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("directory/file", b"tracked bytes")?;
    f.commit()?;
    std::fs::remove_dir_all(f.0.join("directory")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("absent", f.0.join("directory")).map_err(|e| e.to_string())?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    assert!(capture(&f.0, "Pause", &|| true).is_err());
    assert!(git::run(Some(&f.0), &["for-each-ref", PREFIX])?.is_empty());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[test]
pub(super) fn unborn_sha256_and_linked_worktree_versions_keep_separate_identities()
-> Result<(), String> {
    let f = Fixture::new_with_format("sha256")?;
    f.write("unborn ü", b"not committed yet")?;
    let unborn = capture(&f.0, "Pause", &|| true)?.ok_or("No unborn version")?;
    assert!(unborn.metadata.head.is_none());
    assert_eq!(unborn.id.tree.len(), 64);
    f.write("unborn ü", b"newer unborn bytes")?;
    let view = preview(&f.0, &unborn.id, "unborn ü", false, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("unborn ü")).map_err(|e| e.to_string())?,
        b"not committed yet"
    );
    f.commit()?;
    let linked = f.0.join(".git/linked tree ü");
    git::run(
        Some(&f.0),
        &[
            "worktree",
            "add",
            "-b",
            "linked",
            linked.to_str().ok_or("utf8")?,
        ],
    )?;
    std::fs::write(linked.join("unborn ü"), b"linked version").map_err(|e| e.to_string())?;
    let saved = capture(&linked, "Pause", &|| true)?.ok_or("No linked version")?;
    assert_ne!(unborn.metadata.worktree, saved.metadata.worktree);
    assert_eq!(list(&linked, &|| true)?.len(), 1);
    assert!(preview(&f.0, &saved.id, "unborn ü", false, &|| true).is_err());
    assert!(
        list(&f.0, &|| true)?
            .iter()
            .all(|draft| draft.metadata.worktree == unborn.metadata.worktree)
    );
    Ok(())
}

#[test]
pub(super) fn rename_unicode_no_newline_and_hardlinks_recover_only_the_selected_path()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("old", b"base without newline")?;
    f.commit()?;
    git::run(Some(&f.0), &["mv", "old", "renamed ü ' file"])?;
    f.write(
        "renamed ü ' file",
        b"saved \xf0\x9f\x8c\xb1 without newline",
    )?;
    let saved = capture(&f.0, "Renamed", &|| true)?.ok_or("No renamed version")?;
    f.write("renamed ü ' file", b"newer no newline")?;
    let linked = f.0.join(".git/outside-hardlink");
    std::fs::hard_link(f.0.join("renamed ü ' file"), &linked).map_err(|e| e.to_string())?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let view = preview(&f.0, &saved.id, "renamed ü ' file", false, &|| true)?;
    assert_eq!(view.hunks.len(), 1);
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("renamed ü ' file")).map_err(|e| e.to_string())?,
        b"saved \xf0\x9f\x8c\xb1 without newline"
    );
    assert_eq!(
        std::fs::read(linked).map_err(|e| e.to_string())?,
        b"newer no newline"
    );
    assert!(!f.0.join("old").exists());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[cfg(unix)]
#[test]
pub(super) fn ignored_rule_read_warnings_fail_closed_before_any_version_is_published()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base")?;
    f.commit()?;
    f.write(".git/ignore-rules", b"*.secret\n")?;
    std::os::unix::fs::symlink(".git/ignore-rules", f.0.join(".gitignore"))
        .map_err(|e| e.to_string())?;
    f.write("private.secret", b"must not copy")?;
    let ctx = Context::new(&f.0, &|| true)?;
    let read = ctx.source(
        &["ls-files", "--others", "--exclude-standard", "-z"],
        vec![],
        &[0],
        &|| true,
    );
    assert!(read.is_err(), "Uncertain ignore rules must be refused");
    assert!(capture(&f.0, "Pause", &|| true).is_err());
    assert!(git::run(Some(&f.0), &["for-each-ref", PREFIX])?.is_empty());
    Ok(())
}
