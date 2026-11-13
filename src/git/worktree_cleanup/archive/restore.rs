//! Putting an archive's files back into a worktree of the same repository: only new files are
//! made (a file already there with other content keeps it; the archived one goes beside it as
//! `name.restored`), no folder is reached through a link, nothing lands outside the worktree,
//! and the archive stays, so a restore loses nothing and can be done again.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::write::{read_entry, verify};
use super::{Listed, Manifest, Restored, Store, repository};
use crate::git::worktree_cleanup::files::{checked_rel, sha256_hex};

/// What a restore did, file by file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RestoreReport {
    pub created: Vec<String>,
    /// Already there with the same content.
    pub same: Vec<String>,
    /// Already there with other content: (its path, the name the archived one got).
    pub renamed: Vec<(String, String)>,
    /// Not restored, and why.
    pub failed: Vec<(String, String)>,
    /// The manifest with this restore recorded.
    pub(in crate::git) manifest: Option<Manifest>,
}

/// Restores `archive` into the worktree `dest`, which must be a worktree (its top folder) of
/// the repository the archive came from.
pub(super) fn restore(
    store: &Store,
    archive: &Listed,
    dest: &Path,
) -> Result<RestoreReport, String> {
    let manifest = &archive.manifest;
    let here = repository(dest)?;
    if here.key != manifest.repository.key || here.common_dir != manifest.repository.common_dir {
        return Err(crate::i18n::text("worktree-cleanup-restore-other-repository").to_owned());
    }
    let root = top_folder(dest)?;
    verify(&archive.zip, manifest)?;
    let file = std::fs::File::open(&archive.zip).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut report = RestoreReport::default();
    for entry in &manifest.files {
        let placed = checked_rel(&entry.path)
            .ok_or_else(|| crate::i18n::text("worktree-cleanup-restore-bad-path").to_owned())
            .and_then(|names| {
                let bytes = read_entry(&mut zip, &format!("files/{}", entry.path), entry.size)
                    .filter(|b| sha256_hex(b) == entry.sha256)
                    .ok_or_else(|| {
                        crate::i18n::text("worktree-cleanup-archive-unverified").to_owned()
                    })?;
                place(&root, &names, &bytes)
            });
        match placed {
            Ok(Placed::Created) => report.created.push(entry.path.clone()),
            Ok(Placed::Same) => report.same.push(entry.path.clone()),
            Ok(Placed::Renamed(name)) => report.renamed.push((entry.path.clone(), name)),
            Err(e) => report.failed.push((entry.path.clone(), e)),
        }
    }
    if !report.created.is_empty() || !report.renamed.is_empty() || report.failed.is_empty() {
        let restored = Restored {
            at: crate::git::now(),
            into: dest.to_string_lossy().into_owned(),
            created: report.created.clone(),
            renamed: report.renamed.clone(),
        };
        report.manifest = Some(store.record(manifest, restored)?);
    }
    Ok(report)
}

/// `dest` as the top folder of its worktree, resolved; refused when it is a folder inside one.
fn top_folder(dest: &Path) -> Result<PathBuf, String> {
    let top = crate::git::run(Some(dest), &["rev-parse", "--show-toplevel"])?;
    let top = Path::new(top.trim())
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let dest = dest.canonicalize().map_err(|e| e.to_string())?;
    if top != dest {
        return Err(crate::i18n::text("worktree-cleanup-restore-not-top").to_owned());
    }
    Ok(dest)
}

enum Placed {
    Created,
    Same,
    Renamed(String),
}

/// Writes `bytes` as the file `names` of `root` (resolved), never over anything there.
fn place(root: &Path, names: &[&str], bytes: &[u8]) -> Result<Placed, String> {
    let blocked = || crate::i18n::text("worktree-cleanup-restore-blocked").to_owned();
    let (last, folders) = names.split_last().ok_or_else(blocked)?;
    let mut dir = root.to_path_buf();
    for name in folders {
        dir.push(name);
        match std::fs::symlink_metadata(&dir) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Err(blocked()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let inside = |dir: &Path| dir.canonicalize().is_ok_and(|real| real.starts_with(root));
    if !inside(&dir) {
        return Err(blocked());
    }
    let rel = names.join("/");
    let sha = sha256_hex(bytes);
    for n in 0..100u32 {
        let name = match n {
            0 => (*last).to_owned(),
            1 => format!("{last}.restored"),
            n => format!("{last}.restored-{n}"),
        };
        let target = dir.join(&name);
        match std::fs::symlink_metadata(&target) {
            Ok(meta) if n == 0 && meta.is_file() => {
                let there = crate::safe_read::read_inside(root, &rel, bytes.len() as u64);
                if there.is_some_and(|b| sha256_hex(&b) == sha) {
                    return Ok(Placed::Same);
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match write_new(&target, bytes) {
                    Ok(()) if inside(&dir) => {}
                    Ok(()) => {
                        // A folder on the way was swapped for a link meanwhile.
                        let _ = std::fs::remove_file(&target);
                        return Err(blocked());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e.to_string()),
                }
                if n == 0 {
                    return Ok(Placed::Created);
                }
                let mut renamed = names[..names.len() - 1].to_vec();
                renamed.push(&name);
                return Ok(Placed::Renamed(renamed.join("/")));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(blocked())
}

/// A new file (never one already there, nor a link), written whole.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if written.is_err() {
        // Ours, made just now: not left half written.
        let _ = std::fs::remove_file(path);
    }
    written
}
