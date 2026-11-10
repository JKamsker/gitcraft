//! Migration.

use super::*;

#[test]
pub(super) fn migration_reads_renames_copies_scores_and_immutable_blob_sides() -> Result<(), String>
{
    let r = Repo::new()?;
    r.write(
        "old/rename.txt",
        (0..40).map(|i| format!("line {i}\n")).collect::<String>(),
    )?;
    r.write("old/copy.txt", "copied unchanged content\n")?;
    let before = r.commit()?;
    r.git(&["mv", "old/rename.txt", "new-rename.txt"])?;
    r.write(
        "new-rename.txt",
        (0..40)
            .map(|i| format!("line {}\n", if i == 10 { 99 } else { i }))
            .collect::<String>(),
    )?;
    r.write("new-copy.txt", "copied unchanged content\n")?;
    r.git(&["add", "-A"])?;
    let m = read_migration(&r.0, &MigrationSource::Staged, &|| true)?;
    assert!(m.moves.iter().any(|m| !m.copy
        && m.before.path == b"old/rename.txt"
        && m.after.path == b"new-rename.txt"
        && m.similarity.is_some_and(|v| v > 50 && v < 100)));
    let copied = m
        .moves
        .iter()
        .find(|m| m.copy && m.after.path == b"new-copy.txt")
        .ok_or("copy")?;
    assert_eq!(copied.similarity, Some(100));
    let entry = copied.after.clone();
    let after = r.commit()?;
    r.write("new-copy.txt", "later\n")?;
    r.commit()?;
    assert_eq!(
        blob(&r.0, Some(&entry), &|| true)?.bytes,
        b"copied unchanged content\n"
    );
    assert_eq!(
        read_migration(&r.0, &MigrationSource::Range(before, after), &|| true)?
            .moves
            .len(),
        m.moves.len()
    );
    Ok(())
}

#[test]
pub(super) fn unborn_additions_deletions_and_empty_reorganizations_are_explicit()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("new.txt", "one\ntwo\n")?;
    let lens = r.lens("new.txt")?;
    assert!(lens.head.is_none());
    assert!(lens.versions[0].mode.is_none());
    assert!(lens.versions[1].mode.is_none());
    assert_eq!(lens.versions[2].bytes, b"one\ntwo\n");
    r.git(&["add", "new.txt"])?;
    let m = read_migration(&r.0, &MigrationSource::Staged, &|| true)?;
    assert!(m.moves.is_empty());
    assert_eq!(m.other_changes, 1);
    r.commit()?;
    std::fs::remove_file(r.0.join("new.txt")).map_err(|e| e.to_string())?;
    assert!(r.lens("new.txt")?.versions[2].mode.is_none());
    Ok(())
}

#[test]
pub(super) fn matrix_distinguishes_equal_blobs_missing_modes_and_pins_ambiguous_branch_refs()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("folder/config", "same\n")?;
    r.write("folder/gone", "gone\n")?;
    let a = r.commit()?;
    r.git(&["branch", "release-a"])?;
    r.git(&["tag", "release-a"])?;
    r.git(&["rm", "folder/gone"])?;
    r.git(&["update-index", "--chmod=+x", "folder/config"])?;
    r.git(&["commit", "-qm", "mode"])?;
    r.git(&["branch", "release-b"])?;
    let m = matrix(
        &r.0,
        &["refs/heads/release-a".into(), "refs/heads/release-b".into()],
        "folder",
        &|| true,
    )?;
    let x = m.columns[0]
        .files
        .get(b"folder/config".as_slice())
        .ok_or("config")?;
    let y = m.columns[1]
        .files
        .get(b"folder/config".as_slice())
        .ok_or("config")?;
    assert_eq!(x.oid, y.oid);
    assert_eq!((&*x.mode, &*y.mode), ("100644", "100755"));
    assert!(!m.columns[1].files.contains_key(b"folder/gone".as_slice()));
    r.git(&["branch", "-f", "release-a", "release-b"])?;
    assert_eq!(m.columns[0].oid, a);
    assert_eq!(blob(&r.0, Some(x), &|| true)?.bytes, b"same\n");
    Ok(())
}
