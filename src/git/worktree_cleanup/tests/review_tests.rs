//! Regression coverage for review findings #225.
use super::{Fixture, write};
use crate::git::worktree_cleanup::{Outcome, inspect, remove};

#[test]
fn archive_store_inside_ignored_folder_blocks_even_without_untracked_files() -> Result<(), String> {
    let f = Fixture::new("224 archive inside")?;
    let wt = f.merged_worktree("done")?;
    f.repo.git(&[
        "config",
        "core.excludesFile",
        &f.scratch.join("ignore").to_string_lossy(),
    ])?;
    write(&f.scratch, "ignore", b"cache/\n")?;
    let store = super::Store {
        root: wt.join("cache/archives"),
        quota: 1 << 20,
    };
    write(&store.root, "archive.zip", b"precious")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert!(preview.untracked.is_empty());
    for archive in [false, true] {
        assert!(matches!(
            remove(f.repo.path(), &target, &preview, Some(&store), archive),
            Outcome::Failed(_)
        ));
        assert_eq!(
            std::fs::read(store.root.join("archive.zip")).unwrap(),
            b"precious"
        );
    }
    Ok(())
}

#[test]
fn ignored_file_created_after_preview_is_preserved() -> Result<(), String> {
    let f = Fixture::new("224 ignored changed")?;
    let wt = f.merged_worktree("done")?;
    f.repo.git(&[
        "config",
        "core.excludesFile",
        &f.scratch.join("ignore").to_string_lossy(),
    ])?;
    write(&f.scratch, "ignore", b".env\ncache/\n")?;
    write(&wt, "cache/old", b"a")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    write(&wt, ".env", b"secret")?;
    assert_eq!(
        remove(f.repo.path(), &target, &preview, Some(&f.store), false),
        Outcome::Changed
    );
    assert_eq!(std::fs::read(wt.join(".env")).unwrap(), b"secret");
    Ok(())
}

#[test]
fn quota_counts_only_archive_files_in_store_folders() -> Result<(), String> {
    let f = Fixture::new("224 quota")?;
    for (name, bytes) in [
        ("abcd/one.zip", b"zip".as_slice()),
        ("abcd/one.json", b"json"),
        ("abcd/other.txt", b"ignored"),
        ("unrelated/no.zip", b"ignored"),
        ("root.zip", b"ignored"),
        ("abcd/nested/no.json", b"ignored"),
    ] {
        write(&f.store.root, name, bytes)?;
    }
    assert_eq!(f.store.usage(), 7);
    Ok(())
}

#[test]
fn ntfs_aliases_are_never_archive_paths() {
    for path in [
        ".git./config",
        ".git /config",
        "GIT~1/config",
        "git~2/config",
        "a./x",
        "a /x",
    ] {
        assert!(super::super::files::checked_rel(path).is_none(), "{path}");
    }
    assert!(super::super::files::checked_rel("legit/config").is_some());
}
