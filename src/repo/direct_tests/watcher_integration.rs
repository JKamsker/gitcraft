use super::*;

#[test]
fn a_linked_worktree_is_watched_where_git_keeps_its_data() {
    let dir = repo("linked-watch");
    let linked = dir.with_file_name(format!("gitcraft-linked-watch-wt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&linked);
    let added = git::run(
        Some(&dir),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            &linked.to_string_lossy(),
        ],
    );
    let dirs = linked_git_dirs(&linked);
    let main = linked_git_dirs(&dir);
    let canonical = std::fs::canonicalize(dir.join(".git")).ok();
    let _ = std::fs::remove_dir_all(&linked);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(added.is_ok());
    assert!(main.is_none());
    assert!(dirs.is_some());
    let (own, common) = dirs.unwrap_or_default();
    assert_eq!(Some(common.clone()), canonical);
    assert!(own.starts_with(&common) && own != common);
    let watched = [own.clone(), common.clone()];
    // Its HEAD, the shared refs: reload; objects: nothing; its index: status and submodules;
    // its files: status.
    assert_eq!(fs_change(&own.join("HEAD"), &watched), Some(FsChange::Git));
    assert_eq!(
        fs_change(&own.join("index"), &watched),
        Some(FsChange::Index)
    );
    assert_eq!(
        fs_change(&common.join("refs/heads/side"), &watched),
        Some(FsChange::Git)
    );
    assert_eq!(fs_change(&common.join("objects/ab/cd"), &watched), None);
    assert_eq!(
        fs_change(&linked.join("src/a.rs"), &watched),
        Some(FsChange::Files)
    );
    // A main worktree as before.
    assert_eq!(
        fs_change(Path::new("/r/.git/index"), &[]),
        Some(FsChange::Index)
    );
    assert_eq!(
        fs_change(Path::new("/r/.git/MERGE_HEAD"), &[]),
        Some(FsChange::Git)
    );
    assert_eq!(fs_change(Path::new("/r/.git/FETCH_HEAD"), &[]), None);
    assert_eq!(fs_change(Path::new("/r/.git/ORIG_HEAD"), &[]), None);
    assert_eq!(fs_change(Path::new("/r/.git/objects/x"), &[]), None);
    assert_eq!(fs_change(Path::new("/r/a.rs"), &[]), Some(FsChange::Files));
    assert_eq!(fs_change(Path::new("/r/.git"), &[]), None);
    assert_eq!(
        fs_change(Path::new("/r/.github"), &[]),
        Some(FsChange::Files)
    );
    assert_eq!(
        fs_change(Path::new("/r/.gitmodules"), &[]),
        Some(FsChange::Index)
    );
    // A repository made with `--ref-format=reftable`.
    assert_eq!(
        fs_change(Path::new("/r/.git/reftable/tables.list"), &[]),
        Some(FsChange::Git)
    );
}

#[test]
fn a_change_on_disk_reads_again_what_it_can_have_changed() {
    // The index holds the commits submodules should be at: they are read again with the status.
    assert_eq!(
        FsChange::Index.rereads(),
        [Reread::Status, Reread::Submodules]
    );
    assert_eq!(FsChange::Files.rereads(), [Reread::Status]);
    assert_eq!(FsChange::Git.rereads(), [Reread::All]);
}
