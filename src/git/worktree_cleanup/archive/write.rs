//! Making an archive: each file read again (never through a link) and checked against what
//! was looked at, written to a ZIP beside its final name, read back and verified, held to the
//! storage limit, and only then put in place with its manifest.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

use super::{Entry, Listed, MANIFEST_VERSION, Manifest, Repository, Store, write_json};
use crate::git::worktree_cleanup::files::{MAX_TEXT, human, sha256_hex};
use crate::git::worktree_cleanup::{Kind, Untracked};

/// What is archived, and from where.
pub(crate) struct Source<'a> {
    pub repository: &'a Repository,
    pub worktree: &'a Path,
    pub branch: &'a str,
    pub head: &'a str,
    pub files: &'a [Untracked],
}

/// Archives `source.files` (all text, as looked at) into `store`; nothing is left behind when
/// it fails.
pub(crate) fn create(store: &Store, source: &Source<'_>) -> Result<Listed, String> {
    let folder = store.folder(&source.repository.key)?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let mut manifest = Manifest {
        version: MANIFEST_VERSION,
        id: String::new(),
        created: crate::git::now(),
        repository: source.repository.clone(),
        worktree: source.worktree.to_string_lossy().into_owned(),
        branch: source.branch.to_owned(),
        head: source.head.to_owned(),
        files: Vec::new(),
        restored: Vec::new(),
    };
    let (partial, file) = new_partial(store, &mut manifest)?;
    let guard = Partial(partial.clone());
    let written = write_zip(file, source, &mut manifest).and_then(|()| verify(&partial, &manifest));
    written?;
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    // The new ZIP is still a partial file, which the usage doesn't count.
    let zip_len = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
    let total = store.usage() + zip_len + json.len() as u64;
    if total > store.quota {
        return Err(crate::i18n::message(
            "worktree-cleanup-archive-quota",
            &[
                ("needed", human(total).into()),
                ("quota", human(store.quota).into()),
            ],
        ));
    }
    let (zip, json_path) = store.paths(&manifest.repository.key, &manifest.id)?;
    std::fs::rename(&partial, &zip).map_err(|e| e.to_string())?;
    // Moved: the guard finds nothing left to remove.
    drop(guard);
    if let Err(e) = write_json(&json_path, &manifest) {
        let _ = std::fs::remove_file(&zip);
        return Err(e);
    }
    let bytes = std::fs::metadata(&zip).map(|m| m.len()).unwrap_or(0) + json.len() as u64;
    Ok(Listed {
        manifest,
        zip,
        bytes,
    })
}

/// Removes a half-made archive unless it was put in place.
struct Partial(PathBuf);

impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A fresh archive id and its partial file, made here and nowhere else.
fn new_partial(store: &Store, manifest: &mut Manifest) -> Result<(PathBuf, std::fs::File), String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for attempt in 0..100u32 {
        let seed = format!(
            "{}\0{}\0{}\0{nanos}\0{attempt}",
            manifest.worktree, manifest.branch, manifest.head
        );
        let id = format!("{}-{}", manifest.created, &sha256_hex(seed.as_bytes())[..8]);
        let (zip, _) = store.paths(&manifest.repository.key, &id)?;
        let partial = zip.with_extension("zip.partial");
        if zip.exists() {
            continue;
        }
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
        {
            Ok(file) => {
                manifest.id = id;
                return Ok((partial, file));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(crate::i18n::text("worktree-cleanup-archive-no-name").to_owned())
}

fn write_zip(
    file: std::fs::File,
    source: &Source<'_>,
    manifest: &mut Manifest,
) -> Result<(), String> {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = zip::ZipWriter::new(file);
    for untracked in source.files {
        let Kind::Text(expected) = &untracked.kind else {
            return Err(not_text(&untracked.path));
        };
        let bytes = crate::safe_read::read_inside(source.worktree, &untracked.path, MAX_TEXT)
            .ok_or_else(|| changed(&untracked.path))?;
        let sha256 = sha256_hex(&bytes);
        if &sha256 != expected || bytes.contains(&0) {
            return Err(changed(&untracked.path));
        }
        zip.start_file(format!("files/{}", untracked.path), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
        manifest.files.push(Entry {
            path: untracked.path.clone(),
            size: bytes.len() as u64,
            sha256,
        });
    }
    let json = serde_json::to_vec_pretty(&*manifest).map_err(|e| e.to_string())?;
    zip.start_file("manifest.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&json).map_err(|e| e.to_string())?;
    let file = zip.finish().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

/// Reads the archive back: every file there, of the size and checksum the manifest says, and
/// nothing else.
pub(super) fn verify(zip: &Path, manifest: &Manifest) -> Result<(), String> {
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let broken = || crate::i18n::text("worktree-cleanup-archive-unverified").to_owned();
    if archive.len() != manifest.files.len() + 1 {
        return Err(broken());
    }
    let inner = read_entry(&mut archive, "manifest.json", 64 << 20).ok_or_else(broken)?;
    let inner: Manifest = serde_json::from_slice(&inner).map_err(|_| broken())?;
    if inner.files != manifest.files || inner.id != manifest.id {
        return Err(broken());
    }
    for entry in &manifest.files {
        let bytes = read_entry(&mut archive, &format!("files/{}", entry.path), entry.size)
            .ok_or_else(broken)?;
        if bytes.len() as u64 != entry.size || sha256_hex(&bytes) != entry.sha256 {
            return Err(broken());
        }
    }
    Ok(())
}

/// An entry's bytes, at most `max` of them.
pub(super) fn read_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
    max: u64,
) -> Option<Vec<u8>> {
    let entry = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    entry.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}

fn changed(path: &str) -> String {
    crate::i18n::message(
        "worktree-cleanup-archive-changed",
        &[("path", path.to_owned().into())],
    )
}

fn not_text(path: &str) -> String {
    crate::i18n::message(
        "worktree-cleanup-archive-not-text",
        &[("path", path.to_owned().into())],
    )
}
