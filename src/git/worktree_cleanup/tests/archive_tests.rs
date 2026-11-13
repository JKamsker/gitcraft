//! Archives: what they hold, verified, listed per repository, accounted, deleted; refused
//! when a file changed after it was looked at.

use std::io::Read;
use std::path::Path;

use super::{Fixture, UNICODE, write};
use crate::git::worktree_cleanup::archive::{self, Listed, Source};
use crate::git::worktree_cleanup::inspect;

/// An archive of the untracked files of the worktree `wt` (of `branch`), as cleanup makes it.
pub(super) fn archived(f: &Fixture, wt: &Path, branch: &str) -> Result<Listed, String> {
    let found = inspect(f.repo.path(), &f.target(wt, branch)?)?;
    let repository = archive::repository(wt)?;
    archive::create(
        &f.store,
        &Source {
            repository: &repository,
            worktree: wt,
            branch,
            head: &found.head,
            files: &found.untracked,
        },
    )
}

#[test]
fn an_archive_holds_the_files_and_a_manifest_and_is_listed() -> Result<(), String> {
    let f = Fixture::new("archive")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, UNICODE, "grüße\n".as_bytes())?;
    write(&wt, "plain.txt", b"plain\n")?;
    let listed = archived(&f, &wt, "done")?;
    let manifest = &listed.manifest;
    assert_eq!(manifest.branch, "done");
    assert_eq!(manifest.head, f.repo.tip("done")?);
    assert_eq!(Path::new(&manifest.worktree), wt.as_path());
    let mut paths: Vec<&str> = manifest.files.iter().map(|e| e.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, [UNICODE, "plain.txt"]);
    assert!(
        manifest
            .files
            .iter()
            .all(|e| e.sha256.len() == 64 && e.size > 0)
    );
    // Outside every worktree, in the store's folder of this repository.
    assert!(listed.zip.starts_with(&f.store.root));
    assert!(!listed.zip.starts_with(&wt));
    let mut zip =
        zip::ZipArchive::new(std::fs::File::open(&listed.zip).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut text = String::new();
    zip.by_name(&format!("files/{UNICODE}"))
        .map_err(|e| e.to_string())?
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    assert_eq!(text, "grüße\n");
    assert!(zip.by_name("manifest.json").is_ok());
    // Listed for this repository, with its size counted in the store's usage.
    let key = manifest.repository.key.clone();
    assert_eq!(f.store.list(Some(&key)), [listed.clone()]);
    assert_eq!(f.store.usage(), listed.bytes);
    f.store.delete(&listed)?;
    assert!(f.store.list(None).is_empty());
    assert_eq!(f.store.usage(), 0);
    Ok(())
}

#[test]
fn a_file_changed_after_the_look_is_not_archived() -> Result<(), String> {
    let f = Fixture::new("archive changed")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, "a.txt", b"before\n")?;
    let found = inspect(f.repo.path(), &f.target(&wt, "done")?)?;
    write(&wt, "a.txt", b"after!!\n")?;
    let repository = archive::repository(&wt)?;
    let made = archive::create(
        &f.store,
        &Source {
            repository: &repository,
            worktree: &wt,
            branch: "done",
            head: &found.head,
            files: &found.untracked,
        },
    );
    assert!(made.is_err());
    assert!(f.store.list(None).is_empty());
    assert_eq!(f.store.usage(), 0);
    Ok(())
}

#[test]
fn archives_of_another_repository_are_neither_listed_nor_restored() -> Result<(), String> {
    let f = Fixture::new("archive isolation")?;
    let other = Fixture::new("archive isolation other")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, "a.txt", b"mine\n")?;
    let listed = archived(&f, &wt, "done")?;
    let theirs = archive::repository(other.repo.path())?;
    assert_ne!(theirs.key, listed.manifest.repository.key);
    assert!(f.store.list(Some(&theirs.key)).is_empty());
    let restored = f.store.restore(&listed, other.repo.path());
    assert!(restored.is_err());
    assert!(!other.repo.path().join("a.txt").exists());
    Ok(())
}

#[test]
fn a_tampered_archive_or_an_id_outside_the_store_is_refused() -> Result<(), String> {
    let f = Fixture::new("archive tampered")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, "a.txt", &b"original text\n".repeat(50))?;
    let listed = archived(&f, &wt, "done")?;
    let mut evil = listed.manifest.clone();
    evil.id = "../../x".to_owned();
    assert!(f.store.delete_manifest(&evil).is_err());
    evil.repository.key = "../..".to_owned();
    assert!(f.store.delete_manifest(&evil).is_err());
    // Bytes changed in the ZIP: it no longer verifies, and nothing is restored from it.
    let mut bytes = std::fs::read(&listed.zip).map_err(|e| e.to_string())?;
    let middle = bytes.len() / 3;
    bytes[middle] ^= 0xff;
    std::fs::write(&listed.zip, bytes).map_err(|e| e.to_string())?;
    std::fs::remove_file(wt.join("a.txt")).map_err(|e| e.to_string())?;
    assert!(f.store.restore(&listed, &wt).is_err());
    assert!(!wt.join("a.txt").exists());
    Ok(())
}
