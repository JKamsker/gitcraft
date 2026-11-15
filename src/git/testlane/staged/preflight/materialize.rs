//! Raw blobs only: no checkout/filter/encoding or symlink machinery.
use super::{Entry, fail};
use crate::git;
use std::{fs::File, io::Write, path::Path};

pub(super) fn write(repo: &Path, root: &Path, entries: &[Entry]) -> Result<(), String> {
    for entry in entries {
        crate::processes::deadline::check()?;
        if !matches!(entry.mode.as_str(), "100644" | "100755") {
            return Err(fail("staged-links"));
        }
        let bytes = git::command::run_inert_objects(
            repo,
            &["cat-file", "blob", &entry.oid],
            None,
            entry.size as usize + 1,
            &[0],
        )?;
        if bytes.len() as u64 != entry.size {
            return Err(fail("staged-object"));
        }
        // Git hashes `blob <len>\0` + these exact bytes using this repository's
        // SHA-1/SHA-256 object format. No -w, path, filters or object-store writes.
        let hashed = git::command::run_inert_objects(
            repo,
            &["hash-object", "--no-filters", "-t", "blob", "--stdin"],
            Some(&bytes),
            128,
            &[0],
        )?;
        if std::str::from_utf8(&hashed).map(str::trim).ok() != Some(entry.oid.as_str()) {
            return Err(fail("staged-object"));
        }
        let path = root.join(&entry.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        for chunk in bytes.chunks(8192) {
            crate::processes::deadline::check()?;
            file.write_all(chunk).map_err(|e| e.to_string())?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if entry.mode == "100755" { 0o755 } else { 0o644 };
            file.set_permissions(std::fs::Permissions::from_mode(mode))
                .map_err(|_| fail("staged-paths"))?;
        }
    }
    Ok(())
}
